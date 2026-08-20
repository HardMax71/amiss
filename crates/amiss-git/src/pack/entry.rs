use std::io::Read as _;

use amiss_wire::controls::ResourceName;
use flate2::bufread::ZlibDecoder;

use crate::Error;
use crate::object::{ObjectKind, discard_to_unreadable};
use crate::resources::{ValueCap, crossing};

pub(crate) enum EntryKind {
    Object(ObjectKind),
    OffsetDelta,
    ReferenceDelta,
}

pub(crate) struct EntryHeader {
    pub(crate) kind: EntryKind,
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
    let kind = match type_code {
        1 => EntryKind::Object(ObjectKind::Commit),
        2 => EntryKind::Object(ObjectKind::Tree),
        3 => EntryKind::Object(ObjectKind::Blob),
        4 => EntryKind::Object(ObjectKind::Tag),
        6 => EntryKind::OffsetDelta,
        7 => EntryKind::ReferenceDelta,
        _ => return Err(Error::ObjectUnreadable),
    };
    Ok(EntryHeader {
        kind,
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
