use core::fmt;

use sha2::{Digest as _, Sha256};

use crate::json::{Sink, Value, stream};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
        f.write_str("sha256:")?;
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[must_use]
pub fn hb(domain: &str, bytes: &[u8]) -> Digest {
    let mut hasher = with_domain(domain);
    hasher.update(bytes);
    Digest(hasher.finalize().into())
}

#[must_use]
pub fn hj(domain: &str, value: &Value) -> Digest {
    let mut sink = HashSink(with_domain(domain));
    stream(value, &mut sink);
    Digest(sink.0.finalize().into())
}

struct HashSink(Sha256);

impl Sink for HashSink {
    fn write(&mut self, piece: &str) {
        self.0.update(piece.as_bytes());
    }
}

fn with_domain(domain: &str) -> Sha256 {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0_u8]);
    hasher
}
