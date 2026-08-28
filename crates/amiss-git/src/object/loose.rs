use amiss_wire::controls::ResourceName;
use amiss_wire::model::{ObjectFormat, Oid};
use flate2::{Decompress, FlushDecompress, Status};

use super::{Object, ObjectKind, discard_to_unreadable, verify_oid};
use crate::Error;
use crate::resources::ValueCap;

/// Inflates, grammar-checks, and hash-verifies one loose zlib stream.
///
/// # Errors
///
/// `ObjectUnreadable` for any header, stream, or digest defect;
/// `ResourceLimit` when the declared size crosses `inflated_cap` digitwise.
pub fn decode_loose(
    compressed: &[u8],
    object_format: ObjectFormat,
    oid: &Oid,
    inflated_cap: u64,
    value_cap: Option<&ValueCap>,
) -> Result<Object, Error> {
    let mut inflater = Decompress::new(true);
    decode_loose_reusing(
        &mut inflater,
        compressed,
        object_format,
        oid,
        inflated_cap,
        value_cap,
    )
}

pub(crate) fn decode_loose_reusing(
    inflater: &mut Decompress,
    compressed: &[u8],
    object_format: ObjectFormat,
    oid: &Oid,
    inflated_cap: u64,
    value_cap: Option<&ValueCap>,
) -> Result<Object, Error> {
    inflater.reset(true);
    let mut stream = ZlibStream {
        inflater,
        compressed,
        consumed: 0,
        finished: false,
    };
    let header = read_header(&mut stream, inflated_cap, value_cap)?;
    let mut body = vec![0_u8; header.size];
    fill_exact(&mut stream, &mut body)?;
    let mut probe = [0_u8; 1];
    if read_zlib(&mut stream, &mut probe)? != 0 {
        return Err(Error::ObjectUnreadable);
    }
    if stream.inflater.total_in()
        != u64::try_from(compressed.len()).map_err(discard_to_unreadable)?
    {
        return Err(Error::ObjectUnreadable);
    }
    verify_oid(object_format, oid, &header.raw, &body)?;
    Ok(Object {
        kind: header.kind,
        body,
    })
}

struct ZlibStream<'a> {
    inflater: &'a mut Decompress,
    compressed: &'a [u8],
    consumed: usize,
    finished: bool,
}

fn read_zlib(stream: &mut ZlibStream<'_>, output: &mut [u8]) -> Result<usize, Error> {
    if output.is_empty() || stream.finished {
        return Ok(0);
    }
    loop {
        let input = stream
            .compressed
            .get(stream.consumed..)
            .ok_or(Error::ObjectUnreadable)?;
        let input_before = stream.inflater.total_in();
        let output_before = stream.inflater.total_out();
        let flush = if input.is_empty() {
            FlushDecompress::Finish
        } else {
            FlushDecompress::None
        };
        let status = stream
            .inflater
            .decompress(input, output, flush)
            .map_err(discard_to_unreadable)?;
        let consumed = stream
            .inflater
            .total_in()
            .checked_sub(input_before)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(Error::ObjectUnreadable)?;
        let written = stream
            .inflater
            .total_out()
            .checked_sub(output_before)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(Error::ObjectUnreadable)?;
        stream.consumed = stream
            .consumed
            .checked_add(consumed)
            .ok_or(Error::ObjectUnreadable)?;
        stream.finished = status == Status::StreamEnd;
        if written != 0 || stream.finished || input.is_empty() {
            return Ok(written);
        }
        if consumed == 0 {
            return Err(Error::ObjectUnreadable);
        }
    }
}

struct Header {
    kind: ObjectKind,
    size: usize,
    raw: Vec<u8>,
}

fn read_header(
    stream: &mut ZlibStream<'_>,
    inflated_cap: u64,
    value_cap: Option<&ValueCap>,
) -> Result<Header, Error> {
    const MAX_SAFE: u64 = 9_007_199_254_740_991;
    let mut raw: Vec<u8> = Vec::new();
    let mut token: Vec<u8> = Vec::new();
    let kind = loop {
        let byte = next_byte(stream)?;
        raw.push(byte);
        if byte == b' ' {
            break match token.as_slice() {
                b"blob" => Some(ObjectKind::Blob),
                b"commit" => Some(ObjectKind::Commit),
                b"tag" => Some(ObjectKind::Tag),
                b"tree" => Some(ObjectKind::Tree),
                _ => None,
            }
            .ok_or(Error::ObjectUnreadable)?;
        }
        token.push(byte);
        if token.len() > 6 {
            return Err(Error::ObjectUnreadable);
        }
    };

    let mut value: u64 = 0;
    let mut digits: usize = 0;
    let mut leading_zero = false;
    let mut saturated = false;
    loop {
        let byte = next_byte(stream)?;
        raw.push(byte);
        if byte == 0 {
            break;
        }
        if !byte.is_ascii_digit() || leading_zero {
            return Err(Error::ObjectUnreadable);
        }
        if digits == 0 && byte == b'0' {
            leading_zero = true;
        }
        digits = digits.saturating_add(1);
        value = value
            .saturating_mul(10)
            .saturating_add(u64::from(byte.wrapping_sub(b'0')));
        if value > MAX_SAFE {
            value = MAX_SAFE;
            saturated = true;
            break;
        }
    }
    if digits == 0 && !saturated {
        return Err(Error::ObjectUnreadable);
    }
    if let Some(cap) = value_cap
        && value > cap.limit
    {
        return Err(Error::ResourceLimit {
            resource: cap.resource,
            configured_limit: cap.limit,
            observed_lower_bound: value,
        });
    }
    if value > inflated_cap {
        return Err(Error::ResourceLimit {
            resource: ResourceName::GitObjectBytes,
            configured_limit: inflated_cap,
            observed_lower_bound: value,
        });
    }
    let size = usize::try_from(value).map_err(discard_to_unreadable)?;
    Ok(Header { kind, size, raw })
}

fn next_byte(stream: &mut ZlibStream<'_>) -> Result<u8, Error> {
    let mut buf = [0_u8; 1];
    if read_zlib(stream, &mut buf)? == 1 {
        buf.first().copied().ok_or(Error::ObjectUnreadable)
    } else {
        Err(Error::ObjectUnreadable)
    }
}

fn fill_exact(stream: &mut ZlibStream<'_>, body: &mut [u8]) -> Result<(), Error> {
    let mut filled = 0_usize;
    while filled < body.len() {
        let target = body.get_mut(filled..).ok_or(Error::ObjectUnreadable)?;
        let read = read_zlib(stream, target)?;
        if read == 0 {
            return Err(Error::ObjectUnreadable);
        }
        filled = filled.saturating_add(read);
    }
    Ok(())
}
