#![expect(
    clippy::unwrap_used,
    reason = "fixed test fixtures and joined test threads must fail loudly"
)]

#[path = "file_ledger/claims.rs"]
mod claims;
#[path = "file_ledger/locking.rs"]
mod locking;
#[path = "file_ledger/persistence.rs"]
mod persistence;
#[path = "file_ledger/process_locking.rs"]
mod process_locking;
#[path = "file_ledger/support.rs"]
mod support;
