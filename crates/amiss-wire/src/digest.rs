use core::{fmt, str::FromStr};

use serde_with::{DeserializeFromStr, SerializeDisplay};
use sha2::{Digest as _, Sha256};

use crate::json::{Callback, Value, stream};

/// The domain for a digest over exact raw bytes taken as evidence: a resolved
/// target's blob, or one build lockfile as the release manifest records it.
pub const RAW_EVIDENCE_DOMAIN: &str = "amiss/raw-evidence";

#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SerializeDisplay, DeserializeFromStr,
)]
pub struct Digest([u8; 32]);

impl Digest {
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Parses the `sha256:` wire form with exactly 64 lowercase hex digits.
    #[must_use]
    pub fn from_wire(raw: &str) -> Option<Self> {
        let hex = raw.strip_prefix("sha256:")?;
        if hex.len() != 64 {
            return None;
        }
        let mut out = [0_u8; 32];
        for (slot, pair) in out.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
            let [high, low] = pair else { return None };
            *slot = hex_value(*high)?.wrapping_shl(4) | hex_value(*low)?;
        }
        Some(Self(out))
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte.wrapping_sub(b'0')),
        b'a'..=b'f' => Some(byte.wrapping_sub(b'a').wrapping_add(10)),
        _ => None,
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const PREFIX: &[u8; 7] = b"sha256:";
        let mut wire = [b'0'; PREFIX.len() + 64];
        for (slot, byte) in wire.iter_mut().zip(PREFIX) {
            *slot = *byte;
        }
        let Some(encoded) = wire.get_mut(PREFIX.len()..) else {
            return Err(fmt::Error);
        };
        for (pair, byte) in encoded.chunks_exact_mut(2).zip(self.0) {
            let [high, low] = pair else {
                return Err(fmt::Error);
            };
            *high = hex_digit(byte.wrapping_shr(4));
            *low = hex_digit(byte & 0x0f);
        }
        let Ok(text) = std::str::from_utf8(&wire) else {
            return Err(fmt::Error);
        };
        f.write_str(text)
    }
}

impl FromStr for Digest {
    type Err = &'static str;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::from_wire(raw).ok_or("invalid SHA-256 digest")
    }
}

const fn hex_digit(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0'.wrapping_add(nibble),
        _ => b'a'.wrapping_add(nibble.wrapping_sub(10)),
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// The plain SHA-256 of exact bytes, with no domain separation: the
/// manifest's file and binary checksums are ordinary content digests, not
/// domain-separated identities.
#[must_use]
pub fn sha256(bytes: &[u8]) -> Digest {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Digest(hasher.finalize().into())
}

/// The plain SHA-256 of byte pieces emitted in order, without joining them in
/// memory first.
#[must_use]
pub fn sha256_stream(emit: impl FnOnce(&mut dyn FnMut(&[u8]))) -> Digest {
    let mut hasher = Sha256::new();
    emit(&mut |piece| hasher.update(piece));
    Digest(hasher.finalize().into())
}

#[must_use]
pub fn hb(domain: &str, bytes: &[u8]) -> Digest {
    hash(domain, |hasher| hasher.update(bytes))
}

#[must_use]
pub fn hb_stream(domain: &str, emit: impl FnOnce(&mut dyn FnMut(&[u8]))) -> Digest {
    hash(domain, |hasher| {
        emit(&mut |piece| hasher.update(piece));
    })
}

#[must_use]
pub fn hj(domain: &str, value: &Value) -> Digest {
    canonical_hash(domain, value, |_| {})
}

/// Hashes serde's compact JSON directly, without buffering the encoded bytes.
/// The input must already serialize in canonical key order with integer numbers.
///
/// # Errors
///
/// Returns the serialization error without producing a partial digest.
pub fn hj_ordered(domain: &str, value: &impl serde::Serialize) -> serde_json::Result<Digest> {
    let mut writer = digest_io::IoWrapper(Sha256::new());
    writer.0.update(domain.as_bytes());
    writer.0.update([0_u8]);
    serde_json::to_writer(&mut writer, value)?;
    Ok(Digest(writer.0.finalize().into()))
}

#[must_use]
pub fn hj_with_length(domain: &str, value: &Value) -> (Digest, u64) {
    let mut length = 0_u64;
    let digest = canonical_hash(domain, value, |piece| {
        length = length.saturating_add(u64::try_from(piece.len()).unwrap_or(u64::MAX));
    });
    (digest, length)
}

fn canonical_hash(domain: &str, value: &Value, mut observe: impl FnMut(&str)) -> Digest {
    hash(domain, |hasher| {
        stream(
            value,
            &mut Callback(|piece: &str| {
                hasher.update(piece.as_bytes());
                observe(piece);
            }),
        );
    })
}

fn hash(domain: &str, update: impl FnOnce(&mut Sha256)) -> Digest {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0_u8]);
    update(&mut hasher);
    Digest(hasher.finalize().into())
}
