#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    reason = "fixed pagination boundaries must fail loudly"
)]

use amiss_controller::ProviderError;

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
