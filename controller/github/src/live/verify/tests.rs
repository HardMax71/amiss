#![cfg(test)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use amiss_controller::ProviderError;
use amiss_fixtures::{external_facts, external_plan};
use amiss_wire::digest::hj;
use amiss_wire::external::assess;
use amiss_wire::json::Value;

use super::super::rest::{GitHubVerification, OperationDeadline, Presence, RefFamily, Visibility};
use super::{PRODUCER_NAME, verify_external};

#[derive(Default)]
struct ScriptedRest {
    visibility: BTreeMap<&'static str, Visibility>,
    heads: BTreeMap<&'static str, Vec<&'static str>>,
    tags: BTreeMap<&'static str, Vec<&'static str>>,
    contents: BTreeMap<(&'static str, &'static str, Vec<&'static str>), Presence>,
    commits: BTreeMap<(&'static str, &'static str), Presence>,
    refs_unanswered: BTreeSet<&'static str>,
    calls: AtomicUsize,
    unavailable_from: Option<usize>,
}

impl ScriptedRest {
    fn spend(&self) -> Result<(), ProviderError> {
        let call = self.calls.fetch_add(1, Ordering::Relaxed);
        if self.unavailable_from.is_some_and(|limit| call >= limit) {
            return Err(ProviderError::Unavailable);
        }
        Ok(())
    }
}

impl GitHubVerification for ScriptedRest {
    fn deadline(&self) -> Result<OperationDeadline, ProviderError> {
        OperationDeadline::after(Duration::from_secs(30))
    }

    fn repository_visibility(
        &self,
        _owner: &str,
        name: &str,
        _deadline: OperationDeadline,
    ) -> Result<Visibility, ProviderError> {
        self.spend()?;
        Ok(*self.visibility.get(name).unwrap_or(&Visibility::Missing))
    }

    fn matching_refs(
        &self,
        _owner: &str,
        name: &str,
        family: RefFamily,
        prefix: &str,
        _deadline: OperationDeadline,
    ) -> Result<Option<Vec<String>>, ProviderError> {
        self.spend()?;
        if self.refs_unanswered.contains(name) {
            return Ok(None);
        }
        let table = match family {
            RefFamily::Heads => &self.heads,
            RefFamily::Tags => &self.tags,
        };
        Ok(Some(
            table
                .get(name)
                .into_iter()
                .flatten()
                .filter(|candidate| candidate.starts_with(prefix))
                .map(|candidate| (*candidate).to_owned())
                .collect(),
        ))
    }

    fn content_presence(
        &self,
        _owner: &str,
        name: &str,
        reference: &str,
        path: &[String],
        _deadline: OperationDeadline,
    ) -> Result<Presence, ProviderError> {
        self.spend()?;
        let segments: Vec<&str> = path.iter().map(String::as_str).collect();
        Ok(*self
            .contents
            .get(&(name, reference, segments))
            .unwrap_or(&Presence::Absent))
    }

    fn commit_presence(
        &self,
        _owner: &str,
        name: &str,
        oid: &str,
        _deadline: OperationDeadline,
    ) -> Result<Presence, ProviderError> {
        self.spend()?;
        Ok(*self.commits.get(&(name, oid)).unwrap_or(&Presence::Absent))
    }
}

const OID: &str = "0123456789abcdef0123456789abcdef01234567";

fn matrix_rest() -> ScriptedRest {
    ScriptedRest {
        visibility: BTreeMap::from([
            ("agreed", Visibility::Readable),
            ("bare", Visibility::Readable),
            ("bound", Visibility::Readable),
            ("deleted", Visibility::Readable),
            ("denied", Visibility::Denied),
            ("gone", Visibility::Readable),
            ("head", Visibility::Readable),
            ("large", Visibility::Readable),
            ("pinned", Visibility::Readable),
            ("refless", Visibility::Readable),
            ("shadow", Visibility::Readable),
            ("short", Visibility::Readable),
            ("tagged", Visibility::Readable),
            ("tickets", Visibility::Readable),
            ("widgets", Visibility::Readable),
        ]),
        refs_unanswered: BTreeSet::from(["refless"]),
        heads: BTreeMap::from([
            ("agreed", vec!["v1.0"]),
            ("bound", vec!["feature-x"]),
            ("shadow", vec!["feature"]),
            ("gone", vec!["main"]),
            ("large", vec!["main"]),
            ("widgets", vec!["feature/x"]),
        ]),
        tags: BTreeMap::from([
            ("agreed", vec!["v1.0"]),
            ("shadow", vec!["feature/x"]),
            ("tagged", vec!["v1.0"]),
        ]),
        contents: BTreeMap::from([
            (
                ("widgets", "feature/x", vec!["docs", "a.md"]),
                Presence::Present,
            ),
            (("widgets", "feature/x", vec!["docs"]), Presence::Present),
            (("tagged", "v1.0", vec!["a.md"]), Presence::Present),
            (("agreed", "v1.0", vec!["a.md"]), Presence::Present),
            (("pinned", OID, vec!["a.md"]), Presence::Present),
            (("large", "main", vec!["big.bin"]), Presence::Unknown),
            (("head", "HEAD", vec!["README.md"]), Presence::Present),
            (("short", "09059d9", vec!["a.md"]), Presence::Present),
        ]),
        commits: BTreeMap::from([
            (("pinned", OID), Presence::Present),
            (("head", "HEAD"), Presence::Present),
            (("short", "09059d9"), Presence::Present),
        ]),
        ..ScriptedRest::default()
    }
}

#[test]
fn every_visibility_and_resolution_becomes_its_fact() {
    let plan = external_plan(&[
        "https://github.com/acme/agreed/blob/v1.0/a.md",
        "https://github.com/acme/bare",
        "https://github.com/acme/bound/blob/feature/x/a.md",
        "https://github.com/acme/denied/blob/main/a.md",
        "https://github.com/acme/deleted/blob/old-branch/a.md",
        "https://github.com/acme/gone/blob/main/missing.md",
        "https://github.com/acme/head/blob/HEAD/README.md",
        "https://github.com/acme/large/blob/main/big.bin",
        "https://github.com/acme/missing/blob/main/a.md",
        "https://github.com/acme/refless/blob/main/a.md",
        &format!("https://github.com/acme/pinned/blob/{OID}/a.md"),
        "https://github.com/acme/shadow/blob/feature/x/y.md",
        "https://github.com/acme/short/blob/09059d9/a.md",
        "https://github.com/acme/tagged/blob/v1.0/a.md",
        "https://github.com/acme/tickets/issues/5",
        "https://github.com/acme/widgets/blob/feature/x/docs/a.md",
        "https://github.com/acme/widgets/tree/feature/x/docs/",
        "https://gitlab.com/acme/elsewhere",
    ])
    .expect("the report fixture yields a plan");
    let rest = matrix_rest();
    let evidence =
        verify_external(&rest, &plan, "github.com", "0.0.0", "t0").expect("evidence is produced");
    assert_eq!(
        external_facts(&evidence).expect("the evidence fixture has complete facts"),
        vec![
            "https://github.com/acme/agreed/blob/v1.0/a.md readable resolved".to_owned(),
            "https://github.com/acme/bare readable".to_owned(),
            "https://github.com/acme/bound/blob/feature/x/a.md readable revision-missing"
                .to_owned(),
            "https://github.com/acme/deleted/blob/old-branch/a.md readable revision-missing"
                .to_owned(),
            "https://github.com/acme/denied/blob/main/a.md denied".to_owned(),
            "https://github.com/acme/gone/blob/main/missing.md readable path-missing".to_owned(),
            "https://github.com/acme/head/blob/HEAD/README.md readable resolved".to_owned(),
            "https://github.com/acme/large/blob/main/big.bin readable".to_owned(),
            "https://github.com/acme/missing/blob/main/a.md missing".to_owned(),
            format!("https://github.com/acme/pinned/blob/{OID}/a.md readable resolved"),
            "https://github.com/acme/refless/blob/main/a.md readable".to_owned(),
            "https://github.com/acme/shadow/blob/feature/x/y.md readable".to_owned(),
            "https://github.com/acme/short/blob/09059d9/a.md readable resolved".to_owned(),
            "https://github.com/acme/tagged/blob/v1.0/a.md readable resolved".to_owned(),
            "https://github.com/acme/tickets/issues/5 readable".to_owned(),
            "https://github.com/acme/widgets/blob/feature/x/docs/a.md readable resolved".to_owned(),
            "https://github.com/acme/widgets/tree/feature/x/docs/ readable resolved".to_owned(),
        ],
    );
    assert_eq!(
        evidence
            .member("producer")
            .and_then(|producer| producer.text("name")),
        Some(PRODUCER_NAME)
    );
    assert_eq!(
        evidence.text("plan_payload_digest"),
        plan.text("payload_digest"),
        "the evidence binds the exact plan"
    );
}

/// The tail is sliced verbatim from the URL, so its segments still wear
/// the URL's percent-escapes while the forge stores the decoded names.
/// Each segment decodes exactly once after splitting on the slashes the
/// URL wrote: %20 finds the file whose name holds the space, %2520 the
/// name holding a literal %20, and %2F names a slashed ref inside one
/// segment, the way GitHub reads blob/release%2Fx. Decoding must not move
/// a negative, though. An escaped slash rewrites segmentation, and
/// GitHub's own greedy reading may place the revision boundary somewhere
/// this walk cannot see, so such a spelling is only ever confirmed, never
/// refuted: veiled would otherwise be a false revision-missing and
/// coupled a false path-missing against a live page.
#[test]
fn escaped_spellings_resolve_and_never_refute() {
    let plan = external_plan(&[
        "https://github.com/acme/coupled/blob/main/x%2Fy.md",
        "https://github.com/acme/doubled/blob/main/My%2520File.md",
        "https://github.com/acme/nested/blob/main/docs/api.md",
        "https://github.com/acme/slashed/blob/release%2Fx/a.md",
        "https://github.com/acme/spaced/blob/main/My%20File.md",
        "https://github.com/acme/veiled/blob/release%2Fx/a.md",
    ])
    .expect("the report fixture yields a plan");
    let rest = ScriptedRest {
        visibility: BTreeMap::from([
            ("coupled", Visibility::Readable),
            ("doubled", Visibility::Readable),
            ("nested", Visibility::Readable),
            ("slashed", Visibility::Readable),
            ("spaced", Visibility::Readable),
            ("veiled", Visibility::Readable),
        ]),
        heads: BTreeMap::from([
            ("coupled", vec!["main"]),
            ("doubled", vec!["main"]),
            ("nested", vec!["main"]),
            ("slashed", vec!["release/x"]),
            ("spaced", vec!["main"]),
        ]),
        contents: BTreeMap::from([
            (("doubled", "main", vec!["My%20File.md"]), Presence::Present),
            // A real path slash reaches contents as two segments, so a builder
            // that joined and re-split would look up the wrong key and miss.
            (
                ("nested", "main", vec!["docs", "api.md"]),
                Presence::Present,
            ),
            (("slashed", "release/x", vec!["a.md"]), Presence::Present),
            (("spaced", "main", vec!["My File.md"]), Presence::Present),
        ]),
        ..ScriptedRest::default()
    };
    let evidence =
        verify_external(&rest, &plan, "github.com", "0.0.0", "t0").expect("evidence is produced");
    assert_eq!(
        external_facts(&evidence).expect("the evidence fixture has complete facts"),
        vec![
            "https://github.com/acme/coupled/blob/main/x%2Fy.md readable".to_owned(),
            "https://github.com/acme/doubled/blob/main/My%2520File.md readable resolved".to_owned(),
            "https://github.com/acme/nested/blob/main/docs/api.md readable resolved".to_owned(),
            "https://github.com/acme/slashed/blob/release%2Fx/a.md readable resolved".to_owned(),
            "https://github.com/acme/spaced/blob/main/My%20File.md readable resolved".to_owned(),
            "https://github.com/acme/veiled/blob/release%2Fx/a.md readable".to_owned(),
        ],
    );
}

/// A standing unavailability ends the walk with the rows already learned:
/// partial evidence beats none, and the skipped rest stays unproven.
#[test]
fn a_rate_limit_keeps_the_partial_evidence() {
    let plan = external_plan(&[
        "https://github.com/acme/first",
        "https://github.com/acme/second",
    ])
    .expect("the report fixture yields a plan");
    let rest = ScriptedRest {
        visibility: BTreeMap::from([
            ("first", Visibility::Readable),
            ("second", Visibility::Readable),
        ]),
        unavailable_from: Some(1),
        ..ScriptedRest::default()
    };
    let evidence =
        verify_external(&rest, &plan, "github.com", "0.0.0", "t0").expect("partial evidence");
    assert_eq!(
        external_facts(&evidence).expect("the evidence fixture has complete facts"),
        vec!["https://github.com/acme/first readable".to_owned()],
    );
}

/// The whole chain: scripted facts become evidence the engine judges.
#[test]
fn the_evidence_reaches_verdicts_through_the_engine() {
    let plan = external_plan(&[
        "https://github.com/acme/gone/blob/main/missing.md",
        "https://github.com/acme/private",
    ])
    .expect("the report fixture yields a plan");
    let rest = ScriptedRest {
        visibility: BTreeMap::from([
            ("gone", Visibility::Readable),
            ("private", Visibility::Missing),
        ]),
        heads: BTreeMap::from([("gone", vec!["main"])]),
        ..ScriptedRest::default()
    };
    let evidence =
        verify_external(&rest, &plan, "github.com", "0.0.0", "t0").expect("evidence is produced");
    let assessment = assess(&plan, &evidence, "0.0.0", hj("t", &Value::Null))
        .expect("the engine judges the evidence");
    let Some(Value::Array(verdicts)) = assessment
        .member("payload")
        .and_then(|payload| payload.member("verdicts"))
    else {
        panic!("the assessment holds verdicts");
    };
    let verdicts: Vec<(Option<&str>, Option<&str>)> = verdicts
        .iter()
        .map(|row| (row.text("verdict"), row.text("reason")))
        .collect();
    assert_eq!(
        verdicts,
        vec![
            (Some("refuted"), Some("path-missing")),
            (Some("unproven"), Some("repository-unseen")),
        ],
    );
}
