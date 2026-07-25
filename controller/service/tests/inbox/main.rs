#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "fixed inbox fixtures and filesystem setup must fail loudly"
)]
mod admission;
mod claims;
mod corruption;
mod support;
