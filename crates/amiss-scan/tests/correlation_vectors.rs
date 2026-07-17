#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration assertions over repository-owned correlation vectors"
)]

use std::collections::{BTreeMap, BTreeSet};

use amiss_md::extract::BlockKind;
use amiss_scan::correlate::{Observation, Outcome, Side, correlate};
use amiss_scan::observe::intent_value;
use amiss_scan::resolve::{Intent, Resolution};
use amiss_scan::scan::SpanDisplay;
use amiss_wire::controls::{ContentAvailability, SourceConstruct, TargetKind};
use amiss_wire::digest::{Digest, hb};
use amiss_wire::json::{Value, canonical, parse};
use amiss_wire::model::{Adapter, RepoPath};
use amiss_wire::report::{IntentKind, ResolutionCode};

mod support;

use support::{ReportSchemaFragment, fixture_bytes};

const REQUIRED_VECTOR_IDS: [&str; 23] = [
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
];

fn field<'a>(members: &'a [(String, Value)], name: &str) -> &'a Value {
    members
        .iter()
        .find(|(key, _)| key == name)
        .map_or_else(|| panic!("missing field {name}"), |(_, value)| value)
}

fn text(value: &Value, label: &str) -> String {
    let Value::String(text) = value else {
        panic!("{label} must be a string, found {value:?}")
    };
    text.clone()
}

fn optional_text(value: &Value, label: &str) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        other @ (Value::Bool(_) | Value::Integer(_) | Value::Array(_) | Value::Object(_)) => {
            panic!("{label} must be a string or null, found {other:?}")
        }
    }
}

fn digest(value: &Value, label: &str) -> Digest {
    Digest::from_wire(&text(value, label))
        .unwrap_or_else(|| panic!("{label} must be a canonical digest"))
}

fn optional_digest(value: &Value, label: &str) -> Option<Digest> {
    optional_text(value, label).map(|raw| {
        Digest::from_wire(&raw).unwrap_or_else(|| panic!("{label} must be a canonical digest"))
    })
}

fn intent_kind(value: &Value) -> IntentKind {
    match text(value, "kind").as_str() {
        "repository-path" => IntentKind::RepositoryPath,
        "same-repository-github" => IntentKind::SameRepositoryGithub,
        "same-repository-gitlab" => IntentKind::SameRepositoryGitlab,
        "same-repository-gitea" => IntentKind::SameRepositoryGitea,
        "external-url" => IntentKind::ExternalUrl,
        "site-route" => IntentKind::SiteRoute,
        "unsupported" => IntentKind::Unsupported,
        other => panic!("the correlation harness does not know intent kind {other}"),
    }
}

fn target_kind(value: &Value) -> Option<TargetKind> {
    optional_text(value, "target_kind").map(|kind| match kind.as_str() {
        "blob" => TargetKind::Blob,
        "tree" => TargetKind::Tree,
        "either" => TargetKind::Either,
        other => panic!("the correlation harness does not know target kind {other}"),
    })
}

fn preimages(value: &Value, label: &str) -> BTreeMap<Digest, String> {
    let Value::Array(rows) = value else {
        panic!("{label} must be an array")
    };
    assert!(rows.len() >= 2, "{label} pins distinct component values");
    let mut out = BTreeMap::new();
    for row in rows {
        let Value::Object(members) = row else {
            panic!("a {label} row must be an object")
        };
        assert_eq!(members.len(), 2, "a {label} row shape is closed");
        let value = text(field(members, "value"), "component value");
        assert!(!value.is_empty(), "{label} values are nonempty");
        let digest = digest(field(members, "digest"), "component digest");
        assert!(
            out.insert(digest, value).is_none(),
            "{label} repeats a digest"
        );
    }
    out
}

struct FixtureIntent {
    kind: IntentKind,
    raw_destination_digest: Digest,
    repository_path: Option<RepoPath>,
    target_kind: Option<TargetKind>,
    query_digest: Option<Digest>,
    fragment_digest: Option<Digest>,
    external_scheme: Option<String>,
}

impl FixtureIntent {
    fn parse(value: &Value, label: &str) -> Self {
        let Value::Object(members) = value else {
            panic!("{label} must be an object")
        };
        Self {
            kind: intent_kind(field(members, "kind")),
            raw_destination_digest: digest(
                field(members, "raw_destination_digest"),
                "raw_destination_digest",
            ),
            repository_path: optional_text(field(members, "repository_path"), "repository_path")
                .map(|path| {
                    RepoPath::new(path)
                        .unwrap_or_else(|| panic!("{label} repository_path must be canonical"))
                }),
            target_kind: target_kind(field(members, "target_kind")),
            query_digest: optional_digest(field(members, "query_digest"), "query_digest"),
            fragment_digest: optional_digest(field(members, "fragment_digest"), "fragment_digest"),
            external_scheme: optional_text(field(members, "external_scheme"), "external_scheme"),
        }
    }

    fn intent(
        &self,
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
            kind: self.kind,
            repository_path: self.repository_path.clone(),
            target_kind: self.target_kind,
            external_scheme: self.external_scheme.clone(),
            query: lookup(self.query_digest, query_preimages, "query"),
            fragment: lookup(self.fragment_digest, fragment_preimages, "fragment"),
        }
    }
}

fn observation(id: &str, side: &str, fixture: &FixtureIntent, intent: Intent) -> Observation {
    let identity = format!("{id}:{side}:identity");
    let projection = format!("{id}:{side}:projection");
    Observation {
        id: hb("amiss/test-correlation-vector-id", identity.as_bytes()),
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
        intent,
        raw_destination_digest: fixture.raw_destination_digest,
        projection_digest: hb(
            "amiss/test-correlation-vector-projection",
            projection.as_bytes(),
        ),
        resolution: Resolution {
            code: ResolutionCode::ExternalUrl,
            path: None,
            entry_kind: None,
            git_mode: None,
            raw_digest: None,
            projection_digest: None,
            content_availability: ContentAvailability::NotApplicable,
        },
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

    let Value::Object(root) = parse(&bytes).expect("the correlation vectors are strict JSON")
    else {
        panic!("the correlation vectors must be an object")
    };
    assert_eq!(root.len(), 5, "the vector root shape is closed");
    assert_eq!(
        field(&root, "schema"),
        &Value::String("amiss/correlation-intent-vectors".to_owned())
    );
    assert_eq!(
        field(&root, "contract"),
        &Value::String("correlation-intent".to_owned())
    );
    let query_preimages = preimages(field(&root, "query_preimages"), "query_preimages");
    let fragment_preimages = preimages(field(&root, "fragment_preimages"), "fragment_preimages");
    let Value::Array(cases) = field(&root, "cases") else {
        panic!("the correlation vectors must hold cases")
    };
    assert!(
        cases.len() >= REQUIRED_VECTOR_IDS.len(),
        "the pinned corpus only grows"
    );

    let mut ids = BTreeSet::new();
    for case in cases {
        let Value::Object(members) = case else {
            panic!("a correlation case must be an object")
        };
        assert_eq!(members.len(), 4, "a correlation case shape is closed");
        let id = text(field(members, "id"), "id");
        assert!(ids.insert(id.clone()), "duplicate correlation case {id}");
        let left_value = field(members, "left");
        let right_value = field(members, "right");
        let left = FixtureIntent::parse(left_value, &format!("{id} left"));
        let right = FixtureIntent::parse(right_value, &format!("{id} right"));
        let left_intent = left.intent(&query_preimages, &fragment_preimages, &format!("{id} left"));
        let right_intent = right.intent(
            &query_preimages,
            &fragment_preimages,
            &format!("{id} right"),
        );
        assert_eq!(
            canonical(&intent_value(&left_intent, left.raw_destination_digest)),
            canonical(left_value),
            "{id} left target-intent preimage"
        );
        assert_eq!(
            canonical(&intent_value(&right_intent, right.raw_destination_digest)),
            canonical(right_value),
            "{id} right target-intent preimage"
        );

        let rows = correlate(
            &Side {
                observations: vec![observation(&id, "left", &left, left_intent)],
                documents: BTreeMap::new(),
            },
            &Side {
                observations: vec![observation(&id, "right", &right, right_intent)],
                documents: BTreeMap::new(),
            },
        )
        .expect("the vector observations correlate");
        let Value::Bool(expected) = field(members, "expected_equal") else {
            panic!("{id} expected_equal must be a boolean")
        };
        if *expected {
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
