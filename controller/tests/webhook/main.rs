#![expect(
    clippy::unwrap_used,
    reason = "fixed protocol vectors and valid test fixtures must fail loudly"
)]
mod gitea;
mod github;
mod gitlab;
mod keyring;
mod support;
