#![cfg(test)]

use amiss_controller::ForgeNegative;

use super::super::model::RefRecord;
use super::{Presence, REF_CEILING, RefFamily, listed_commit, ref_listing};

#[test]
fn ref_responses_cannot_discard_unknown_data() {
    for input in [
        include_str!("../../../tests/fixtures/gitea-refs.json"),
        include_str!("../../../tests/fixtures/forgejo-refs.json"),
    ] {
        let changed = input.replace(r#""object":{"#, r#""extra":true,"object":{"#);
        assert_ne!(changed, input);
        assert!(serde_json::from_str::<Vec<RefRecord>>(&changed).is_err());
    }
}

/// The refs route is one unpaginated GET, and its 404 cannot distinguish
/// an empty match set from a repository that turned private or vanished
/// mid-walk. A revision-missing built on that guess would refute a live
/// URL, so only a 2xx body is an answer: its empty array is the empty
/// match set, and a body past the ceiling the paginated siblings imply is
/// not proven whole.
#[test]
fn a_ref_listing_is_a_fact_only_when_positively_complete() {
    let records: Vec<RefRecord> = amiss_wire::read_json(
        include_bytes!("../../../tests/fixtures/gitea-refs.json"),
        u64::MAX,
    )
    .unwrap();
    let reference = &records[0];
    assert_eq!(
        ref_listing(Err(ForgeNegative::Missing), RefFamily::Heads),
        None
    );
    assert_eq!(
        ref_listing(Err(ForgeNegative::Denied), RefFamily::Heads),
        None
    );
    assert_eq!(
        ref_listing(Ok(Vec::new()), RefFamily::Heads),
        Some(Vec::new()),
        "an empty 2xx array positively means no refs under the prefix"
    );
    assert_eq!(
        ref_listing(
            Ok(vec![
                reference.clone(),
                RefRecord {
                    reference: "refs/tags/v1".to_owned(),
                    ..reference.clone()
                }
            ]),
            RefFamily::Heads
        ),
        Some(vec!["main".to_owned()]),
        "only the named family's qualifier strips into a candidate"
    );
    let overfull: Vec<RefRecord> = (0..=REF_CEILING)
        .map(|index| RefRecord {
            reference: format!("refs/heads/b{index}"),
            ..reference.clone()
        })
        .collect();
    assert_eq!(
        ref_listing(Ok(overfull), RefFamily::Heads),
        None,
        "past the ceiling nothing proves the set complete"
    );
    let bounded: Vec<RefRecord> = (0..REF_CEILING)
        .map(|index| RefRecord {
            reference: format!("refs/heads/b{index}"),
            ..reference.clone()
        })
        .collect();
    assert!(ref_listing(Ok(bounded), RefFamily::Heads).is_some());
}

/// An empty repository answers the commit list route 200 with an empty
/// array for any revision at all; reading that as presence cascaded into
/// path-missing refutations of files behind never-pushed repositories.
/// Only a listed commit is presence, a 404 stays the positive absence,
/// and the empty page is no fact.
#[test]
fn an_empty_commit_page_is_no_fact() {
    assert_eq!(listed_commit(Ok(Vec::new())), Presence::Unknown);
    let commit = amiss_wire::read_json(
        include_bytes!("../../../tests/fixtures/gitea-commit-full.json"),
        u64::MAX,
    )
    .unwrap();
    assert_eq!(listed_commit(Ok(vec![commit])), Presence::Present);
    assert_eq!(listed_commit(Err(ForgeNegative::Missing)), Presence::Absent);
    assert_eq!(listed_commit(Err(ForgeNegative::Denied)), Presence::Unknown);
}
