use core::{fmt, str::FromStr};

use serde_with::{DeserializeFromStr, SerializeDisplay};

/// The domain for exact raw evidence bytes, including resolved blobs and build lockfiles.
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
        let encoded = raw.strip_prefix("sha256:")?;
        if encoded.len() != 64 || encoded.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return None;
        }
        let mut bytes = [0_u8; 32];
        hex::decode_to_slice(encoded, &mut bytes).ok()?;
        Some(Self(bytes))
    }
}

impl From<[u8; 32]> for Digest {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut spelled = [0_u8; 64];
        hex::encode_to_slice(self.0, &mut spelled).map_err(|_defect| fmt::Error)?;
        formatter.write_str("sha256:")?;
        formatter.write_str(core::str::from_utf8(&spelled).map_err(|_defect| fmt::Error)?)
    }
}

impl FromStr for Digest {
    type Err = &'static str;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        Self::from_wire(raw).ok_or("invalid SHA-256 digest")
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}
