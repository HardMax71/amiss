#![expect(
    clippy::unwrap_used,
    reason = "fixed test fixtures and poison-free test mutexes must fail loudly"
)]

#[path = "flow/claims.rs"]
mod claims;
#[path = "flow/delivery.rs"]
mod delivery;
#[path = "flow/identity.rs"]
mod identity;
#[path = "flow/leases.rs"]
mod leases;
#[path = "flow/results.rs"]
mod results;
#[path = "flow/support.rs"]
mod support;
