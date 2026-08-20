#![cfg(test)]

use serde::de::IgnoredAny;

use amiss_controller::ForgeNegative;

use super::super::model::RefRecord;
use super::{Presence, REF_CEILING, RefFamily, listed_commit, ref_listing};

fn named(reference: &str) -> RefRecord {
    RefRecord {
        reference: reference.to_owned(),
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
            Ok(vec![named("refs/heads/main"), named("refs/tags/v1")]),
            RefFamily::Heads
        ),
        Some(vec!["main".to_owned()]),
        "only the named family's qualifier strips into a candidate"
    );
    let overfull: Vec<RefRecord> = (0..=REF_CEILING)
        .map(|index| named(&format!("refs/heads/b{index}")))
        .collect();
    assert_eq!(
        ref_listing(Ok(overfull), RefFamily::Heads),
        None,
        "past the ceiling nothing proves the set complete"
    );
    let bounded: Vec<RefRecord> = (0..REF_CEILING)
        .map(|index| named(&format!("refs/heads/b{index}")))
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
    assert_eq!(listed_commit(Ok(vec![IgnoredAny])), Presence::Present);
    assert_eq!(listed_commit(Err(ForgeNegative::Missing)), Presence::Absent);
    assert_eq!(listed_commit(Err(ForgeNegative::Denied)), Presence::Unknown);
}
