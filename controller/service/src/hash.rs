use sha2::{Digest as _, Sha256};

pub(crate) fn digest(domain: &str, bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    hasher.finalize().into()
}

pub(crate) fn digest_hex(domain: &str, bytes: &[u8]) -> String {
    hex::encode(digest(domain, bytes))
}

pub(crate) fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
