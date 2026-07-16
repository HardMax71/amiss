use std::io::Read as _;

use amiss_wire::controls::{GitMode, ResourceName};
use amiss_wire::model::{ObjectFormat, Oid};
use flate2::bufread::ZlibDecoder;
use sha1_checked::Digest as _;
use sha2::Digest as _;

use crate::Error;
use crate::resources::ValueCap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Blob,
    Commit,
    Tag,
    Tree,
}

impl ObjectKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blob => "blob",
            Self::Commit => "commit",
            Self::Tag => "tag",
            Self::Tree => "tree",
        }
    }

    pub(crate) const fn from_pack_type(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Commit),
            2 => Some(Self::Tree),
            3 => Some(Self::Blob),
            4 => Some(Self::Tag),
            _ => None,
        }
    }

    fn from_token(token: &[u8]) -> Option<Self> {
        match token {
            b"blob" => Some(Self::Blob),
            b"commit" => Some(Self::Commit),
            b"tag" => Some(Self::Tag),
            b"tree" => Some(Self::Tree),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Object {
    pub kind: ObjectKind,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeEntry {
    pub mode: GitMode,
    pub name: Vec<u8>,
    pub oid: Oid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub tree: Oid,
    pub parents: Vec<Oid>,
}

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
    let mut decoder = ZlibDecoder::new(compressed);
    let header = read_header(&mut decoder, inflated_cap, value_cap)?;
    let mut body = vec![0_u8; header.size];
    fill_exact(&mut decoder, &mut body)?;
    let mut probe = [0_u8; 1];
    match decoder.read(&mut probe) {
        Ok(0) => {}
        Ok(_) | Err(_) => return Err(Error::ObjectUnreadable),
    }
    if decoder.total_in() != u64::try_from(compressed.len()).map_err(discard_to_unreadable)? {
        return Err(Error::ObjectUnreadable);
    }
    verify_oid(object_format, oid, &header.raw, &body)?;
    Ok(Object {
        kind: header.kind,
        body,
    })
}

struct Header {
    kind: ObjectKind,
    size: usize,
    raw: Vec<u8>,
}

fn read_header(
    decoder: &mut ZlibDecoder<&[u8]>,
    inflated_cap: u64,
    value_cap: Option<&ValueCap>,
) -> Result<Header, Error> {
    const MAX_SAFE: u64 = 9_007_199_254_740_991;
    let mut raw: Vec<u8> = Vec::new();
    let mut token: Vec<u8> = Vec::new();
    let kind = loop {
        let byte = next_byte(decoder)?;
        raw.push(byte);
        if byte == b' ' {
            break ObjectKind::from_token(&token).ok_or(Error::ObjectUnreadable)?;
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
        let byte = next_byte(decoder)?;
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

fn next_byte(decoder: &mut ZlibDecoder<&[u8]>) -> Result<u8, Error> {
    let mut buf = [0_u8; 1];
    match decoder.read(&mut buf) {
        Ok(1) => buf.first().copied().ok_or(Error::ObjectUnreadable),
        Ok(_) | Err(_) => Err(Error::ObjectUnreadable),
    }
}

fn fill_exact(decoder: &mut ZlibDecoder<&[u8]>, body: &mut [u8]) -> Result<(), Error> {
    let mut filled = 0_usize;
    while filled < body.len() {
        let target = body.get_mut(filled..).ok_or(Error::ObjectUnreadable)?;
        match decoder.read(target) {
            Ok(0) | Err(_) => return Err(Error::ObjectUnreadable),
            Ok(n) => filled = filled.saturating_add(n),
        }
    }
    Ok(())
}

pub(crate) fn discard_to_unreadable<T>(_defect: T) -> Error {
    Error::ObjectUnreadable
}

pub(crate) fn ordinary_digest(object_format: ObjectFormat, data: &[u8]) -> Vec<u8> {
    match object_format {
        ObjectFormat::Sha1 => {
            let mut hasher = sha1_checked::Sha1::builder()
                .detect_collision(false)
                .build();
            hasher.update(data);
            hasher.try_finalize().hash().to_vec()
        }
        ObjectFormat::Sha256 => {
            let mut hasher = sha2::Sha256::new();
            hasher.update(data);
            hasher.finalize().to_vec()
        }
    }
}

pub(crate) fn verify_oid(
    object_format: ObjectFormat,
    oid: &Oid,
    raw_header: &[u8],
    body: &[u8],
) -> Result<(), Error> {
    let actual = match object_format {
        ObjectFormat::Sha1 => {
            let mut hasher = sha1_checked::Sha1::builder()
                .detect_collision(true)
                .safe_hash(false)
                .use_ubc(true)
                .build();
            hasher.update(raw_header);
            hasher.update(body);
            let result = hasher.try_finalize();
            if result.has_collision() {
                return Err(Error::ObjectUnreadable);
            }
            hex(result.hash().as_slice())
        }
        ObjectFormat::Sha256 => {
            let mut hasher = sha2::Sha256::new();
            hasher.update(raw_header);
            hasher.update(body);
            hex(&hasher.finalize())
        }
    };
    if actual == oid.as_str() {
        Ok(())
    } else {
        Err(Error::ObjectUnreadable)
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut out = String::new();
    for byte in bytes {
        out.push(hex_digit(u32::from(byte.wrapping_shr(4))));
        out.push(hex_digit(u32::from(byte & 0xF)));
    }
    out
}

fn hex_digit(value: u32) -> char {
    char::from_digit(value, 16).unwrap_or('0')
}

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
    let mut previous_key: Option<Vec<u8>> = None;
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

        let mut key = name.to_vec();
        if mode == GitMode::Tree {
            key.push(b'/');
        }
        if let Some(previous) = &previous_key
            && *previous >= key
        {
            return Err(Error::ObjectUnreadable);
        }
        previous_key = Some(key);

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

/// Parses a commit body's headers under `git-object-grammar`; the message
/// stays opaque.
///
/// # Errors
///
/// `ObjectUnreadable` for any header order, multiplicity, byte, or
/// continuation defect.
pub fn parse_commit(object_format: ObjectFormat, body: &[u8]) -> Result<Commit, Error> {
    let mut lines = HeaderLines { body, pos: 0 };

    let first = lines.next_line()?;
    let tree_hex = first
        .strip_prefix(b"tree ")
        .ok_or(Error::ObjectUnreadable)?;
    let tree = header_oid(object_format, tree_hex)?;

    let mut parents: Vec<Oid> = Vec::new();
    let mut line = lines.next_line()?;
    while let Some(parent_hex) = line.strip_prefix(b"parent ") {
        parents.push(header_oid(object_format, parent_hex)?);
        line = lines.next_line()?;
    }

    let author = line
        .strip_prefix(b"author ")
        .ok_or(Error::ObjectUnreadable)?;
    if author.is_empty() {
        return Err(Error::ObjectUnreadable);
    }
    let committer_line = lines.next_line()?;
    let committer = committer_line
        .strip_prefix(b"committer ")
        .ok_or(Error::ObjectUnreadable)?;
    if committer.is_empty() {
        return Err(Error::ObjectUnreadable);
    }

    let mut seen_extension = false;
    loop {
        let line = lines.next_line()?;
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

struct HeaderLines<'a> {
    body: &'a [u8],
    pos: usize,
}

impl HeaderLines<'_> {
    fn next_line(&mut self) -> Result<&[u8], Error> {
        let rest = self.body.get(self.pos..).ok_or(Error::ObjectUnreadable)?;
        let lf = rest
            .iter()
            .position(|&b| b == b'\n')
            .ok_or(Error::ObjectUnreadable)?;
        let line = rest.get(..lf).ok_or(Error::ObjectUnreadable)?;
        if line.iter().any(|&b| b == 0 || b == b'\r') {
            return Err(Error::ObjectUnreadable);
        }
        self.pos = self.pos.saturating_add(lf).saturating_add(1);
        Ok(line)
    }
}

fn header_oid(object_format: ObjectFormat, hex_bytes: &[u8]) -> Result<Oid, Error> {
    let text = std::str::from_utf8(hex_bytes).map_err(discard_to_unreadable)?;
    Oid::new(object_format, text.to_owned()).ok_or(Error::ObjectUnreadable)
}
