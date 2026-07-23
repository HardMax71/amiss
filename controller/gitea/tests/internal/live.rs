#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    reason = "fixed provider records and protocol identities must fail loudly"
)]

#[path = "live/publication.rs"]
mod publication;
#[path = "live/refresh.rs"]
mod refresh;
#[path = "live/support.rs"]
mod support;
#[path = "live/transport.rs"]
mod transport;
