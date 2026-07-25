#![expect(
    clippy::unwrap_used,
    reason = "fixed HTTP fixtures and request construction must fail loudly"
)]
mod limits;
mod routes;
mod safety;
mod support;
