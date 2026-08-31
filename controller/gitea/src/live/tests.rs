#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    reason = "fixed provider records and protocol identities must fail loudly"
)]

mod publication;
mod refresh;
mod relation;
mod support;
mod transport;
