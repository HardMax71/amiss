use amiss_wire::model::{ObjectFormat, Oid};

use super::Commit;
use crate::Error;

/// Parses a commit body's headers under `git-object-grammar`; the message
/// stays opaque.
///
/// # Errors
///
/// `ObjectUnreadable` for any header order, multiplicity, byte, or
/// continuation defect.
pub fn parse_commit(object_format: ObjectFormat, body: &[u8]) -> Result<Commit, Error> {
    let mut headers = body;

    let first = next_line(&mut headers)?;
    let tree_hex = first
        .strip_prefix(b"tree ")
        .ok_or(Error::ObjectUnreadable)?;
    let tree = header_oid(object_format, tree_hex)?;

    let mut parents: Vec<Oid> = Vec::new();
    let mut line = next_line(&mut headers)?;
    while let Some(parent_hex) = line.strip_prefix(b"parent ") {
        parents.push(header_oid(object_format, parent_hex)?);
        line = next_line(&mut headers)?;
    }

    let author = line
        .strip_prefix(b"author ")
        .ok_or(Error::ObjectUnreadable)?;
    if author.is_empty() {
        return Err(Error::ObjectUnreadable);
    }
    let committer_line = next_line(&mut headers)?;
    let committer = committer_line
        .strip_prefix(b"committer ")
        .ok_or(Error::ObjectUnreadable)?;
    if committer.is_empty() {
        return Err(Error::ObjectUnreadable);
    }

    let mut seen_extension = false;
    loop {
        let line = next_line(&mut headers)?;
        if line.is_empty() {
            break;
        }
        if line.first() == Some(&b' ') {
            if !seen_extension {
                return Err(Error::ObjectUnreadable);
            }
            continue;
        }
        let space = line
            .iter()
            .position(|&b| b == b' ')
            .ok_or(Error::ObjectUnreadable)?;
        let key = line.get(..space).ok_or(Error::ObjectUnreadable)?;
        let key_ok = !key.is_empty()
            && key
                .iter()
                .all(|&b| b.is_ascii() && !b.is_ascii_control() && b != b' ')
            && !matches!(key, b"tree" | b"parent" | b"author" | b"committer");
        if !key_ok {
            return Err(Error::ObjectUnreadable);
        }
        seen_extension = true;
    }

    Ok(Commit { tree, parents })
}

fn next_line<'a>(remaining: &mut &'a [u8]) -> Result<&'a [u8], Error> {
    let rest = *remaining;
    let lf = rest
        .iter()
        .position(|&b| b == b'\n')
        .ok_or(Error::ObjectUnreadable)?;
    let line = rest.get(..lf).ok_or(Error::ObjectUnreadable)?;
    if line.iter().any(|&b| b == 0 || b == b'\r') {
        return Err(Error::ObjectUnreadable);
    }
    *remaining = rest
        .get(lf.saturating_add(1)..)
        .ok_or(Error::ObjectUnreadable)?;
    Ok(line)
}

fn header_oid(object_format: ObjectFormat, hex_bytes: &[u8]) -> Result<Oid, Error> {
    let text = std::str::from_utf8(hex_bytes).map_err(|_defect| Error::ObjectUnreadable)?;
    Oid::new(object_format, text.to_owned()).ok_or(Error::ObjectUnreadable)
}
