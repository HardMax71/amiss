#![cfg(test)]

use super::PullRequestChange;

#[test]
fn a_pull_request_change_spells_its_ids_and_reads_them_back() {
    let change = PullRequestChange::new(101, 4201, 42).unwrap();
    let spelled = change.to_string();
    assert_eq!(spelled, "repository/101/pull/4201/number/42");
    assert_eq!(spelled.parse::<PullRequestChange>().ok(), Some(change));
}

#[test]
fn a_pull_request_change_refuses_zero_ids_and_other_spellings() {
    assert_eq!(PullRequestChange::new(0, 4201, 42), None);
    assert_eq!(PullRequestChange::new(101, 0, 42), None);
    assert_eq!(PullRequestChange::new(101, 4201, 0), None);
    for raw in [
        "",
        "repository/101/pull/4201",
        "repository/101/pull/4201/number/42/extra",
        "repository/0/pull/4201/number/42",
        "repo/101/pull/4201/number/42",
        "repository/101/pull/4201/number/-42",
        "repository/101/pull/4201/number/forty-two",
        "repository/101/pull/4201/number/42/",
    ] {
        assert!(raw.parse::<PullRequestChange>().is_err(), "{raw}");
    }
}
