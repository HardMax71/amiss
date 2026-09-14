#![cfg(test)]

use std::num::NonZeroU64;

use super::{Change, MergeRequestChange, PullRequestChange};

#[test]
fn changes_refuse_zero_ids() {
    assert_eq!(PullRequestChange::new(0, 4201, 42), None);
    assert_eq!(PullRequestChange::new(101, 0, 42), None);
    assert_eq!(PullRequestChange::new(101, 4201, 0), None);
    assert_eq!(MergeRequestChange::new(0, 3), None);
    assert_eq!(MergeRequestChange::new(7, 0), None);
    assert_eq!(
        PullRequestChange::new(101, 4201, 42).map(|change| change.number),
        NonZeroU64::new(42)
    );
}

#[test]
fn a_stored_change_reads_back_as_written_and_refuses_zeros() {
    let change = Change::PullRequest(PullRequestChange::new(101, 4201, 42).unwrap());
    let bytes = serde_json::to_vec(&change).unwrap();
    assert_eq!(serde_json::from_slice::<Change>(&bytes).unwrap(), change);
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        r#"{"pull-request":{"repository_id":101,"pull_request_id":4201,"number":42}}"#
    );
    for invalid in [
        r#"{"pull-request":{"repository_id":0,"pull_request_id":4201,"number":42}}"#,
        r#"{"merge-request":{"project_id":7,"iid":3,"extra":1}}"#,
        r#"{"merge-request":{"project_id":7}}"#,
        r#"{"issue":{"number":1}}"#,
        r#""repository/101/pull/4201/number/42""#,
    ] {
        assert!(
            serde_json::from_str::<Change>(invalid).is_err(),
            "{invalid}"
        );
    }
}
