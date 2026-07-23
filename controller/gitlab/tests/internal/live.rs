#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    reason = "fixed pagination boundaries must fail loudly"
)]

use amiss_controller::ProviderError;

use super::refresh::validated_repository_url;
use super::{MAX_PAGES, PAGE_SIZE, page_complete};

#[test]
fn pagination_must_prove_the_complete_protection_set() {
    assert_eq!(page_complete(1, 0), Ok(true));
    assert_eq!(page_complete(1, PAGE_SIZE - 1), Ok(true));
    assert_eq!(page_complete(1, PAGE_SIZE), Ok(false));
    assert_eq!(
        page_complete(1, PAGE_SIZE + 1),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(
        page_complete(MAX_PAGES, PAGE_SIZE),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn object_fetch_uses_only_the_canonical_provider_repository_url() {
    let canonical = "https://gitlab.example/acme/widget.git";
    assert_eq!(
        validated_repository_url("gitlab.example", 101, 101, "acme/widget", canonical),
        Ok(canonical.to_owned())
    );

    for (project_id, path, reported) in [
        (202, "acme/widget", canonical),
        (
            101,
            "acme/widget",
            "https://attacker.invalid/acme/widget.git",
        ),
        (101, "acme/other", canonical),
    ] {
        assert_eq!(
            validated_repository_url("gitlab.example", 101, project_id, path, reported),
            Err(ProviderError::InvalidResponse)
        );
    }
}
