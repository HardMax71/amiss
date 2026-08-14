#![expect(
    clippy::unwrap_used,
    reason = "fixed test fixtures and poison-free test mutexes must fail loudly"
)]
mod claims;
mod delivery;
mod external;
mod identity;
mod leases;
mod results;
mod support;
