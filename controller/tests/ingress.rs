#![expect(clippy::unwrap_used, reason = "fixed test fixtures must fail loudly")]

#[path = "ingress/freshness.rs"]
mod freshness;
#[path = "ingress/limits.rs"]
mod limits;
#[path = "ingress/proof.rs"]
mod proof;
#[path = "ingress/support.rs"]
mod support;
