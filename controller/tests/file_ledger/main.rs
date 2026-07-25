#![expect(
    clippy::unwrap_used,
    reason = "fixed test fixtures and joined test threads must fail loudly"
)]
mod capacity;
mod claims;
mod cleanup;
mod locking;
mod measure;
mod persistence;
mod process_locking;
mod support;
