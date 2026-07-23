#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "fixed inbox fixtures and filesystem setup must fail loudly"
)]

#[path = "inbox/admission.rs"]
mod admission;
#[path = "inbox/claims.rs"]
mod claims;
#[path = "inbox/corruption.rs"]
mod corruption;
#[path = "inbox/support.rs"]
mod support;
