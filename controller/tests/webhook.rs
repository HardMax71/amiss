#![expect(
    clippy::unwrap_used,
    reason = "fixed protocol vectors and valid test fixtures must fail loudly"
)]

#[path = "webhook/gitea.rs"]
mod gitea;
#[path = "webhook/github.rs"]
mod github;
#[path = "webhook/gitlab/mod.rs"]
mod gitlab;
#[path = "webhook/keyring.rs"]
mod keyring;
#[path = "webhook/support.rs"]
mod support;
