#![cfg(test)]

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use amiss_controller::ProviderError;
use amiss_wire::external::assess;
use amiss_wire::json::{Value, parse};

use super::super::rest::{GiteaVerification, OperationDeadline, Presence, RefFamily, Visibility};
use super::{PRODUCER_NAME, verify_external};

fn plan_over(destinations: &[&str]) -> Value {
    let report = amiss_fixtures::external_report(destinations);
    let parsed = parse(&report).expect("the report is strict JSON");
    let engine = parsed
        .member("payload")
        .and_then(|payload| payload.member("engine"))
        .expect("the report names its engine");
    amiss_wire::external::plan(
        &parsed,
        engine.text("engine_version").expect("a version"),
        engine.text("engine_digest").expect("a digest"),
    )
    .expect("the report yields a plan")
}

#[derive(Default)]
struct ScriptedRest {
    visibility: BTreeMap<&'static str, Visibility>,
    heads: BTreeMap<&'static str, Vec<&'static str>>,
    tags: BTreeMap<&'static str, Vec<&'static str>>,
    contents: BTreeMap<(&'static str, &'static str, &'static str), Presence>,
    commits: BTreeMap<(&'static str, &'static str), Presence>,
    refs_denied: BTreeSet<&'static str>,
}

impl GiteaVerification for ScriptedRest {
    fn deadline(&self) -> Result<OperationDeadline, ProviderError> {
        OperationDeadline::after(Duration::from_secs(30))
    }

    fn repository_visibility(
        &self,
        _owner: &str,
        name: &str,
        _deadline: OperationDeadline,
    ) -> Result<Visibility, ProviderError> {
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
        if self.refs_denied.contains(name) {
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
        Ok(*self
            .contents
            .get(&(name, reference, path.join("/").as_str()))
            .unwrap_or(&Presence::Absent))
    }

    fn commit_presence(
        &self,
        _owner: &str,
        name: &str,
        revision: &str,
        _deadline: OperationDeadline,
    ) -> Result<Presence, ProviderError> {
        Ok(*self
            .commits
            .get(&(name, revision))
            .unwrap_or(&Presence::Absent))
    }
}

fn facts(evidence: &Value) -> Vec<String> {
    let Some(Value::Array(rows)) = evidence.member("rows") else {
        panic!("the evidence holds rows");
    };
    rows.iter()
        .map(|row| {
            let destination = row.text("destination").expect("a destination");
            let repository = row.text("repository").expect("a repository fact");
            match row.text("tail") {
                Some(tail) => format!("{destination} {repository} {tail}"),
                None => format!("{destination} {repository}"),
            }
        })
        .collect()
}

const OID: &str = "0123456789abcdef0123456789abcdef01234567";

fn matrix_rest() -> ScriptedRest {
    ScriptedRest {
        visibility: BTreeMap::from([
            ("bare", Visibility::Readable),
            ("deleted", Visibility::Readable),
            ("denied", Visibility::Denied),
            ("gone", Visibility::Readable),
            ("large", Visibility::Readable),
            ("legacy", Visibility::Readable),
            ("opaque", Visibility::Readable),
            ("pinned", Visibility::Readable),
            ("tagged", Visibility::Readable),
            ("walled", Visibility::Readable),
            ("widgets", Visibility::Readable),
        ]),
        refs_denied: BTreeSet::from(["walled"]),
        heads: BTreeMap::from([
            ("gone", vec!["main"]),
            ("large", vec!["main"]),
            ("widgets", vec!["feature/x"]),
        ]),
        tags: BTreeMap::from([("tagged", vec!["v1.0"])]),
        contents: BTreeMap::from([
            (("large", "main", "big.bin"), Presence::Unknown),
            (("widgets", "feature/x", "docs/a.md"), Presence::Present),
            (("tagged", "v1.0", "a.md"), Presence::Present),
            (("pinned", OID, "a.md"), Presence::Present),
        ]),
        commits: BTreeMap::from([
            (("opaque", OID), Presence::Unknown),
            (("pinned", OID), Presence::Present),
        ]),
    }
}

#[test]
fn every_selector_and_visibility_becomes_its_fact() {
    let plan = plan_over(&[
        "https://codeberg.org/acme/bare",
        "https://codeberg.org/acme/deleted/src/branch/old-branch/a.md",
        "https://codeberg.org/acme/denied/src/branch/main/a.md",
        "https://codeberg.org/acme/gone/src/branch/main/missing.md",
        "https://codeberg.org/acme/large/src/branch/main/big.bin",
        "https://codeberg.org/acme/legacy/src/main/docs/a.md",
        "https://codeberg.org/acme/missing/src/branch/main/a.md",
        &format!("https://codeberg.org/acme/opaque/src/commit/{OID}/a.md"),
        &format!("https://codeberg.org/acme/pinned/src/commit/{OID}/a.md"),
        "https://codeberg.org/acme/tagged/src/tag/v1.0/a.md",
        "https://codeberg.org/acme/walled/src/branch/main/a.md",
        "https://codeberg.org/acme/widgets/src/branch/feature/x/docs/a.md",
        "https://github.com/acme/elsewhere",
    ]);
    let evidence = verify_external(&matrix_rest(), &plan, "codeberg.org", "0.0.0", "t0")
        .expect("evidence is produced");
    assert_eq!(
        facts(&evidence),
        vec![
            "https://codeberg.org/acme/bare readable".to_owned(),
            "https://codeberg.org/acme/deleted/src/branch/old-branch/a.md readable \
             revision-missing"
                .to_owned(),
            "https://codeberg.org/acme/denied/src/branch/main/a.md denied".to_owned(),
            "https://codeberg.org/acme/gone/src/branch/main/missing.md readable path-missing"
                .to_owned(),
            "https://codeberg.org/acme/large/src/branch/main/big.bin readable".to_owned(),
            "https://codeberg.org/acme/legacy/src/main/docs/a.md readable".to_owned(),
            "https://codeberg.org/acme/missing/src/branch/main/a.md missing".to_owned(),
            format!("https://codeberg.org/acme/opaque/src/commit/{OID}/a.md readable"),
            format!("https://codeberg.org/acme/pinned/src/commit/{OID}/a.md readable resolved"),
            "https://codeberg.org/acme/tagged/src/tag/v1.0/a.md readable resolved".to_owned(),
            "https://codeberg.org/acme/walled/src/branch/main/a.md readable".to_owned(),
            "https://codeberg.org/acme/widgets/src/branch/feature/x/docs/a.md readable resolved"
                .to_owned(),
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

/// The tail is sliced verbatim from the URL, so the ref and path after
/// the selector still wear the URL's percent-escapes while the forge
/// stores the decoded names. Each segment decodes exactly once after
/// splitting on the slashes the URL wrote: %20 finds the file whose name
/// holds the space, %2520 the name holding a literal %20, and %2F names a
/// slashed branch inside one segment. The selector still names the
/// family, but an escaped slash rewrites segmentation and the forge may
/// read the revision boundary elsewhere, so that spelling is only ever
/// confirmed, never refuted: veiled would otherwise be a false
/// revision-missing and coupled a false path-missing against a live page.
#[test]
fn escaped_spellings_resolve_and_never_refute() {
    let plan = plan_over(&[
        "https://codeberg.org/acme/coupled/src/branch/main/x%2Fy.md",
        "https://codeberg.org/acme/doubled/src/branch/main/My%2520File.md",
        "https://codeberg.org/acme/slashed/src/branch/release%2Fx/a.md",
        "https://codeberg.org/acme/spaced/src/branch/main/My%20File.md",
        "https://codeberg.org/acme/veiled/src/branch/release%2Fx/a.md",
    ]);
    let rest = ScriptedRest {
        visibility: BTreeMap::from([
            ("coupled", Visibility::Readable),
            ("doubled", Visibility::Readable),
            ("slashed", Visibility::Readable),
            ("spaced", Visibility::Readable),
            ("veiled", Visibility::Readable),
        ]),
        heads: BTreeMap::from([
            ("coupled", vec!["main"]),
            ("doubled", vec!["main"]),
            ("slashed", vec!["release/x"]),
            ("spaced", vec!["main"]),
        ]),
        contents: BTreeMap::from([
            (("doubled", "main", "My%20File.md"), Presence::Present),
            (("slashed", "release/x", "a.md"), Presence::Present),
            (("spaced", "main", "My File.md"), Presence::Present),
        ]),
        ..ScriptedRest::default()
    };
    let evidence =
        verify_external(&rest, &plan, "codeberg.org", "0.0.0", "t0").expect("evidence is produced");
    assert_eq!(
        facts(&evidence),
        vec![
            "https://codeberg.org/acme/coupled/src/branch/main/x%2Fy.md readable".to_owned(),
            "https://codeberg.org/acme/doubled/src/branch/main/My%2520File.md readable resolved"
                .to_owned(),
            "https://codeberg.org/acme/slashed/src/branch/release%2Fx/a.md readable resolved"
                .to_owned(),
            "https://codeberg.org/acme/spaced/src/branch/main/My%20File.md readable resolved"
                .to_owned(),
            "https://codeberg.org/acme/veiled/src/branch/release%2Fx/a.md readable".to_owned(),
        ],
    );
}

/// The whole chain: selector facts become verdicts through the engine.
#[test]
fn the_evidence_reaches_verdicts_through_the_engine() {
    let plan = plan_over(&[
        "https://codeberg.org/acme/gone/src/branch/main/missing.md",
        "https://codeberg.org/acme/private",
    ]);
    let evidence = verify_external(&matrix_rest(), &plan, "codeberg.org", "0.0.0", "t0")
        .expect("evidence is produced");
    let assessment = assess(
        &plan,
        &evidence,
        "0.0.0",
        &amiss_wire::digest::hj("t", &Value::Null).to_string(),
    )
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
