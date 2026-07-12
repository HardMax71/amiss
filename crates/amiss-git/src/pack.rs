use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read as _;
use std::ops::Bound;
use std::os::fd::OwnedFd;
use std::os::unix::fs::FileExt as _;

use amiss_wire::controls::ResourceName;
use amiss_wire::model::ObjectFormat;
use flate2::bufread::ZlibDecoder;
use rustix::fs::{Mode, OFlags, openat};
use rustix::io::Errno;

use crate::Error;
use crate::object::{ObjectKind, discard_to_unreadable, ordinary_digest};
use crate::resources::{GitResources, ValueCap, crossing};

#[derive(Debug)]
pub(crate) struct PackSet {
    pub(crate) packs: Vec<Pack>,
    pub(crate) index_sizes: Vec<(String, u64)>,
}

#[derive(Debug)]
pub(crate) struct Pack {
    pub(crate) name_hex: String,
    file: File,
    width: usize,
    oids: Vec<u8>,
    rows_by_offset: BTreeMap<u64, usize>,
    offsets: Vec<u64>,
    crcs: Option<Vec<u32>>,
    data_end: u64,
}

struct ParsedIndex {
    oids: Vec<u8>,
    offsets: Vec<u64>,
    crcs: Option<Vec<u32>>,
    stored_pack_checksum: Vec<u8>,
}

fn dir_flags() -> OFlags {
    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::DIRECTORY | OFlags::CLOEXEC
}

fn file_flags() -> OFlags {
    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC
}

fn oid_width(object_format: ObjectFormat) -> usize {
    match object_format {
        ObjectFormat::Sha1 => 20,
        ObjectFormat::Sha256 => 32,
    }
}

pub(crate) fn build(
    objects: &OwnedFd,
    object_format: ObjectFormat,
    resources: &mut GitResources,
) -> Result<PackSet, Error> {
    let pack_dir = match openat(objects, "pack", dir_flags(), Mode::empty()) {
        Ok(fd) => fd,
        Err(errno) if errno == Errno::NOENT => {
            return Ok(PackSet {
                packs: Vec::new(),
                index_sizes: Vec::new(),
            });
        }
        Err(_) => return Err(Error::ObjectUnreadable),
    };

    let limits = resources.limits();
    let mut names: Vec<Vec<u8>> = Vec::new();
    let mut seen: u64 = 0;
    let dir = rustix::fs::Dir::read_from(&pack_dir).map_err(discard_to_unreadable)?;
    for entry in dir {
        let entry = entry.map_err(discard_to_unreadable)?;
        let name = entry.file_name().to_bytes().to_vec();
        if name == b"." || name == b".." {
            continue;
        }
        seen = seen.saturating_add(1);
        if seen > limits.pack_directory_entries {
            return Err(crossing(
                ResourceName::GitPackDirectoryEntries,
                limits.pack_directory_entries,
                seen,
            ));
        }
        names.push(name);
    }
    names.sort_unstable();

    let hex_len = oid_width(object_format).saturating_mul(2);
    let mut pairs: BTreeMap<String, (bool, bool)> = BTreeMap::new();
    for name in &names {
        let Some((hex_part, is_pack)) = classify(name, hex_len) else {
            continue;
        };
        let slot = pairs.entry(hex_part).or_insert((false, false));
        if is_pack {
            slot.0 = true;
        } else {
            slot.1 = true;
        }
    }
    if pairs.values().any(|(pack, idx)| !(*pack && *idx)) {
        return Err(Error::ObjectUnreadable);
    }
    let pair_count = u64::try_from(pairs.len()).unwrap_or(u64::MAX);
    if pair_count > limits.pack_files {
        return Err(crossing(
            ResourceName::GitPackFiles,
            limits.pack_files,
            pair_count,
        ));
    }

    let mut packs = Vec::new();
    let mut index_sizes = Vec::new();
    for name_hex in pairs.keys() {
        let (pack, index_bytes) = load_pack(&pack_dir, object_format, resources, name_hex)?;
        index_sizes.push((name_hex.clone(), index_bytes));
        packs.push(pack);
    }
    Ok(PackSet { packs, index_sizes })
}

fn classify(name: &[u8], hex_len: usize) -> Option<(String, bool)> {
    let rest = name.strip_prefix(b"pack-")?;
    let (hex_part, suffix) = match rest.strip_suffix(b".pack") {
        Some(stem) => (stem, true),
        None => (rest.strip_suffix(b".idx")?, false),
    };
    if hex_part.len() != hex_len
        || !hex_part
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    {
        return None;
    }
    let text = std::str::from_utf8(hex_part).ok()?;
    Some((text.to_owned(), suffix))
}

fn load_pack(
    pack_dir: &OwnedFd,
    object_format: ObjectFormat,
    resources: &mut GitResources,
    name_hex: &str,
) -> Result<(Pack, u64), Error> {
    let idx_fd = openat(
        pack_dir,
        format!("pack-{name_hex}.idx"),
        file_flags(),
        Mode::empty(),
    )
    .map_err(discard_to_unreadable)?;
    let idx_file = File::from(idx_fd);
    let idx_meta = idx_file.metadata().map_err(discard_to_unreadable)?;
    if !idx_meta.file_type().is_file() {
        return Err(Error::ObjectUnreadable);
    }
    resources.charge_index(name_hex, idx_meta.len())?;
    let mut idx_bytes = Vec::new();
    let cap = resources.limits().pack_index_bytes;
    let read = idx_file
        .take(cap.saturating_add(1))
        .read_to_end(&mut idx_bytes)
        .map_err(discard_to_unreadable)?;
    if u64::try_from(read).unwrap_or(u64::MAX) > cap {
        return Err(crossing(
            ResourceName::GitPackIndexBytes,
            cap,
            cap.saturating_add(1),
        ));
    }
    let parsed = parse_index(&idx_bytes, object_format)?;

    let pack_fd = openat(
        pack_dir,
        format!("pack-{name_hex}.pack"),
        file_flags(),
        Mode::empty(),
    )
    .map_err(discard_to_unreadable)?;
    let file = File::from(pack_fd);
    let meta = file.metadata().map_err(discard_to_unreadable)?;
    if !meta.file_type().is_file() {
        return Err(Error::ObjectUnreadable);
    }
    let width = oid_width(object_format);
    let trailer = u64::try_from(width).unwrap_or(u64::MAX);
    let size = meta.len();
    if size < 12_u64.saturating_add(trailer) {
        return Err(Error::ObjectUnreadable);
    }

    let mut header = [0_u8; 12];
    file.read_exact_at(&mut header, 0)
        .map_err(discard_to_unreadable)?;
    let (magic, rest) = header.split_at(4);
    let (version, count_bytes) = rest.split_at(4);
    if magic != b"PACK" {
        return Err(Error::ObjectUnreadable);
    }
    let version = u32::from_be_bytes(version.try_into().map_err(discard_to_unreadable)?);
    if version != 2 && version != 3 {
        return Err(Error::ObjectUnreadable);
    }
    let count = u32::from_be_bytes(count_bytes.try_into().map_err(discard_to_unreadable)?);
    if usize::try_from(count).map_err(discard_to_unreadable)? != parsed.offsets.len() {
        return Err(Error::ObjectUnreadable);
    }

    let mut trailer_bytes = vec![0_u8; width];
    file.read_exact_at(&mut trailer_bytes, size.saturating_sub(trailer))
        .map_err(discard_to_unreadable)?;
    if trailer_bytes != parsed.stored_pack_checksum {
        return Err(Error::ObjectUnreadable);
    }
    let name_raw = decode_hex(name_hex).ok_or(Error::ObjectUnreadable)?;
    if name_raw != trailer_bytes {
        return Err(Error::ObjectUnreadable);
    }

    let data_end = size.saturating_sub(trailer);
    let mut rows_by_offset = BTreeMap::new();
    for (row, offset) in parsed.offsets.iter().enumerate() {
        if *offset < 12 || *offset >= data_end {
            return Err(Error::ObjectUnreadable);
        }
        if rows_by_offset.insert(*offset, row).is_some() {
            return Err(Error::ObjectUnreadable);
        }
    }

    Ok((
        Pack {
            name_hex: name_hex.to_owned(),
            file,
            width,
            oids: parsed.oids,
            rows_by_offset,
            offsets: parsed.offsets,
            crcs: parsed.crcs,
            data_end,
        },
        idx_meta.len(),
    ))
}

fn parse_index(bytes: &[u8], object_format: ObjectFormat) -> Result<ParsedIndex, Error> {
    let width = oid_width(object_format);
    let split = bytes
        .len()
        .checked_sub(width)
        .ok_or(Error::ObjectUnreadable)?;
    let content = bytes.get(..split).ok_or(Error::ObjectUnreadable)?;
    let idx_checksum = bytes.get(split..).ok_or(Error::ObjectUnreadable)?;
    if ordinary_digest(object_format, content) != idx_checksum {
        return Err(Error::ObjectUnreadable);
    }
    let pack_ck_at = content
        .len()
        .checked_sub(width)
        .ok_or(Error::ObjectUnreadable)?;
    let stored_pack_checksum = content
        .get(pack_ck_at..)
        .ok_or(Error::ObjectUnreadable)?
        .to_vec();
    let body = content.get(..pack_ck_at).ok_or(Error::ObjectUnreadable)?;

    if body.get(..4) == Some(&[0xff, b't', b'O', b'c']) {
        parse_index_v2(body, width, stored_pack_checksum)
    } else {
        parse_index_v1(body, width, stored_pack_checksum)
    }
}

fn read_fanout(body: &[u8], at: usize) -> Result<(Vec<u64>, usize), Error> {
    let mut fanout = Vec::with_capacity(256);
    let mut previous = 0_u64;
    for bucket in 0..256_usize {
        let value = u64::from(be32(body, at.saturating_add(bucket.saturating_mul(4)))?);
        if value < previous {
            return Err(Error::ObjectUnreadable);
        }
        previous = value;
        fanout.push(value);
    }
    Ok((fanout, at.saturating_add(1024)))
}

fn validate_oids(oids: &[u8], width: usize, fanout: &[u64]) -> Result<(), Error> {
    let count = oids.len().checked_div(width).unwrap_or(0);
    let mut previous: Option<&[u8]> = None;
    for row in 0..count {
        let start = row.saturating_mul(width);
        let oid = oids
            .get(start..start.saturating_add(width))
            .ok_or(Error::ObjectUnreadable)?;
        if let Some(prev) = previous
            && prev >= oid
        {
            return Err(Error::ObjectUnreadable);
        }
        let bucket = usize::from(*oid.first().ok_or(Error::ObjectUnreadable)?);
        let lower = if bucket == 0 {
            0
        } else {
            *fanout
                .get(bucket.saturating_sub(1))
                .ok_or(Error::ObjectUnreadable)?
        };
        let upper = *fanout.get(bucket).ok_or(Error::ObjectUnreadable)?;
        let row_u64 = u64::try_from(row).map_err(discard_to_unreadable)?;
        if row_u64 < lower || row_u64 >= upper {
            return Err(Error::ObjectUnreadable);
        }
        previous = Some(oid);
    }
    Ok(())
}

fn parse_index_v2(
    body: &[u8],
    width: usize,
    stored_pack_checksum: Vec<u8>,
) -> Result<ParsedIndex, Error> {
    let version = be32(body, 4)?;
    if version != 2 {
        return Err(Error::ObjectUnreadable);
    }
    let (fanout, oids_at) = read_fanout(body, 8)?;
    let count = usize::try_from(*fanout.last().ok_or(Error::ObjectUnreadable)?)
        .map_err(discard_to_unreadable)?;
    let oids_len = count.saturating_mul(width);
    let crcs_at = oids_at.saturating_add(oids_len);
    let offsets_at = crcs_at.saturating_add(count.saturating_mul(4));
    let large_at = offsets_at.saturating_add(count.saturating_mul(4));
    let large_len = body
        .len()
        .checked_sub(large_at)
        .ok_or(Error::ObjectUnreadable)?;
    if !large_len.is_multiple_of(8) {
        return Err(Error::ObjectUnreadable);
    }
    let large_count = large_len.checked_div(8).unwrap_or(0);

    let oids = body
        .get(oids_at..crcs_at)
        .ok_or(Error::ObjectUnreadable)?
        .to_vec();
    validate_oids(&oids, width, &fanout)?;

    let mut crcs = Vec::with_capacity(count);
    for row in 0..count {
        crcs.push(be32(body, crcs_at.saturating_add(row.saturating_mul(4)))?);
    }

    let mut offsets = Vec::with_capacity(count);
    for row in 0..count {
        let raw = be32(body, offsets_at.saturating_add(row.saturating_mul(4)))?;
        if raw & 0x8000_0000 == 0 {
            offsets.push(u64::from(raw));
        } else {
            let index = usize::try_from(raw & 0x7fff_ffff).map_err(discard_to_unreadable)?;
            if index >= large_count {
                return Err(Error::ObjectUnreadable);
            }
            offsets.push(be64(
                body,
                large_at.saturating_add(index.saturating_mul(8)),
            )?);
        }
    }
    Ok(ParsedIndex {
        oids,
        offsets,
        crcs: Some(crcs),
        stored_pack_checksum,
    })
}

fn parse_index_v1(
    body: &[u8],
    width: usize,
    stored_pack_checksum: Vec<u8>,
) -> Result<ParsedIndex, Error> {
    let (fanout, entries_at) = read_fanout(body, 0)?;
    let count = usize::try_from(*fanout.last().ok_or(Error::ObjectUnreadable)?)
        .map_err(discard_to_unreadable)?;
    let stride = width.saturating_add(4);
    let expected = entries_at.saturating_add(count.saturating_mul(stride));
    if body.len() != expected {
        return Err(Error::ObjectUnreadable);
    }
    let mut oids = Vec::with_capacity(count.saturating_mul(width));
    let mut offsets = Vec::with_capacity(count);
    for row in 0..count {
        let at = entries_at.saturating_add(row.saturating_mul(stride));
        offsets.push(u64::from(be32(body, at)?));
        let oid = body
            .get(at.saturating_add(4)..at.saturating_add(stride))
            .ok_or(Error::ObjectUnreadable)?;
        oids.extend_from_slice(oid);
    }
    validate_oids(&oids, width, &fanout)?;
    Ok(ParsedIndex {
        oids,
        offsets,
        crcs: None,
        stored_pack_checksum,
    })
}

impl PackSet {
    pub(crate) fn locate(&self, oid_raw: &[u8]) -> Option<(usize, u64)> {
        for (pack_index, pack) in self.packs.iter().enumerate() {
            if let Some(row) = pack.find(oid_raw) {
                let offset = *pack.offsets.get(row)?;
                return Some((pack_index, offset));
            }
        }
        None
    }
}

impl Pack {
    fn find(&self, oid_raw: &[u8]) -> Option<usize> {
        let count = self.oids.len().checked_div(self.width)?;
        let mut low = 0_usize;
        let mut high = count;
        while low < high {
            let middle = low.midpoint(high);
            let start = middle.saturating_mul(self.width);
            let candidate = self.oids.get(start..start.saturating_add(self.width))?;
            match candidate.cmp(oid_raw) {
                std::cmp::Ordering::Less => low = middle.saturating_add(1),
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => return Some(middle),
            }
        }
        None
    }

    pub(crate) fn interval_end(&self, offset: u64) -> u64 {
        self.rows_by_offset
            .range((Bound::Excluded(offset), Bound::Unbounded))
            .next()
            .map_or(self.data_end, |(next, _)| *next)
    }

    pub(crate) fn row_at(&self, offset: u64) -> Option<usize> {
        self.rows_by_offset.get(&offset).copied()
    }

    pub(crate) fn read_interval(
        &self,
        resources: &mut GitResources,
        offset: u64,
    ) -> Result<Vec<u8>, Error> {
        let end = self.interval_end(offset);
        let length = end.checked_sub(offset).ok_or(Error::ObjectUnreadable)?;
        let member = format!("pack:{}:{offset}", self.name_hex);
        resources.charge_compressed(&member, length)?;
        let mut bytes = vec![0_u8; usize::try_from(length).map_err(discard_to_unreadable)?];
        self.file
            .read_exact_at(&mut bytes, offset)
            .map_err(discard_to_unreadable)?;
        if let (Some(crcs), Some(row)) = (&self.crcs, self.row_at(offset)) {
            let expected = *crcs.get(row).ok_or(Error::ObjectUnreadable)?;
            if crc32fast::hash(&bytes) != expected {
                return Err(Error::ObjectUnreadable);
            }
        }
        Ok(bytes)
    }
}

pub(crate) struct EntryHeader {
    pub(crate) type_code: u8,
    pub(crate) size: u64,
    pub(crate) header_len: usize,
}

pub(crate) fn parse_entry_header(bytes: &[u8]) -> Result<EntryHeader, Error> {
    let first = *bytes.first().ok_or(Error::ObjectUnreadable)?;
    let type_code = first.wrapping_shr(4) & 0x7;
    let mut size = u64::from(first & 0x0f);
    let mut shift = 4_u32;
    let mut position = 1_usize;
    let mut byte = first;
    while byte & 0x80 != 0 {
        byte = *bytes.get(position).ok_or(Error::ObjectUnreadable)?;
        if shift > 57 {
            return Err(Error::ObjectUnreadable);
        }
        size |= u64::from(byte & 0x7f).wrapping_shl(shift);
        shift = shift.saturating_add(7);
        position = position.saturating_add(1);
    }
    Ok(EntryHeader {
        type_code,
        size,
        header_len: position,
    })
}

pub(crate) fn parse_ofs_distance(bytes: &[u8]) -> Result<(u64, usize), Error> {
    let mut position = 0_usize;
    let mut byte = *bytes.first().ok_or(Error::ObjectUnreadable)?;
    let mut value = u64::from(byte & 0x7f);
    position = position.saturating_add(1);
    while byte & 0x80 != 0 {
        byte = *bytes.get(position).ok_or(Error::ObjectUnreadable)?;
        value = value
            .checked_add(1)
            .and_then(|v| v.checked_mul(128))
            .and_then(|v| v.checked_add(u64::from(byte & 0x7f)))
            .ok_or(Error::ObjectUnreadable)?;
        position = position.saturating_add(1);
    }
    Ok((value, position))
}

pub(crate) fn inflate_exact(data: &[u8], expected: u64, cap: u64) -> Result<Vec<u8>, Error> {
    if expected > cap {
        return Err(crossing(ResourceName::GitObjectBytes, cap, expected));
    }
    let mut decoder = ZlibDecoder::new(data);
    let mut out = vec![0_u8; usize::try_from(expected).map_err(discard_to_unreadable)?];
    let mut filled = 0_usize;
    while filled < out.len() {
        let target = out.get_mut(filled..).ok_or(Error::ObjectUnreadable)?;
        match decoder.read(target) {
            Ok(0) | Err(_) => return Err(Error::ObjectUnreadable),
            Ok(read) => filled = filled.saturating_add(read),
        }
    }
    let mut probe = [0_u8; 1];
    match decoder.read(&mut probe) {
        Ok(0) => {}
        Ok(_) | Err(_) => return Err(Error::ObjectUnreadable),
    }
    if decoder.total_in() != u64::try_from(data.len()).map_err(discard_to_unreadable)? {
        return Err(Error::ObjectUnreadable);
    }
    Ok(out)
}

pub(crate) fn apply_delta(
    base: &[u8],
    script: &[u8],
    cap: u64,
    value_cap: Option<&ValueCap>,
) -> Result<Vec<u8>, Error> {
    let (source_size, at) = leb128(script, 0)?;
    let (target_size, mut at) = leb128(script, at)?;
    if source_size != u64::try_from(base.len()).map_err(discard_to_unreadable)? {
        return Err(Error::ObjectUnreadable);
    }
    if let Some(value) = value_cap
        && target_size > value.limit
    {
        return Err(crossing(value.resource, value.limit, target_size));
    }
    if target_size > cap {
        return Err(crossing(ResourceName::GitObjectBytes, cap, target_size));
    }
    let target_len = usize::try_from(target_size).map_err(discard_to_unreadable)?;
    let mut out: Vec<u8> = Vec::with_capacity(target_len);
    while at < script.len() {
        let opcode = *script.get(at).ok_or(Error::ObjectUnreadable)?;
        at = at.saturating_add(1);
        if opcode & 0x80 != 0 {
            let mut offset = 0_u64;
            let mut size = 0_u64;
            for bit in 0..4_u32 {
                if opcode & (1_u8.wrapping_shl(bit)) != 0 {
                    let byte = *script.get(at).ok_or(Error::ObjectUnreadable)?;
                    at = at.saturating_add(1);
                    offset |= u64::from(byte).wrapping_shl(bit.saturating_mul(8));
                }
            }
            for bit in 0..3_u32 {
                if opcode & (0x10_u8.wrapping_shl(bit)) != 0 {
                    let byte = *script.get(at).ok_or(Error::ObjectUnreadable)?;
                    at = at.saturating_add(1);
                    size |= u64::from(byte).wrapping_shl(bit.saturating_mul(8));
                }
            }
            if size == 0 {
                size = 0x10000;
            }
            let start = usize::try_from(offset).map_err(discard_to_unreadable)?;
            let length = usize::try_from(size).map_err(discard_to_unreadable)?;
            let end = start.checked_add(length).ok_or(Error::ObjectUnreadable)?;
            let slice = base.get(start..end).ok_or(Error::ObjectUnreadable)?;
            out.extend_from_slice(slice);
        } else {
            if opcode == 0 {
                return Err(Error::ObjectUnreadable);
            }
            let length = usize::from(opcode);
            let end = at.checked_add(length).ok_or(Error::ObjectUnreadable)?;
            let literal = script.get(at..end).ok_or(Error::ObjectUnreadable)?;
            out.extend_from_slice(literal);
            at = end;
        }
        if out.len() > target_len {
            return Err(Error::ObjectUnreadable);
        }
    }
    if out.len() != target_len {
        return Err(Error::ObjectUnreadable);
    }
    Ok(out)
}

fn leb128(bytes: &[u8], mut at: usize) -> Result<(u64, usize), Error> {
    let mut value = 0_u64;
    let mut shift = 0_u32;
    loop {
        let byte = *bytes.get(at).ok_or(Error::ObjectUnreadable)?;
        at = at.saturating_add(1);
        if shift > 57 {
            return Err(Error::ObjectUnreadable);
        }
        value |= u64::from(byte & 0x7f).wrapping_shl(shift);
        shift = shift.saturating_add(7);
        if byte & 0x80 == 0 {
            return Ok((value, at));
        }
    }
}

pub(crate) fn kind_of(type_code: u8) -> Result<ObjectKind, Error> {
    ObjectKind::from_pack_type(type_code).ok_or(Error::ObjectUnreadable)
}

fn be32(bytes: &[u8], at: usize) -> Result<u32, Error> {
    let slice = bytes
        .get(at..at.saturating_add(4))
        .ok_or(Error::ObjectUnreadable)?;
    let array: [u8; 4] = slice.try_into().map_err(discard_to_unreadable)?;
    Ok(u32::from_be_bytes(array))
}

fn be64(bytes: &[u8], at: usize) -> Result<u64, Error> {
    let slice = bytes
        .get(at..at.saturating_add(8))
        .ok_or(Error::ObjectUnreadable)?;
    let array: [u8; 8] = slice.try_into().map_err(discard_to_unreadable)?;
    Ok(u64::from_be_bytes(array))
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(text.len().checked_div(2)?);
    for pair in text.as_bytes().chunks_exact(2) {
        let [high, low] = pair else { return None };
        out.push(hex_value(*high)?.wrapping_shl(4) | hex_value(*low)?);
    }
    Some(out)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte.wrapping_sub(b'0')),
        b'a'..=b'f' => Some(byte.wrapping_sub(b'a').wrapping_add(10)),
        _ => None,
    }
}
