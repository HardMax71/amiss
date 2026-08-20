use amiss_wire::model::ObjectFormat;

use crate::Error;
use crate::object::{discard_to_unreadable, ordinary_digest};

use super::oid_width;

pub(super) struct ParsedIndex {
    pub(super) oids: Vec<u8>,
    pub(super) fanout_bounds: [u32; 257],
    pub(super) offsets: Vec<u64>,
    pub(super) crcs: Option<Vec<u32>>,
    pub(super) stored_pack_checksum: Vec<u8>,
}

pub(super) fn parse_index(bytes: &[u8], object_format: ObjectFormat) -> Result<ParsedIndex, Error> {
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

fn read_fanout(body: &[u8], at: usize) -> Result<([u32; 257], usize), Error> {
    let mut bounds = [0_u32; 257];
    let mut previous = 0_u32;
    for (bucket, upper) in bounds.iter_mut().skip(1).enumerate() {
        let value = be32(body, at.saturating_add(bucket.saturating_mul(4)))?;
        if value < previous {
            return Err(Error::ObjectUnreadable);
        }
        previous = value;
        *upper = value;
    }
    Ok((bounds, at.saturating_add(1024)))
}

fn validate_oids(oids: &[u8], width: usize, fanout_bounds: &[u32; 257]) -> Result<(), Error> {
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
        let lower = *fanout_bounds.get(bucket).ok_or(Error::ObjectUnreadable)?;
        let upper = *fanout_bounds
            .get(bucket.saturating_add(1))
            .ok_or(Error::ObjectUnreadable)?;
        let row = u32::try_from(row).map_err(discard_to_unreadable)?;
        if row < lower || row >= upper {
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
    let (fanout_bounds, oids_at) = read_fanout(body, 8)?;
    let count = usize::try_from(*fanout_bounds.last().ok_or(Error::ObjectUnreadable)?)
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
    validate_oids(&oids, width, &fanout_bounds)?;

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
        fanout_bounds,
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
    let (fanout_bounds, entries_at) = read_fanout(body, 0)?;
    let count = usize::try_from(*fanout_bounds.last().ok_or(Error::ObjectUnreadable)?)
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
    validate_oids(&oids, width, &fanout_bounds)?;
    Ok(ParsedIndex {
        oids,
        fanout_bounds,
        offsets,
        crcs: None,
        stored_pack_checksum,
    })
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
