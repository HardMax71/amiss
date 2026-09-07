#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "black-box harness over asserted fixture shapes"
)]

use std::io::Write as _;
use std::process::{Command, Stdio};

use amiss_fixtures::{SiteObservation, site_observation};
use amiss_wire::assessment::Nullable;
use amiss_wire::controls::{Profile, canonical_organization_floor, parse_organization_floor};
use amiss_wire::digest::hb;
use amiss_wire::model::{
    ArtifactId, BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity,
};
use amiss_wire::report::validate_envelope;
use amiss_wire::requests::{
    ControlsRequest, EvaluationRequest, RequestStreams, RequestTrust, SEALED_ENGINE_ARGUMENT,
    SnapshotRequest, SuppliedControl, SuppliedSemanticEvidence, commit_candidate_identity_digest,
};
use amiss_wire::semantic::observation::{
    Observation, SiteBuildObservation, SphinxLabelKind, SphinxLabelObservation,
};
use amiss_wire::semantic::{PayloadSchema, SemanticEvidence, SemanticProducer, SemanticSubject};

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

fn contract_report(bytes: &[u8]) -> serde_json::Value {
    validate_envelope(bytes.strip_suffix(b"\n").unwrap()).unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../../../spec/scanner-report.schema.json")).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let defects: Vec<String> = validator
        .iter_errors(&envelope)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert!(defects.is_empty(), "{defects:?}");
    envelope
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
    let floor = parse_organization_floor(floor_bytes).unwrap();
    let controls = ControlsRequest {
        organization_floor: Some(SuppliedControl {
            expected_digest: canonical_organization_floor(&floor).unwrap().1,
            value: floor,
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

fn id(value: &str) -> ArtifactId {
    ArtifactId::new(value.to_owned()).unwrap()
}

fn sphinx_label(inventory: &str, name: &str, destination: &str) -> Observation {
    Observation::Sphinx(SphinxLabelObservation {
        kind: SphinxLabelKind::Current,
        inventory: id(inventory),
        name: name.to_owned(),
        destination: destination.to_owned(),
    })
}

fn intersphinx_case() -> (
    amiss_fixtures::CommitPair,
    EvaluationRequest,
    SemanticEvidence<'static>,
) {
    let fixture = amiss_fixtures::commit_pair(
        &[("docs/guide.rst", "Guide\n=====\n")],
        &[(
            "docs/guide.rst",
            "Guide\n=====\n\nSee :ref:`except_star`, :ref:`package_env`, :ref:`python:assert`, and :ref:`shared`.\n",
        )],
    )
    .unwrap();
    let format = ObjectFormat::Sha1;
    let evaluation = EvaluationRequest::commit_pair(
        Profile::Observe,
        format,
        Oid::new(format, fixture.base.clone()).unwrap(),
        Oid::new(format, fixture.candidate.clone()).unwrap(),
    );
    let identity = commit_candidate_identity_digest(
        &evaluation,
        &Oid::new(format, fixture.base_tree.clone()).unwrap(),
        &Oid::new(format, fixture.candidate_tree.clone()).unwrap(),
    )
    .unwrap();
    let context_digest = hb("amiss-test/inventory", b"python and another");
    let semantic = SemanticEvidence {
        schema: PayloadSchema::Current,
        subject: SemanticSubject {
            candidate_identity_digest: identity,
            source_report_payload_digest: Nullable::Null,
        },
        producer: SemanticProducer {
            kind: amiss_wire::semantic::SemanticProducerKind::SphinxInventorySet,
            identity: id("amiss-test"),
            version: "1".to_owned(),
            context_digest,
            input_digest: context_digest,
        },
        complete: true,
        observations: vec![
            sphinx_label(
                "python",
                "except_star",
                "https://docs.python.org/3/reference/compound_stmts.html#except-star",
            ),
            sphinx_label("python", "shared", "https://docs.python.org/3/shared"),
            sphinx_label("another", "shared", "https://example.invalid/shared"),
        ]
        .into_iter()
        .map(std::borrow::Cow::Owned)
        .collect(),
    };
    (fixture, evaluation, semantic)
}

#[test]
fn sealed_intersphinx_evidence_resolves_only_unique_labels() {
    use amiss_wire::report::model::{SemanticEvidenceProducer, SemanticEvidenceProvenance};

    let (fixture, evaluation, mut semantic) = intersphinx_case();
    semantic.producer.input_digest = hb("amiss-test/input", b"inventory bytes");
    let producer = semantic.producer.clone();
    let expected_context_digest = semantic.producer.context_digest;
    let evidence = amiss_wire::semantic::envelope(semantic).unwrap();
    let expected_provenance = vec![SemanticEvidenceProvenance {
        payload_digest: evidence.payload_digest,
        producer: SemanticEvidenceProducer {
            kind: producer.kind,
            identity: producer.identity,
            version: producer.version,
            input_digest: producer.input_digest,
        },
    }];
    let controls = ControlsRequest {
        semantic_evidence: vec![SuppliedSemanticEvidence {
            value: evidence,
            expected_context_digest,
        }],
        ..ControlsRequest::default()
    };
    let streams = RequestStreams {
        evaluation: evaluation.canonical_bytes().unwrap(),
        snapshot: SnapshotRequest::git_objects().canonical_bytes().unwrap(),
        controls: controls.canonical_bytes().unwrap(),
    };
    let output = run(Some(&fixture.repo), &framed(&streams));
    assert_eq!(output.status.code(), Some(0), "{:?}", output.stderr);
    let envelope = contract_report(&output.stdout);
    let labels: Vec<&serde_json::Value> = envelope["payload"]["observations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row.get("candidate"))
        .filter(|row| row.pointer("/intent/kind") == Some(&serde_json::json!("label")))
        .collect();
    assert_eq!(labels.len(), 4);
    assert!(labels.iter().any(|row| {
        row.pointer("/resolution/reason") == Some(&serde_json::json!("intersphinx-inventory"))
            && row.get("external_destination")
                == Some(&serde_json::json!(
                    "https://docs.python.org/3/reference/compound_stmts.html#except-star"
                ))
    }));
    assert!(labels.iter().any(|row| {
        row.pointer("/resolution/reason") == Some(&serde_json::json!("label-not-declared"))
    }));
    assert_eq!(
        labels
            .iter()
            .filter(|row| {
                row.pointer("/resolution/reason") == Some(&serde_json::json!("external-inventory"))
            })
            .count(),
        2,
        "a named inventory and ambiguous prefixless evidence remain unsupported"
    );
    assert_eq!(
        envelope["payload"]["controls"]["semantic_evidence"],
        serde_json::to_value(&expected_provenance).unwrap(),
    );
    assert_eq!(
        serde_json::to_vec(&expected_provenance).unwrap(),
        serde_json_canonicalizer::to_vec(&envelope["payload"]["controls"]["semantic_evidence"])
            .unwrap(),
    );
    assert_eq!(envelope["payload"]["summary"]["references"]["resolved"], 1);
}

#[test]
fn sealed_site_build_evidence_resolves_candidate_routes_anchors_and_redirects() {
    let index = "# Index\n\n[route](/guide/) [anchor](/guide/#intr%6F) [raw anchor](/guide/#raw%anchor) [mixed anchor](/guide/#mixed%20anchor%tail) [page top](/guide/#TOP) [absent](/guide/#absent) [unknown](/missing/) [stale](/stale/) [duplicate](/duplicate/) [redirect](/legacy/) [redirect anchor](/legacy/#intro) [changed redirect anchor](/changed/#absent) [cleared redirect anchor](/cleared/#absent) [broken redirect anchor](/broken-fragment/) [broken redirect](/broken/) [collision](/collision/) ![image](/legacy/) [generated](/generated/) [generated anchor](/generated/#api) [generated absent](/generated/#absent) [generated stale](/generated-stale/) [generated redirect](/generated-legacy/) ![generated image](/generated/)\n";
    let fixture = amiss_fixtures::commit_pair(
        &[
            ("README.md", "# Repository\n"),
            ("docs/redirects.toml", "# Redirect rules\n"),
            ("docs/SUMMARY.md", "# Summary\n"),
            ("docs/index.md", index),
            ("docs/guide.md", "# Intro\n"),
            ("site.config.js", "export default {};\n"),
        ],
        &[
            ("README.md", "# Repository\n"),
            ("docs/redirects.toml", "# Redirect rules\n"),
            ("docs/SUMMARY.md", "# Summary\n"),
            ("docs/index.md", index),
            ("docs/guide.md", "# Intro\n"),
            ("site.config.js", "export default {};\n"),
        ],
    )
    .unwrap();
    let format = ObjectFormat::Sha1;
    let evaluation = EvaluationRequest::commit_pair(
        Profile::Observe,
        format,
        Oid::new(format, fixture.base.clone()).unwrap(),
        Oid::new(format, fixture.candidate.clone()).unwrap(),
    );
    let identity = commit_candidate_identity_digest(
        &evaluation,
        &Oid::new(format, fixture.base_tree.clone()).unwrap(),
        &Oid::new(format, fixture.candidate_tree.clone()).unwrap(),
    )
    .unwrap();
    let context_digest = hb("amiss-test/site-context", b"default/current");
    let evidence = amiss_wire::semantic::envelope(SemanticEvidence {
        schema: PayloadSchema::Current,
        subject: SemanticSubject {
            candidate_identity_digest: identity,
            source_report_payload_digest: Nullable::Value(hb(
                "amiss-test/report",
                b"source report",
            )),
        },
        producer: SemanticProducer {
            kind: amiss_wire::semantic::SemanticProducerKind::SiteBuild,
            identity: id("amiss-test"),
            version: "0.5.1".to_owned(),
            context_digest,
            input_digest: hb("amiss-test/site-output", b"site output"),
        },
        complete: true,
        observations: site_build_observations()
            .into_iter()
            .map(std::borrow::Cow::Owned)
            .collect(),
    })
    .unwrap();
    let streams = RequestStreams {
        evaluation: evaluation.canonical_bytes().unwrap(),
        snapshot: SnapshotRequest::git_objects().canonical_bytes().unwrap(),
        controls: ControlsRequest {
            semantic_evidence: vec![SuppliedSemanticEvidence {
                value: evidence,
                expected_context_digest: context_digest,
            }],
            ..ControlsRequest::default()
        }
        .canonical_bytes()
        .unwrap(),
    };
    let output = run(Some(&fixture.repo), &framed(&streams));
    assert_eq!(output.status.code(), Some(0), "{:?}", output.stderr);
    let envelope = contract_report(&output.stdout);
    assert_site_routes(&envelope);
    assert_unlinked(&envelope, &["docs/index.md"]);
    assert_site_defects(&envelope);
}

fn assert_site_routes(envelope: &serde_json::Value) {
    let routes: Vec<&serde_json::Value> = envelope
        .pointer("/payload/observations")
        .and_then(serde_json::Value::as_array)
        .unwrap()
        .iter()
        .filter(|row| {
            row.pointer("/candidate/intent/kind") == Some(&serde_json::json!("site-route"))
        })
        .collect();
    assert_eq!(routes.len(), 23);
    assert_eq!(
        routes
            .iter()
            .filter(|row| {
                row.pointer("/base/resolution/reason") == Some(&serde_json::json!("site-route"))
            })
            .count(),
        21
    );
    assert_eq!(
        routes
            .iter()
            .filter(|row| {
                row.pointer("/base/resolution/reason")
                    == Some(&serde_json::json!("fragment-encoding"))
            })
            .count(),
        2
    );
    assert_eq!(
        routes
            .iter()
            .filter(|row| {
                row.pointer("/candidate/resolution/target/path")
                    == Some(&serde_json::json!("docs/guide.md"))
            })
            .count(),
        9
    );
    assert_generated_routes(&routes);
    assert_eq!(
        routes
            .iter()
            .filter(|row| {
                row.pointer("/candidate/resolution/reason")
                    == Some(&serde_json::json!("site-route"))
            })
            .count(),
        11,
        "unproved route uses remain explicitly unsupported: {routes:?}"
    );
    assert_eq!(
        envelope.pointer("/payload/summary/references/resolved"),
        Some(&serde_json::json!(12))
    );
    assert_eq!(
        envelope.pointer("/payload/summary/references/unsupported"),
        Some(&serde_json::json!(11))
    );
}

fn assert_generated_routes(routes: &[&serde_json::Value]) {
    let generated: Vec<&&serde_json::Value> = routes
        .iter()
        .filter(|row| {
            row.pointer("/candidate/resolution/reason") == Some(&serde_json::json!("site-build"))
        })
        .collect();
    assert_eq!(
        generated.len(),
        3,
        "generated pages resolve from build evidence without becoming repository blobs: {routes:?}"
    );
    assert!(
        generated
            .iter()
            .all(|row| row.pointer("/candidate/external_destination").is_none())
    );
}

fn assert_site_defects(envelope: &serde_json::Value) {
    let defects: Vec<&serde_json::Value> = envelope
        .pointer("/payload/findings")
        .and_then(serde_json::Value::as_array)
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "site-build-defect")
        .collect();
    assert_eq!(
        defects.len(),
        7,
        "complete route-table defects: {defects:?}"
    );
    for (route, kind, reason, members, attributed) in [
        (
            "/broken-fragment/",
            "broken-redirect",
            "missing-anchor",
            1,
            true,
        ),
        ("/broken/", "broken-redirect", "missing-route", 1, true),
        ("/collision/", "duplicate-route", "", 2, true),
        ("/duplicate/", "duplicate-route", "", 2, true),
        ("/generated-duplicate/", "duplicate-route", "", 2, false),
        (
            "/redirect-to-duplicate/",
            "broken-redirect",
            "ambiguous-route",
            1,
            true,
        ),
        (
            "/redirect-to-redirect/",
            "broken-redirect",
            "nonterminal-redirect",
            1,
            true,
        ),
    ] {
        let row = defects
            .iter()
            .find(|row| {
                row.pointer("/candidate_fact/evidence/route") == Some(&serde_json::json!(route))
            })
            .unwrap();
        assert_eq!(
            row.pointer("/candidate_fact/evidence/kind"),
            Some(&serde_json::json!(kind))
        );
        if reason.is_empty() {
            assert!(row.pointer("/candidate_fact/evidence/reason").is_none());
        } else {
            assert_eq!(
                row.pointer("/candidate_fact/evidence/reason"),
                Some(&serde_json::json!(reason))
            );
        }
        assert_eq!(
            row.pointer("/aggregation/member_count"),
            Some(&serde_json::json!(members))
        );
        assert_eq!(
            row.pointer("/location/path")
                .is_some_and(|path| !path.is_null()),
            attributed
        );
        if !attributed {
            assert_eq!(
                row.pointer("/candidate_fact/evidence/sources"),
                Some(&serde_json::json!([]))
            );
        }
    }
}

fn site_build_observations() -> Vec<Observation> {
    let pages = [
        site_observation(
            "/guide/",
            SiteObservation::Page(
                "docs/guide.md",
                &["intro", "mixed anchor%tail", "raw%anchor"],
            ),
        ),
        site_observation("/stale/", SiteObservation::Page("docs/removed.md", &[])),
        site_observation("/duplicate/", SiteObservation::Page("docs/guide.md", &[])),
        site_observation("/duplicate/", SiteObservation::Page("docs/index.md", &[])),
        site_observation("/collision/", SiteObservation::Page("docs/guide.md", &[])),
    ];
    let redirects = [
        site_observation(
            "/legacy/",
            SiteObservation::Redirect("docs/redirects.toml", "/guide/"),
        ),
        site_observation(
            "/changed/",
            SiteObservation::Redirect("docs/redirects.toml", "/guide/#intr%6F"),
        ),
        site_observation(
            "/cleared/",
            SiteObservation::Redirect("docs/redirects.toml", "/guide/#"),
        ),
        site_observation(
            "/raw-fragment/",
            SiteObservation::Redirect("docs/redirects.toml", "/guide/#raw%anchor"),
        ),
        site_observation(
            "/mixed-fragment/",
            SiteObservation::Redirect("docs/redirects.toml", "/guide/#mixed%20anchor%tail"),
        ),
        site_observation(
            "/top-fragment/",
            SiteObservation::Redirect("docs/redirects.toml", "/guide/#TOP"),
        ),
        site_observation(
            "/broken-fragment/",
            SiteObservation::Redirect("docs/redirects.toml", "/guide/#absent"),
        ),
        site_observation(
            "/broken/",
            SiteObservation::Redirect("docs/redirects.toml", "/missing/"),
        ),
        site_observation(
            "/collision/",
            SiteObservation::Redirect("docs/redirects.toml", "/guide/"),
        ),
        site_observation(
            "/redirect-to-duplicate/",
            SiteObservation::Redirect("docs/redirects.toml", "/duplicate/"),
        ),
        site_observation(
            "/redirect-to-redirect/",
            SiteObservation::Redirect("docs/redirects.toml", "/legacy/"),
        ),
        site_observation(
            "/generated-legacy/",
            SiteObservation::Redirect("docs/redirects.toml", "/generated/"),
        ),
    ];
    let generated = [
        site_observation("/generated/", SiteObservation::Generated(None, &["api"])),
        site_observation(
            "/generated-duplicate/",
            SiteObservation::Generated(None, &["first"]),
        ),
        site_observation(
            "/generated-duplicate/",
            SiteObservation::Generated(None, &["second"]),
        ),
        site_observation(
            "/generated-stale/",
            SiteObservation::Generated(Some("missing.config.js"), &[]),
        ),
    ];
    let navigation = [Ok(Observation::Site(SiteBuildObservation::Navigation {
        root: Nullable::Value("docs".parse().unwrap()),
        manifest: "docs/SUMMARY.md".parse().unwrap(),
        entrypoints: vec!["/generated/".to_owned()],
        reachable: vec!["docs/guide.md".parse().unwrap()],
    }))];

    pages
        .into_iter()
        .chain(redirects)
        .chain(generated)
        .chain(navigation)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

fn assert_unlinked(envelope: &serde_json::Value, expected: &[&str]) {
    let paths: Vec<&str> = envelope
        .pointer("/payload/findings")
        .and_then(serde_json::Value::as_array)
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "unlinked-document")
        .filter_map(|row| {
            row.pointer("/location/path")
                .and_then(serde_json::Value::as_str)
        })
        .collect();
    assert_eq!(paths, expected);
}

#[test]
fn stale_intersphinx_evidence_refuses_the_run() {
    let (fixture, evaluation, semantic) = intersphinx_case();
    let expected_context_digest = semantic.producer.context_digest;
    let stale = amiss_wire::semantic::envelope(SemanticEvidence {
        subject: SemanticSubject {
            candidate_identity_digest: hb("amiss-test/stale", b"another candidate"),
            source_report_payload_digest: Nullable::Null,
        },
        ..semantic
    })
    .unwrap();
    let streams = RequestStreams {
        evaluation: evaluation.canonical_bytes().unwrap(),
        snapshot: SnapshotRequest::git_objects().canonical_bytes().unwrap(),
        controls: ControlsRequest {
            semantic_evidence: vec![SuppliedSemanticEvidence {
                value: stale,
                expected_context_digest,
            }],
            ..ControlsRequest::default()
        }
        .canonical_bytes()
        .unwrap(),
    };
    let output = run(Some(&fixture.repo), &framed(&streams));
    assert_eq!(output.status.code(), Some(2), "{:?}", output.stderr);
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let errors = envelope["payload"]["errors"].as_array().unwrap();
    assert!(
        errors
            .iter()
            .any(|row| row["code"] == "CONTROL_BINDING_MISMATCH"),
        "stale evidence errors: {errors:?}"
    );
}
