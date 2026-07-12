use core::fmt;

use sha2::{Digest as _, Sha256};

use crate::json::{Value, canonical};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Digest([u8; 32]);

impl Digest {
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
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
    let mut hasher = with_domain(domain);
    hasher.update(canonical(value));
    Digest(hasher.finalize().into())
}

fn with_domain(domain: &str) -> Sha256 {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0_u8]);
    hasher
}
