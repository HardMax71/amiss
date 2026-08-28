use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write as _};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use flate2::bufread::ZlibDecoder;

use super::{PackError, PackLimits, active};

const PACK_HEADER_BYTES: usize = 12;
const PACK_TRAILER_BYTES: usize = 20;
const INFLATE_BUFFER_BYTES: usize = 16_384;

pub(super) fn validate_and_spool(
    input: &mut dyn BufRead,
    spool: &mut File,
    limits: PackLimits,
    cancelled: &AtomicBool,
    started: Instant,
    timeout: Duration,
) -> Result<(), PackError> {
    let tracked = BoundedRead {
        input,
        spool,
        read: 0,
        limit: limits.pack_bytes,
        cancelled,
        started,
        timeout,
    };
    let mut input = BufReader::with_capacity(INFLATE_BUFFER_BYTES, tracked);
    let mut cursor = 0_u64;
    let object_count = pack_header(&mut input, &mut cursor)?;
    if object_count > limits.objects {
        return Err(PackError("the pack declares too many objects"));
    }

    let capacity = usize::try_from(object_count)
        .map_err(|_defect| PackError("the pack object count is unsupported"))?;
    let mut entries = Vec::with_capacity(capacity);
    let mut inflated = 0_u64;
    let mut resolved = 0_u64;
    for _ in 0..object_count {
        active(cancelled, started, timeout)?;
        let offset = cursor;
        let header = entry_header(&mut input, &mut cursor, limits.object_bytes)?;
        let base = match header.kind {
            EntryKind::Ordinary => None,
            EntryKind::OffsetDelta => {
                let base_offset = offset_base(&mut input, &mut cursor, offset)?;
                Some(
                    entries
                        .binary_search_by_key(&base_offset, |entry: &Entry| entry.offset)
                        .ok()
                        .and_then(|position| entries.get(position))
                        .copied()
                        .ok_or(PackError("an offset delta does not name an earlier entry"))?,
                )
            }
        };
        let stream = inflate(
            &mut input,
            header.declared_size,
            header.kind,
            cancelled,
            started,
            timeout,
        )?;
        cursor = cursor
            .checked_add(stream.compressed_bytes)
            .ok_or(PackError("the pack position overflows"))?;
        inflated = inflated
            .checked_add(stream.decompressed_bytes)
            .filter(|total| *total <= limits.inflated_bytes)
            .ok_or(PackError("the pack inflates beyond its byte limit"))?;

        let (resolved_size, depth) = match base {
            Some(base) => {
                if stream.delta_base_size != Some(base.resolved_size) {
                    return Err(PackError("a delta base size does not match its base"));
                }
                let result = stream
                    .delta_result_size
                    .ok_or(PackError("a delta omits its result size"))?;
                if result > limits.object_bytes {
                    return Err(PackError("a delta result exceeds the object byte limit"));
                }
                let depth = base
                    .depth
                    .checked_add(1)
                    .filter(|depth| *depth <= limits.delta_depth)
                    .ok_or(PackError("a delta chain exceeds its depth limit"))?;
                (result, depth)
            }
            None => (header.declared_size, 0),
        };
        resolved = resolved
            .checked_add(resolved_size)
            .filter(|total| *total <= limits.resolved_bytes)
            .ok_or(PackError("the pack resolves beyond its byte limit"))?;
        entries.push(Entry {
            offset,
            resolved_size,
            depth,
        });
    }

    let mut trailer = [0_u8; PACK_TRAILER_BYTES];
    read_exact(&mut input, &mut trailer, &mut cursor)?;
    let mut extra = [0_u8; 1];
    if input
        .read(&mut extra)
        .map_err(|_defect| PackError("the pack stream is unreadable"))?
        != 0
    {
        return Err(PackError("the pack has bytes after its checksum"));
    }
    active(cancelled, started, timeout)
}

fn pack_header(input: &mut impl Read, cursor: &mut u64) -> Result<u32, PackError> {
    let mut header = [0_u8; PACK_HEADER_BYTES];
    read_exact(input, &mut header, cursor)?;
    let [
        magic_0,
        magic_1,
        magic_2,
        magic_3,
        version_0,
        version_1,
        version_2,
        version_3,
        count_0,
        count_1,
        count_2,
        count_3,
    ] = header;
    if [magic_0, magic_1, magic_2, magic_3] != *b"PACK"
        || u32::from_be_bytes([version_0, version_1, version_2, version_3]) != 2
    {
        return Err(PackError("the pack header is unsupported"));
    }
    Ok(u32::from_be_bytes([count_0, count_1, count_2, count_3]))
}

#[derive(Clone, Copy)]
enum EntryKind {
    Ordinary,
    OffsetDelta,
}

struct EntryHeader {
    kind: EntryKind,
    declared_size: u64,
}

fn entry_header(
    input: &mut impl Read,
    cursor: &mut u64,
    object_limit: u64,
) -> Result<EntryHeader, PackError> {
    let mut byte = read_byte(input, cursor)?;
    let kind = match (byte >> 4) & 7 {
        1..=4 => EntryKind::Ordinary,
        6 => EntryKind::OffsetDelta,
        7 => return Err(PackError("reference deltas are not accepted")),
        _ => return Err(PackError("the pack entry type is unsupported")),
    };
    let mut declared_size = u64::from(byte & 15);
    let mut shift = 4_u32;
    while byte & 128 != 0 {
        byte = read_byte(input, cursor)?;
        let part = u64::from(byte & 127)
            .checked_shl(shift)
            .ok_or(PackError("the pack entry size overflows"))?;
        declared_size = declared_size
            .checked_add(part)
            .filter(|size| *size <= object_limit)
            .ok_or(PackError("a pack entry exceeds the object byte limit"))?;
        shift = shift
            .checked_add(7)
            .filter(|shift| *shift < 64)
            .ok_or(PackError("the pack entry size overflows"))?;
    }
    if declared_size > object_limit {
        return Err(PackError("a pack entry exceeds the object byte limit"));
    }
    Ok(EntryHeader {
        kind,
        declared_size,
    })
}

fn offset_base(
    input: &mut impl Read,
    cursor: &mut u64,
    entry_offset: u64,
) -> Result<u64, PackError> {
    let mut byte = read_byte(input, cursor)?;
    let mut distance = u64::from(byte & 127);
    while byte & 128 != 0 {
        byte = read_byte(input, cursor)?;
        distance = distance
            .checked_add(1)
            .and_then(|value| value.checked_shl(7))
            .and_then(|value| value.checked_add(u64::from(byte & 127)))
            .ok_or(PackError("the delta base offset overflows"))?;
    }
    entry_offset
        .checked_sub(distance)
        .filter(|base| *base < entry_offset)
        .ok_or(PackError("the delta base offset is invalid"))
}

#[derive(Clone, Copy)]
struct Entry {
    offset: u64,
    resolved_size: u64,
    depth: u16,
}

struct InflatedStream {
    compressed_bytes: u64,
    decompressed_bytes: u64,
    delta_base_size: Option<u64>,
    delta_result_size: Option<u64>,
}

fn inflate(
    input: &mut impl BufRead,
    declared_size: u64,
    kind: EntryKind,
    cancelled: &AtomicBool,
    started: Instant,
    timeout: Duration,
) -> Result<InflatedStream, PackError> {
    let mut decoder = ZlibDecoder::new(input);
    let mut output = [0_u8; INFLATE_BUFFER_BYTES];
    let mut decompressed = 0_u64;
    let mut delta = DeltaHeader {
        base: None,
        result: None,
        value: 0,
        shift: 0,
    };
    loop {
        active(cancelled, started, timeout)?;
        let read = decoder
            .read(&mut output)
            .map_err(|_defect| PackError("the pack stream is unreadable"))?;
        if read == 0 {
            break;
        }
        let read =
            u64::try_from(read).map_err(|_defect| PackError("the inflated size is unsupported"))?;
        decompressed = decompressed
            .checked_add(read)
            .filter(|size| *size <= declared_size)
            .ok_or(PackError("an entry inflates beyond its declared size"))?;
        if matches!(kind, EntryKind::OffsetDelta) {
            consume_delta(
                &mut delta,
                output
                    .get(
                        ..usize::try_from(read)
                            .map_err(|_defect| PackError("the inflated size is unsupported"))?,
                    )
                    .ok_or(PackError("the inflate buffer is inconsistent"))?,
            )?;
        }
    }
    if decompressed != declared_size {
        return Err(PackError("an entry does not match its declared size"));
    }
    Ok(InflatedStream {
        compressed_bytes: decoder.total_in(),
        decompressed_bytes: decompressed,
        delta_base_size: delta.base,
        delta_result_size: delta.result,
    })
}

struct DeltaHeader {
    base: Option<u64>,
    result: Option<u64>,
    value: u64,
    shift: u32,
}

fn consume_delta(header: &mut DeltaHeader, bytes: &[u8]) -> Result<(), PackError> {
    for byte in bytes.iter().copied() {
        if header.result.is_some() {
            break;
        }
        let part = u64::from(byte & 127)
            .checked_shl(header.shift)
            .ok_or(PackError("a delta size overflows"))?;
        header.value = header
            .value
            .checked_add(part)
            .ok_or(PackError("a delta size overflows"))?;
        if byte & 128 == 0 {
            if header.base.is_none() {
                header.base = Some(header.value);
                header.value = 0;
                header.shift = 0;
            } else {
                header.result = Some(header.value);
            }
        } else {
            header.shift = header
                .shift
                .checked_add(7)
                .filter(|shift| *shift < 64)
                .ok_or(PackError("a delta size overflows"))?;
        }
    }
    Ok(())
}

struct BoundedRead<'a> {
    input: &'a mut dyn BufRead,
    spool: &'a mut File,
    read: u64,
    limit: u64,
    cancelled: &'a AtomicBool,
    started: Instant,
    timeout: Duration,
}

impl Read for BoundedRead<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        active(self.cancelled, self.started, self.timeout)
            .map_err(|defect| std::io::Error::other(defect.to_string()))?;
        let remaining = self.limit.saturating_sub(self.read);
        if remaining == 0 {
            return if self.input.fill_buf()?.is_empty() {
                Ok(0)
            } else {
                Err(std::io::Error::other("the pack exceeds its byte limit"))
            };
        }
        let available = usize::try_from(remaining)
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        let read = self.input.read(
            buffer
                .get_mut(..available)
                .ok_or_else(|| std::io::Error::other("the pack buffer is inconsistent"))?,
        )?;
        self.spool.write_all(
            buffer
                .get(..read)
                .ok_or_else(|| std::io::Error::other("the pack buffer is inconsistent"))?,
        )?;
        self.read = self
            .read
            .saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        Ok(read)
    }
}

fn read_exact(input: &mut impl Read, buffer: &mut [u8], cursor: &mut u64) -> Result<(), PackError> {
    input
        .read_exact(buffer)
        .map_err(|_defect| PackError("the pack stream is unreadable"))?;
    *cursor = cursor
        .checked_add(
            u64::try_from(buffer.len())
                .map_err(|_defect| PackError("the pack position is unsupported"))?,
        )
        .ok_or(PackError("the pack position overflows"))?;
    Ok(())
}

fn read_byte(input: &mut impl Read, cursor: &mut u64) -> Result<u8, PackError> {
    let mut byte = [0_u8; 1];
    read_exact(input, &mut byte, cursor)?;
    let [byte] = byte;
    Ok(byte)
}
