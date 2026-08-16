#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "black-box harness over asserted fixture shapes"
)]

use std::io::Write as _;
use std::process::{Command, Stdio};

use amiss_wire::controls::{OrganizationFloor, Profile};
use amiss_wire::json::parse;
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::requests::{
    ControlsRequest, EvaluationRequest, RequestStreams, RequestTrust, SEALED_ENGINE_ARGUMENT,
    SnapshotRequest, SuppliedControl,
};

fn run(repo: Option<&str>, input: &[u8]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_amiss"));
    command
        .arg(SEALED_ENGINE_ARGUMENT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = repo {
        command.current_dir(path);
    }
    let mut child = command.spawn().expect("spawn sealed engine");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(input)
        .expect("write request frame");
    child.wait_with_output().expect("collect sealed engine")
}

fn example_streams() -> RequestStreams {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples");
    let evaluation = EvaluationRequest::parse(
        &std::fs::read(root.join("scanner-evaluation-request.json")).unwrap(),
    )
    .unwrap();
    let snapshot =
        SnapshotRequest::parse(&std::fs::read(root.join("scanner-snapshot-request.json")).unwrap())
            .unwrap();
    let controls =
        ControlsRequest::parse(&std::fs::read(root.join("scanner-controls-request.json")).unwrap())
            .unwrap();
    RequestStreams {
        evaluation: evaluation.canonical_bytes().unwrap(),
        snapshot: snapshot.canonical_bytes().unwrap(),
        controls: controls.canonical_bytes().unwrap(),
    }
}

#[test]
fn malformed_and_trailing_frames_never_reach_the_command_grammar() {
    let mut trailing = Vec::new();
    example_streams().write_to(&mut trailing).unwrap();
    trailing.push(0);
    for input in [b"not-a-frame".to_vec(), trailing] {
        let output = run(None, &input);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("REQUEST_UNREADABLE"));
    }
}

fn framed(streams: &RequestStreams) -> Vec<u8> {
    let mut frame = Vec::new();
    streams.write_to(&mut frame).unwrap();
    frame
}

/// Byte-different, same value: the gate reads bytes, not meaning.
fn spaced(canonical: &[u8]) -> Vec<u8> {
    let mut loosened = canonical.to_vec();
    loosened.insert(1, b' ');
    loosened
}

/// Each stream arrives in canonical form and the two modes agree, checked
/// stream by stream so no clause rides on another.
#[test]
fn a_sealed_frame_is_canonical_in_every_stream_and_agrees_on_the_mode() {
    let elsewhere = tempfile::tempdir().expect("a directory that is no repository");
    let root = amiss_fixtures::path_arg(elsewhere.path());
    let mut cases: Vec<(&str, RequestStreams)> = Vec::new();
    for name in ["evaluation", "snapshot", "controls"] {
        let mut streams = example_streams();
        let stream = match name {
            "evaluation" => &mut streams.evaluation,
            "snapshot" => &mut streams.snapshot,
            _ => &mut streams.controls,
        };
        *stream = spaced(stream);
        cases.push((name, streams));
    }
    let mut mismatched = example_streams();
    mismatched.snapshot = SnapshotRequest::index().canonical_bytes().unwrap();
    cases.push(("the snapshot mode", mismatched));

    for (name, streams) in cases {
        let output = run(Some(&root), &framed(&streams));
        assert_eq!(output.status.code(), Some(2), "{name}");
        assert!(output.stdout.is_empty(), "{name}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("INVALID_INVOCATION"),
            "{name} is refused before the run reaches a repository: {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// The identity in the frame is the one resolution works with, so a URL on
/// the declared host is this repository and not a foreign site.
#[test]
fn a_sealed_run_resolves_against_the_identity_it_was_given() {
    let fixture = amiss_fixtures::commit_pair(
        &[("docs/guide.md", "# Guide\n")],
        &[(
            "docs/guide.md",
            "# Guide\n\n[self](https://ghes.example/acme/widget/blob/main/docs/guide.md)\n",
        )],
    )
    .unwrap();
    let format = ObjectFormat::Sha1;
    let mut evaluation = EvaluationRequest::commit_pair(
        Profile::Observe,
        format,
        Oid::new(format, fixture.base.clone()).unwrap(),
        Oid::new(format, fixture.candidate.clone()).unwrap(),
    );
    evaluation.repository = RepositoryIdentity::new(
        "ghes.example".to_owned(),
        "acme".to_owned(),
        "widget".to_owned(),
    );
    evaluation.forge = Some(ForgeDialect::Github);
    evaluation.candidate_ref = BranchRef::new("refs/heads/main".to_owned());
    evaluation.target_ref = BranchRef::new("refs/heads/main".to_owned());
    evaluation.default_branch_ref = BranchRef::new("refs/heads/main".to_owned());

    let streams = RequestStreams {
        evaluation: evaluation.canonical_bytes().unwrap(),
        snapshot: SnapshotRequest::git_objects().canonical_bytes().unwrap(),
        controls: ControlsRequest::default().canonical_bytes().unwrap(),
    };
    let output = run(Some(&fixture.repo), &framed(&streams));
    assert_eq!(output.status.code(), Some(0), "{:?}", output.stderr);
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let references = &envelope["payload"]["summary"]["references"];
    assert_eq!(
        references["same_repository"], 1,
        "the declared host's own URL: {references}"
    );
    assert_eq!(references["external_out_of_scope"], 0);
}

#[test]
fn sealed_requests_keep_candidate_identity_separate_from_the_control_target() {
    let fixture =
        amiss_fixtures::commit_pair(&[("README.md", "base\n")], &[("README.md", "candidate\n")])
            .unwrap();
    let format = ObjectFormat::Sha1;
    let mut evaluation = EvaluationRequest::commit_pair(
        Profile::Observe,
        format,
        Oid::new(format, fixture.base.clone()).unwrap(),
        Oid::new(format, fixture.candidate.clone()).unwrap(),
    );
    evaluation.repository = RepositoryIdentity::new(
        "github.com".to_owned(),
        "acme".to_owned(),
        "docs".to_owned(),
    );
    evaluation.forge = Some(ForgeDialect::Github);
    evaluation.candidate_ref = BranchRef::new("refs/heads/feature/docs".to_owned());
    evaluation.target_ref = BranchRef::new("refs/heads/main".to_owned());
    evaluation.default_branch_ref = BranchRef::new("refs/heads/main".to_owned());

    let floor_bytes = br#"{
      "schema":"amiss/organization-floor",
      "floor_id":"acme/floor",
      "repository":{"host":"github.com","owner":"acme","name":"docs"},
      "ref":"refs/heads/main",
      "minimum_profile":"observe",
      "minimum_dispositions":[],
      "protected_inventory":[],
      "protected_control_paths":[],
      "waivable_finding_kinds":[],
      "authorized_debt_owners":[],
      "authorized_waiver_issuers":[],
      "resource_limits":[]
    }"#;
    let floor = OrganizationFloor::parse(floor_bytes).unwrap();
    let controls = ControlsRequest {
        organization_floor: Some(SuppliedControl {
            value: parse(floor_bytes).unwrap(),
            expected_digest: floor.digest(),
            trust_source: RequestTrust::OrganizationPolicy,
        }),
        ..ControlsRequest::default()
    };
    let streams = RequestStreams {
        evaluation: evaluation.canonical_bytes().unwrap(),
        snapshot: SnapshotRequest::git_objects().canonical_bytes().unwrap(),
        controls: controls.canonical_bytes().unwrap(),
    };
    let mut frame = Vec::new();
    streams.write_to(&mut frame).unwrap();
    let output = run(Some(&fixture.repo), &frame);
    assert_eq!(output.status.code(), Some(0), "{:?}", output.stderr);
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let payload = &envelope["payload"];
    assert_eq!(
        payload["evaluation"]["candidate_ref"],
        "refs/heads/feature/docs"
    );
    assert_eq!(payload["evaluation"]["target_ref"], "refs/heads/main");
    assert_eq!(
        payload["controls"]["organization_floor"]["status"],
        "verified"
    );
    assert_eq!(payload["controls"]["sandbox"]["assurance"], "self-asserted");
}
