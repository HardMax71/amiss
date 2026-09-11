#![cfg(test)]

use amiss_controller::{ForgeNegative, ProviderError};
use http::Response;

use super::super::model::RefRecord;
use super::transport::MAX_RESPONSE_BYTES;
use super::{PAGE_SIZE, PageQuery, Presence, REF_CEILING, RefFamily, listed_commit, ref_listing};

#[test]
fn list_queries_keep_pagination_and_optional_status_ordering() {
    let client = reqwest::blocking::Client::new();
    for sort in [None, Some("highestindex")] {
        let request = client
            .get("https://forge.example/api/v1/repos/acme/widget/statuses/commit")
            .query(&PageQuery {
                page: 2,
                limit: PAGE_SIZE,
                sort,
            })
            .build()
            .unwrap();
        let pairs: Vec<_> = request.url().query_pairs().collect();
        let mut expected = vec![("page".into(), "2".into()), ("limit".into(), "50".into())];
        if let Some(sort) = sort {
            expected.push(("sort".into(), sort.into()));
        }
        assert_eq!(pairs, expected);
    }
}

#[test]
fn ref_responses_ignore_unconsumed_provider_metadata() {
    for input in [
        include_str!("../../../tests/fixtures/gitea-refs.json"),
        include_str!("../../../tests/fixtures/forgejo-refs.json"),
    ] {
        let changed = input.replace(r#""object":{"#, r#""extra":true,"object":{"#);
        assert_ne!(changed, input);
        assert_eq!(
            ref_listing(Ok(Response::new(changed).into()), RefFamily::Heads).unwrap(),
            ref_listing(Ok(Response::new(input).into()), RefFamily::Heads).unwrap()
        );
    }
}

#[test]
fn a_ref_listing_is_a_fact_only_when_positively_complete() {
    let records: Vec<RefRecord> =
        serde_json::from_slice(include_bytes!("../../../tests/fixtures/gitea-refs.json")).unwrap();
    let reference = &records[0];
    for negative in [ForgeNegative::Missing, ForgeNegative::Denied] {
        assert_eq!(ref_listing(Err(negative), RefFamily::Heads), Ok(None));
    }
    assert_eq!(
        ref_listing(Ok(Response::new("[]").into()), RefFamily::Heads),
        Ok(Some(Vec::new())),
        "an empty 2xx array positively means no refs under the prefix"
    );
    let mixed = [
        reference.clone(),
        RefRecord {
            reference: "refs/tags/v1".to_owned(),
            ..reference.clone()
        },
    ];
    assert_eq!(
        ref_listing(
            Ok(Response::new(serde_json::to_vec(&mixed).unwrap()).into()),
            RefFamily::Heads
        ),
        Ok(Some(vec!["main".to_owned()])),
        "only the named family's qualifier strips into a candidate"
    );
    let records: Vec<RefRecord> = (0..=REF_CEILING)
        .map(|index| RefRecord {
            reference: format!("refs/heads/b{index}"),
            ..reference.clone()
        })
        .collect();
    for (length, complete) in [(REF_CEILING, true), (REF_CEILING + 1, false)] {
        let response = Response::new(serde_json::to_vec(&records[..length]).unwrap());
        let listing = ref_listing(Ok(response.into()), RefFamily::Heads).unwrap();
        assert_eq!(listing.is_some(), complete);
    }
}

#[test]
fn an_empty_commit_page_is_no_fact() {
    assert_eq!(
        listed_commit(Ok(Response::new("[]").into())),
        Ok(Presence::Unknown)
    );
    let commit = include_str!("../../../tests/fixtures/gitea-commit-full.json");
    assert_eq!(
        listed_commit(Ok(Response::new(format!("[{commit}]")).into())),
        Ok(Presence::Present)
    );
    assert_eq!(
        listed_commit(Err(ForgeNegative::Missing)),
        Ok(Presence::Absent)
    );
    assert_eq!(
        listed_commit(Err(ForgeNegative::Denied)),
        Ok(Presence::Unknown)
    );
}

#[test]
fn verification_listings_still_require_bounded_valid_bodies() {
    for body in [
        "not JSON".to_owned(),
        "[{}]".to_owned(),
        "[] trailing".to_owned(),
        " ".repeat(MAX_RESPONSE_BYTES + 1),
    ] {
        assert_eq!(
            ref_listing(Ok(Response::new(body.clone()).into()), RefFamily::Heads),
            Err(ProviderError::InvalidResponse)
        );
        assert_eq!(
            listed_commit(Ok(Response::new(body).into())),
            Err(ProviderError::InvalidResponse)
        );
    }
}
