#![expect(
    clippy::unwrap_used,
    reason = "fixed HTTP fixtures and request construction must fail loudly"
)]

#[path = "receiver/limits.rs"]
mod limits;
#[path = "receiver/routes.rs"]
mod routes;
#[path = "receiver/safety.rs"]
mod safety;
#[path = "receiver/support.rs"]
mod support;
