use amiss_wire::controls::GitMode;
use amiss_wire::model::{ObjectFormat, Oid};

use crate::Error;
use crate::object::{hex, ordinary_digest};

/// One supported stage-zero row of the logical index: the raw path bytes as
/// stored, the exact paired mode, the object in the declared namespace, and
/// the ordinary skip-worktree bit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexEntry {
    pub path: Vec<u8>,
    pub mode: GitMode,
    pub oid: Oid,
    pub skip_worktree: bool,
}

/// The complete logical stage-zero index: rows unique, path-byte sorted, and
/// prefix-free directly after parsing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogicalIndex {
    pub entries: Vec<IndexEntry>,
}

const STAGE_MASK: u16 = 0x3000;
const EXTENDED_FLAG: u16 = 0x4000;
const NAME_MASK: u16 = 0x0fff;
const SKIP_WORKTREE: u16 = 0x4000;
const INTENT_TO_ADD: u16 = 0x2000;
const EXTENDED_RESERVED: u16 = 0x8000;

fn be32(bytes: &[u8], at: usize) -> Result<u32, Error> {
    let raw = bytes
        .get(at..at.saturating_add(4))
        .ok_or(Error::IndexInvalid)?;
    let fixed: [u8; 4] = raw.try_into().map_err(|_short| Error::IndexInvalid)?;
    Ok(u32::from_be_bytes(fixed))
}

fn be16(bytes: &[u8], at: usize) -> Result<u16, Error> {
    let raw = bytes
        .get(at..at.saturating_add(2))
        .ok_or(Error::IndexInvalid)?;
    let fixed: [u8; 2] = raw.try_into().map_err(|_short| Error::IndexInvalid)?;
    Ok(u16::from_be_bytes(fixed))
}

fn entry_mode(raw: u32) -> Result<GitMode, Error> {
    match raw {
        0o100_644 => Ok(GitMode::RegularFile),
        0o100_755 => Ok(GitMode::ExecutableFile),
        0o120_000 => Ok(GitMode::Symlink),
        0o160_000 => Ok(GitMode::Gitlink),
        _ => Err(Error::IndexInvalid),
    }
}

/// A later path conflicts when some earlier row is one of its directory
/// prefixes; the sorted order guarantees the prefix would already have been
/// seen.
fn prefix_conflict(seen: &[Vec<u8>], path: &[u8]) -> bool {
    let mut end = path.len();
    while let Some(at) = path
        .get(..end)
        .and_then(|prefix| prefix.iter().rposition(|&byte| byte == b'/'))
    {
        let prefix = path.get(..at).unwrap_or_default();
        if seen
            .binary_search_by(|candidate| candidate.as_slice().cmp(prefix))
            .is_ok()
        {
            return true;
        }
        end = at;
    }
    false
}

/// Parses one complete raw `.git/index` byte string under `git-index-v1`:
/// versions two through four, ordinary stage-zero rows only, the exact mode
/// pairings, strictly increasing prefix-free paths, mandatory unknown or
/// split-index and sparse-directory extensions rejected, and the trailing
/// checksum verified in the declared namespace.
///
/// # Errors
///
/// `IndexUnmerged` for any nonzero stage, `IntentToAdd` for that bit, and
/// `IndexInvalid` for every structural defect.
pub fn parse_index_file(object_format: ObjectFormat, bytes: &[u8]) -> Result<LogicalIndex, Error> {
    let oid_width = match object_format {
        ObjectFormat::Sha1 => 20_usize,
        ObjectFormat::Sha256 => 32_usize,
    };
    let checksum_at = bytes
        .len()
        .checked_sub(oid_width)
        .ok_or(Error::IndexInvalid)?;
    let content = bytes.get(..checksum_at).ok_or(Error::IndexInvalid)?;
    let stored = bytes.get(checksum_at..).ok_or(Error::IndexInvalid)?;
    if ordinary_digest(object_format, content) != stored {
        return Err(Error::IndexInvalid);
    }
    if content.get(..4) != Some(b"DIRC") {
        return Err(Error::IndexInvalid);
    }
    let version = be32(content, 4)?;
    if !(2..=4).contains(&version) {
        return Err(Error::IndexInvalid);
    }
    let count = usize::try_from(be32(content, 8)?).map_err(|_wide| Error::IndexInvalid)?;

    let mut entries: Vec<IndexEntry> = Vec::new();
    let mut seen: Vec<Vec<u8>> = Vec::new();
    let mut previous_path: Vec<u8> = Vec::new();
    let mut at = 12_usize;
    for _entry in 0..count {
        let start = at;
        let mode = entry_mode(be32(content, start.saturating_add(24))?)?;
        let oid_start = start.saturating_add(40);
        let raw_oid = content
            .get(oid_start..oid_start.saturating_add(oid_width))
            .ok_or(Error::IndexInvalid)?;
        let oid = Oid::new(object_format, hex(raw_oid)).ok_or(Error::IndexInvalid)?;
        let flags_at = oid_start.saturating_add(oid_width);
        let flags = be16(content, flags_at)?;
        if flags & STAGE_MASK != 0 {
            return Err(Error::IndexUnmerged);
        }
        let mut skip_worktree = false;
        let mut path_at = flags_at.saturating_add(2);
        if flags & EXTENDED_FLAG != 0 {
            if version == 2 {
                return Err(Error::IndexInvalid);
            }
            let extended = be16(content, path_at)?;
            if extended & EXTENDED_RESERVED != 0 {
                return Err(Error::IndexInvalid);
            }
            if extended & INTENT_TO_ADD != 0 {
                return Err(Error::IntentToAdd);
            }
            skip_worktree = extended & SKIP_WORKTREE != 0;
            path_at = path_at.saturating_add(2);
        }

        let (path, next) = entry_path(content, version, start, path_at, flags, &previous_path)?;
        at = next;

        if path.is_empty() {
            return Err(Error::IndexInvalid);
        }
        if !previous_path.is_empty() && previous_path.as_slice() >= path.as_slice() {
            return Err(Error::IndexInvalid);
        }
        if prefix_conflict(&seen, &path) {
            return Err(Error::IndexInvalid);
        }
        seen.push(path.clone());
        previous_path.clone_from(&path);
        entries.push(IndexEntry {
            path,
            mode,
            oid,
            skip_worktree,
        });
    }

    extensions(content, at)?;
    Ok(LogicalIndex { entries })
}

/// One entry's path and the offset of the next entry: version four strips a
/// varint prefix from the previous path and appends a terminated suffix with
/// no padding; earlier versions carry the whole path, its redundant length
/// bits, and one to eight terminating pad bytes to the eight-byte boundary.
fn entry_path(
    content: &[u8],
    version: u32,
    start: usize,
    path_at: usize,
    flags: u16,
    previous_path: &[u8],
) -> Result<(Vec<u8>, usize), Error> {
    if version == 4 {
        let (strip, after) = varint(content, path_at)?;
        let nul = content
            .get(after..)
            .and_then(|rest| rest.iter().position(|&byte| byte == 0))
            .ok_or(Error::IndexInvalid)?;
        let suffix = content
            .get(after..after.saturating_add(nul))
            .ok_or(Error::IndexInvalid)?;
        let keep = previous_path
            .len()
            .checked_sub(usize::try_from(strip).map_err(|_wide| Error::IndexInvalid)?)
            .ok_or(Error::IndexInvalid)?;
        let mut path = previous_path.get(..keep).unwrap_or_default().to_vec();
        path.extend_from_slice(suffix);
        return Ok((path, after.saturating_add(nul).saturating_add(1)));
    }
    let nul = content
        .get(path_at..)
        .and_then(|rest| rest.iter().position(|&byte| byte == 0))
        .ok_or(Error::IndexInvalid)?;
    let path = content
        .get(path_at..path_at.saturating_add(nul))
        .ok_or(Error::IndexInvalid)?
        .to_vec();
    let name_bits = usize::from(flags & NAME_MASK);
    if name_bits < usize::from(NAME_MASK) && name_bits != path.len() {
        return Err(Error::IndexInvalid);
    }
    let unpadded = path_at.saturating_add(nul).saturating_sub(start);
    let remainder = unpadded.checked_rem(8).ok_or(Error::IndexInvalid)?;
    let pad = 8_usize.saturating_sub(remainder);
    let entry_end = start.saturating_add(unpadded).saturating_add(pad);
    for pad_at in path_at.saturating_add(nul)..entry_end {
        if content.get(pad_at) != Some(&0) {
            return Err(Error::IndexInvalid);
        }
    }
    Ok((path, entry_end))
}

/// Extensions after the rows: an uppercase-initial signature is optional and
/// skipped; split-index backing, sparse directories, and any other mandatory
/// unknown extension reject the index.
fn extensions(content: &[u8], mut at: usize) -> Result<(), Error> {
    while at < content.len() {
        let signature = content
            .get(at..at.saturating_add(4))
            .ok_or(Error::IndexInvalid)?;
        let length = usize::try_from(be32(content, at.saturating_add(4))?)
            .map_err(|_wide| Error::IndexInvalid)?;
        let payload_at = at.saturating_add(8);
        if payload_at.saturating_add(length) > content.len() {
            return Err(Error::IndexInvalid);
        }
        let optional = signature.first().is_some_and(u8::is_ascii_uppercase);
        if !optional {
            return Err(Error::IndexInvalid);
        }
        at = payload_at.saturating_add(length);
    }
    Ok(())
}

/// The offset varint of index version four.
fn varint(content: &[u8], at: usize) -> Result<(u64, usize), Error> {
    let mut value: u64 = 0;
    let mut cursor = at;
    loop {
        let byte = *content.get(cursor).ok_or(Error::IndexInvalid)?;
        cursor = cursor.saturating_add(1);
        value = value
            .checked_shl(7)
            .and_then(|shifted| shifted.checked_add(u64::from(byte & 0x7f)))
            .ok_or(Error::IndexInvalid)?;
        if byte & 0x80 == 0 {
            return Ok((value, cursor));
        }
        value = value.checked_add(1).ok_or(Error::IndexInvalid)?;
    }
}
