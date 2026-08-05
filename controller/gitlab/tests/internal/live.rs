#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    reason = "fixed pagination boundaries must fail loudly"
)]

use amiss_controller::ProviderError;

use super::model::{CommitResponse, ProjectResponse};
use super::refresh::validated_repository_url;
use super::{MAX_PAGES, PAGE_SIZE, page_complete};

const BELOW_THE_FLOOR: &str = r#"{
  "id": 1,
  "path_with_namespace": "root/lane",
  "default_branch": "main",
  "http_url_to_repo": "https://gitlab.example/root/lane.git",
  "repository_object_format": "sha1",
  "only_allow_merge_if_pipeline_succeeds": false,
  "allow_merge_on_skipped_pipeline": null,
  "merge_method": "merge",
  "squash_option": "default_off"
}"#;

const AT_THE_FLOOR: &str = r#"{
  "id": 1,
  "path_with_namespace": "root/lane",
  "default_branch": "main",
  "http_url_to_repo": "https://gitlab.example/root/lane.git",
  "repository_object_format": "sha1",
  "only_allow_merge_if_pipeline_succeeds": true,
  "allow_merge_on_skipped_pipeline": false,
  "merge_pipelines_enabled": true,
  "merge_trains_enabled": true,
  "merge_trains_skip_train_allowed": false,
  "merge_train_enforcement": "enforced",
  "merge_method": "merge",
  "squash_option": "default_off"
}"#;

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

#[test]
fn a_project_without_the_merge_train_settings_is_below_the_supported_floor() {
    assert!(serde_json::from_str::<ProjectResponse>(BELOW_THE_FLOOR).is_err());
    assert!(serde_json::from_str::<ProjectResponse>(AT_THE_FLOOR).is_ok());
}

type QueryDeviation = fn(&mut crate::GitLabRefreshQuery);

/// Every falsifiable field of the refresh query is load-bearing on its own.
/// The gate commit is an `Oid`, and `exact_sha1` is that same grammar, so no
/// constructible value can deviate; the valid case pins that clause's
/// polarity instead.
#[test]
fn the_refresh_query_is_exact_in_every_field() {
    use amiss_wire::model::{ObjectFormat, Oid};

    use super::refresh::validate_query;
    use crate::GitLabRefreshQuery;

    let valid = || GitLabRefreshQuery {
        project_id: 1,
        merge_request_iid: 42,
        pipeline_id: 202,
        job_id: 303,
        runner_id: 77,
        gate_commit: Oid::new(ObjectFormat::Sha1, "b".repeat(40)).unwrap(),
    };
    assert!(validate_query(&valid()).is_ok());

    let deviations: [(&str, QueryDeviation); 5] = [
        ("project", |query| query.project_id = 0),
        ("merge request", |query| query.merge_request_iid = 0),
        ("pipeline", |query| query.pipeline_id = 0),
        ("job", |query| query.job_id = 0),
        ("runner", |query| query.runner_id = 0),
    ];
    for (reason, deviate) in deviations {
        let mut query = valid();
        deviate(&mut query);
        assert_eq!(
            validate_query(&query),
            Err(ProviderError::InvalidResponse),
            "{reason}"
        );
    }
}

fn rest_commit(id: &str, parents: &[&str]) -> CommitResponse {
    CommitResponse {
        id: id.to_owned(),
        parent_ids: parents.iter().map(|parent| (*parent).to_owned()).collect(),
    }
}

fn resolved(id: &str, parents: &[&str], tree: char) -> crate::GitLabCommit {
    crate::GitLabCommit {
        id: id.to_owned(),
        tree: tree.to_string().repeat(40),
        parents: parents.iter().map(|parent| (*parent).to_owned()).collect(),
    }
}

/// The REST answer must be the commit it was asked about, with an exact
/// first parent to name the base by.
#[test]
fn the_rest_answer_must_be_the_gate_it_was_asked_for() {
    use amiss_wire::model::{ObjectFormat, Oid};

    use super::refresh::claimed_base;

    let gate_hex = "b".repeat(40);
    let base_hex = "a".repeat(40);
    let gate = Oid::new(ObjectFormat::Sha1, gate_hex.clone()).unwrap();

    assert_eq!(
        claimed_base(&rest_commit(&gate_hex, &[&base_hex]), &gate),
        Oid::new(ObjectFormat::Sha1, base_hex.clone()).ok_or(ProviderError::InvalidResponse),
        "the sound answer names its base"
    );
    for (reason, commit) in [
        (
            "another commit entirely",
            rest_commit(&"c".repeat(40), &[&base_hex]),
        ),
        ("no parents to base on", rest_commit(&gate_hex, &[])),
        (
            "a parent outside the grammar",
            rest_commit(&gate_hex, &["not-a-sha"]),
        ),
    ] {
        assert_eq!(
            claimed_base(&commit, &gate),
            Err(ProviderError::InvalidResponse),
            "{reason}"
        );
    }
}

/// What Git resolved must repeat the REST claim exactly, clause by clause.
#[test]
fn resolved_objects_must_repeat_the_claim_exactly() {
    use super::refresh::resolved_matches_claim;

    let gate_hex = "b".repeat(40);
    let base_hex = "a".repeat(40);
    let other_hex = "c".repeat(40);
    let claim = rest_commit(&gate_hex, &[&base_hex]);
    let objects =
        |gate: crate::GitLabCommit, base: crate::GitLabCommit| crate::GitLabObjects { gate, base };

    assert_eq!(
        resolved_matches_claim(
            &objects(
                resolved(&gate_hex, &[&base_hex], 'd'),
                resolved(&base_hex, &[], 'e'),
            ),
            &claim,
        ),
        Ok(()),
        "the resolution repeats the claim"
    );

    let cases = [
        (
            "the gate resolved to another commit",
            objects(
                resolved(&other_hex, &[&base_hex], 'd'),
                resolved(&base_hex, &[], 'e'),
            ),
        ),
        (
            "the gate's parents changed underneath the claim",
            objects(
                resolved(&gate_hex, &[&other_hex], 'd'),
                resolved(&other_hex, &[], 'e'),
            ),
        ),
        (
            "the base is not the parent the gate resolved",
            objects(
                resolved(&gate_hex, &[&base_hex], 'd'),
                resolved(&other_hex, &[], 'e'),
            ),
        ),
        (
            "a gate with no parents has no base to stand on",
            objects(resolved(&gate_hex, &[], 'd'), resolved(&base_hex, &[], 'e')),
        ),
    ];
    for (reason, resolved_pair) in cases {
        assert_eq!(
            resolved_matches_claim(&resolved_pair, &claim),
            Err(ProviderError::InvalidResponse),
            "{reason}"
        );
    }
}
