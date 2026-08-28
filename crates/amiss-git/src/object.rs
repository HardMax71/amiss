mod commit;
mod loose;
mod tree;

use amiss_wire::controls::GitMode;
use amiss_wire::model::{ObjectFormat, Oid};
use sha1_checked::Digest as _;
use sha2::Digest as _;

use crate::Error;

pub use commit::parse_commit;
pub use loose::decode_loose;
pub(crate) use loose::decode_loose_reusing;
pub use tree::parse_tree;

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::AsRefStr)]
#[strum(serialize_all = "lowercase")]
pub enum ObjectKind {
    Blob,
    Commit,
    Tag,
    Tree,
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
    let mut out = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        out.push(char::from_digit(u32::from(byte.wrapping_shr(4)), 16).unwrap_or('0'));
        out.push(char::from_digit(u32::from(byte & 0xF), 16).unwrap_or('0'));
    }
    out
}
