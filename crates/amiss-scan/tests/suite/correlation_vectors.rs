#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration assertions over repository-owned correlation vectors"
)]

use sha2::Digest as _;
use std::collections::{BTreeMap, BTreeSet};

use amiss_md::extract::BlockKind;
use amiss_scan::correlate::{Observation, Outcome, Side, correlate};
use amiss_scan::observe::target_intent;
use amiss_scan::resolve::{Intent, Resolution};
use amiss_scan::scan::SpanDisplay;
use amiss_wire::controls::SourceConstruct;
use amiss_wire::model::Digest;
use amiss_wire::model::{Adapter, RepoPath, RepoPathText};
use amiss_wire::report::model::TargetIntent;
use amiss_wire::resolution::ExternalReference;

use crate::support;

use support::{ReportSchemaFragment, fixture_bytes};

const REQUIRED_VECTOR_IDS: [&str; 25] = [
    "CI-001-native-github-equivalent",
    "CI-002-repository-path-changed",
    "CI-003-target-kind-changed",
    "CI-004-query-presence-changed",
    "CI-005-external-identical",
    "CI-006-external-raw-spelling-changed",
    "CI-007-site-route-identical",
    "CI-008-unsupported-raw-changed",
    "CI-009-native-gitlab-equivalent",
    "CI-010-native-gitea-equivalent",
    "CI-011-fragment-changed",
    "CI-012-external-scheme-changed",
    "CI-013-site-route-raw-changed",
    "CI-014-other-kind-changed",
    "CI-015-repository-query-digest-changed",
    "CI-016-repository-fragment-digest-changed",
    "CI-017-external-query-digest-changed",
    "CI-018-external-fragment-digest-changed",
    "CI-019-site-route-query-digest-changed",
    "CI-020-unsupported-fragment-digest-changed",
    "CI-021-repository-components-identical",
    "CI-022-external-components-identical",
    "CI-023-other-components-identical",
    "CI-024-native-bitbucket-cloud-equivalent",
    "CI-025-native-bitbucket-data-center-equivalent",
];

type FixtureIntent = TargetIntent<RepoPathText>;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Vectors {
    schema: String,
    contract: String,
    query_preimages: Vec<Preimage>,
    fragment_preimages: Vec<Preimage>,
    cases: Vec<Vector>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Vector {
    id: String,
    left: FixtureIntent,
    right: FixtureIntent,
    expected_equal: bool,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Preimage {
    value: String,
    digest: Digest,
}

fn preimages(rows: Vec<Preimage>, label: &str) -> BTreeMap<Digest, String> {
    assert!(rows.len() >= 2, "{label} pins distinct component values");
    let mut out = BTreeMap::new();
    for Preimage { value, digest } in rows {
        assert!(!value.is_empty(), "{label} values are nonempty");
        assert!(
            out.insert(digest, value).is_none(),
            "{label} repeats a digest"
        );
    }
    out
}

fn fixture_intent(
    fixture: &FixtureIntent,
    query_preimages: &BTreeMap<Digest, String>,
    fragment_preimages: &BTreeMap<Digest, String>,
    label: &str,
) -> Intent {
    let lookup = |digest: Option<Digest>, values: &BTreeMap<Digest, String>, kind: &str| {
        digest.map(|value| {
            values
                .get(&value)
                .unwrap_or_else(|| panic!("{label} has no {kind} preimage for {value}"))
                .clone()
        })
    };
    Intent {
        kind: fixture.kind,
        commit_oid: fixture.commit_oid.clone(),
        repository_path: fixture.repository_path.as_ref().map(RepoPath::from),
        target_kind: fixture.target_kind,
        external_scheme: fixture.external_scheme.clone(),
        query: lookup(fixture.query_digest, query_preimages, "query"),
        fragment: lookup(fixture.fragment_digest, fragment_preimages, "fragment"),
    }
}

fn observation(id: &str, side: &str, fixture: &FixtureIntent, intent: Intent) -> Observation {
    let identity = format!("{id}:{side}:identity");
    let projection = format!("{id}:{side}:projection");
    Observation {
        id: Digest::from(
            sha2::Sha256::new_with_prefix("amiss/test-correlation-vector-id")
                .chain_update([0_u8])
                .chain_update(identity.as_bytes())
                .finalize()
                .0,
        ),
        adapter_contract_digest: Digest::from(
            sha2::Sha256::new_with_prefix("amiss/test-adapter-contract")
                .chain_update([0_u8])
                .chain_update(b"markdown")
                .finalize()
                .0,
        ),
        document: RepoPath::new("docs/source.md".to_owned()).expect("the test path is canonical"),
        span: (0, 1),
        display: SpanDisplay {
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 2,
        },
        block_kind: BlockKind::Paragraph,
        node_path: vec![usize::from(side != "left")],
        adapter: Adapter::Markdown,
        construct: SourceConstruct::InlineLink,
        external_destination: None,
        intent,
        raw_destination: String::new(),
        raw_destination_digest: fixture.raw_destination_digest,
        projection_digest: Digest::from(
            sha2::Sha256::new_with_prefix("amiss/test-correlation-vector-projection")
                .chain_update([0_u8])
                .chain_update(projection.as_bytes())
                .finalize()
                .0,
        ),
        resolution: Resolution::External {
            reason: ExternalReference::Url,
        },
        fragment_span: None,
        path_span: None,
    }
}

fn validate_target_intents(bytes: &[u8]) {
    let fixture: serde_json::Value =
        serde_json::from_slice(bytes).expect("the correlation vectors are JSON");
    let cases = fixture
        .get("cases")
        .and_then(serde_json::Value::as_array)
        .expect("the correlation vectors hold cases");
    let target_intent = ReportSchemaFragment::new("TargetIntent");
    for case in cases {
        let id = case
            .get("id")
            .and_then(serde_json::Value::as_str)
            .expect("a correlation case has an id");
        for side in ["left", "right"] {
            target_intent.assert_value(
                case.get(side)
                    .unwrap_or_else(|| panic!("{id} has no {side} intent")),
                &format!("{id} {side}"),
            );
        }
    }
}

#[test]
fn the_published_vectors_execute_live_correlation() {
    let bytes = fixture_bytes("correlation-intent-vectors.json");
    validate_target_intents(&bytes);
    let vectors: Vectors =
        serde_json::from_slice(&bytes).expect("the correlation vectors have the published shape");
    assert_eq!(vectors.schema, "amiss/correlation-intent-vectors");
    assert_eq!(vectors.contract, "correlation-intent");
    let query_preimages = preimages(vectors.query_preimages, "query_preimages");
    let fragment_preimages = preimages(vectors.fragment_preimages, "fragment_preimages");
    assert!(
        vectors.cases.len() >= REQUIRED_VECTOR_IDS.len(),
        "the pinned corpus only grows"
    );
    let mut ids = BTreeSet::new();
    for Vector {
        id,
        left,
        right,
        expected_equal,
    } in vectors.cases
    {
        assert!(ids.insert(id.clone()), "duplicate correlation case {id}");
        let left_intent = fixture_intent(
            &left,
            &query_preimages,
            &fragment_preimages,
            &format!("{id} left"),
        );
        let right_intent = fixture_intent(
            &right,
            &query_preimages,
            &fragment_preimages,
            &format!("{id} right"),
        );
        assert_eq!(
            serde_json::to_vec(&target_intent(
                &left_intent,
                left.raw_destination_digest,
                left_intent.repository_path.as_ref(),
            ))
            .unwrap(),
            serde_json_canonicalizer::to_vec(&left).unwrap(),
            "{id} left target-intent preimage"
        );
        assert_eq!(
            serde_json::to_vec(&target_intent(
                &right_intent,
                right.raw_destination_digest,
                right_intent.repository_path.as_ref(),
            ))
            .unwrap(),
            serde_json_canonicalizer::to_vec(&right).unwrap(),
            "{id} right target-intent preimage"
        );
        let rows = correlate(
            Side {
                observations: vec![observation(&id, "left", &left, left_intent)],
                documents: BTreeMap::new(),
            },
            Side {
                observations: vec![observation(&id, "right", &right, right_intent)],
                documents: BTreeMap::new(),
            },
        )
        .expect("the vector observations correlate");
        if expected_equal {
            assert_eq!(rows.len(), 1, "{id} forms one candidate component");
            assert_eq!(
                rows.first().expect("one candidate component").outcome,
                Outcome::Candidate,
                "{id}"
            );
        } else {
            assert_eq!(rows.len(), 2, "{id} remains two isolated observations");
            assert!(rows.iter().all(|row| row.outcome == Outcome::None), "{id}");
        }
    }
    for required in REQUIRED_VECTOR_IDS {
        assert!(
            ids.contains(required),
            "the published correlation corpus lost {required}"
        );
    }
}
