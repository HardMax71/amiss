#![cfg(test)]

use std::time::Duration;

use amiss_controller::ProviderError;

use super::{
    MAX_PAGES, OperationDeadline, PAGE_SIZE, PAGE_SIZE_U8, PageQuery, check_page, page_complete,
    path_segment, query_route, runs_settled,
};

#[test]
fn a_deadline_keeps_a_positive_remainder_or_refuses() {
    let open = OperationDeadline::after(Duration::from_mins(1)).unwrap();
    assert!(
        open.remaining().unwrap() > Duration::ZERO,
        "a fresh deadline has time left"
    );
    let spent = OperationDeadline::after(Duration::ZERO).unwrap();
    assert_eq!(spent.remaining(), Err(ProviderError::Unavailable));
}

#[test]
fn a_rules_page_is_complete_exactly_under_its_size() {
    assert_eq!(page_complete(0), Ok(true));
    assert_eq!(page_complete(PAGE_SIZE - 1), Ok(true));
    assert_eq!(page_complete(PAGE_SIZE), Ok(false));
    assert_eq!(
        page_complete(PAGE_SIZE + 1),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn a_check_run_page_binds_its_count_to_the_total() {
    let maximum = u64::from(PAGE_SIZE_U8) * u64::from(MAX_PAGES);
    assert_eq!(check_page(0, PAGE_SIZE, maximum), Ok(()));
    assert_eq!(
        check_page(0, PAGE_SIZE + 1, 1),
        Err(ProviderError::InvalidResponse),
        "a page larger than the page size"
    );
    assert_eq!(
        check_page(3, 0, 2),
        Err(ProviderError::InvalidResponse),
        "a total below what is already collected"
    );
    assert_eq!(
        check_page(3, 0, 3),
        Ok(()),
        "a total exactly at the collected count"
    );
    assert_eq!(
        check_page(0, 0, maximum + 1),
        Err(ProviderError::InvalidResponse),
        "a total past the page budget"
    );
    assert_eq!(
        check_page(0, 0, maximum),
        Ok(()),
        "a total exactly at the budget"
    );

    assert_eq!(runs_settled(2, 2), Ok(true));
    assert_eq!(runs_settled(1, 2), Ok(false));
    assert_eq!(runs_settled(3, 2), Err(ProviderError::InvalidResponse));
}

#[test]
fn routes_encode_their_segments_and_queries() {
    assert_eq!(path_segment("release/x"), "release%2Fx");
    assert_eq!(path_segment("main"), "main");
    let query = PageQuery {
        per_page: PAGE_SIZE_U8,
        page: 2,
    };
    assert_eq!(
        query_route("/repos/a/b/rules/branches/main", &query).unwrap(),
        format!("/repos/a/b/rules/branches/main?per_page={PAGE_SIZE_U8}&page=2")
    );
}
