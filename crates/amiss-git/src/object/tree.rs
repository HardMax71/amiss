use std::cmp::Ordering;

use amiss_wire::controls::GitMode;
use amiss_wire::model::{ObjectFormat, Oid};

use super::{TreeEntry, hex};
use crate::Error;

/// Parses a tree body under `git-object-grammar`.
///
/// # Errors
///
/// `ObjectUnreadable` for any mode, name, width, ordering, or padding defect.
pub fn parse_tree(object_format: ObjectFormat, body: &[u8]) -> Result<Vec<TreeEntry>, Error> {
    let oid_width = match object_format {
        ObjectFormat::Sha1 => 20_usize,
        ObjectFormat::Sha256 => 32_usize,
    };
    let mut entries: Vec<TreeEntry> = Vec::new();
    let mut pos = 0_usize;
    while pos < body.len() {
        let rest = body.get(pos..).ok_or(Error::ObjectUnreadable)?;
        let space = rest
            .iter()
            .position(|&b| b == b' ')
            .ok_or(Error::ObjectUnreadable)?;
        let mode_bytes = rest.get(..space).ok_or(Error::ObjectUnreadable)?;
        let mode = tree_mode(mode_bytes)?;
        let after_mode = rest
            .get(space.saturating_add(1)..)
            .ok_or(Error::ObjectUnreadable)?;
        let nul = after_mode
            .iter()
            .position(|&b| b == 0)
            .ok_or(Error::ObjectUnreadable)?;
        let name = after_mode.get(..nul).ok_or(Error::ObjectUnreadable)?;
        if name.is_empty() || name.contains(&b'/') || name == b"." || name == b".." {
            return Err(Error::ObjectUnreadable);
        }
        let oid_start = nul.saturating_add(1);
        let oid_end = oid_start.saturating_add(oid_width);
        let raw_oid = after_mode
            .get(oid_start..oid_end)
            .ok_or(Error::ObjectUnreadable)?;
        let oid = Oid::new(object_format, hex(raw_oid)).ok_or(Error::ObjectUnreadable)?;

        let is_tree = mode == GitMode::Tree;
        if let Some(previous) = entries.last()
            && tree_name_order(
                &previous.name,
                previous.mode == GitMode::Tree,
                name,
                is_tree,
            ) != Ordering::Less
        {
            return Err(Error::ObjectUnreadable);
        }
        if is_tree
            && entries
                .binary_search_by(|entry| {
                    tree_name_order(&entry.name, entry.mode == GitMode::Tree, name, false)
                })
                .is_ok()
        {
            return Err(Error::ObjectUnreadable);
        }

        entries.push(TreeEntry {
            mode,
            name: name.to_vec(),
            oid,
        });
        pos = pos
            .saturating_add(space)
            .saturating_add(1)
            .saturating_add(oid_end);
    }
    Ok(entries)
}

fn tree_name_order(
    left_name: &[u8],
    left_is_tree: bool,
    right_name: &[u8],
    right_is_tree: bool,
) -> Ordering {
    left_name
        .iter()
        .copied()
        .chain(left_is_tree.then_some(b'/'))
        .cmp(
            right_name
                .iter()
                .copied()
                .chain(right_is_tree.then_some(b'/')),
        )
}

fn tree_mode(bytes: &[u8]) -> Result<GitMode, Error> {
    match bytes {
        b"40000" => Ok(GitMode::Tree),
        b"100644" => Ok(GitMode::RegularFile),
        b"100755" => Ok(GitMode::ExecutableFile),
        b"120000" => Ok(GitMode::Symlink),
        b"160000" => Ok(GitMode::Gitlink),
        _ => Err(Error::ObjectUnreadable),
    }
}
