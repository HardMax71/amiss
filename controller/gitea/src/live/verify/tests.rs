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
        path: &str,
        _deadline: OperationDeadline,
    ) -> Result<Presence, ProviderError> {
        Ok(*self
            .contents
            .get(&(name, reference, path))
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
            ("legacy", Visibility::Readable),
            ("pinned", Visibility::Readable),
            ("tagged", Visibility::Readable),
            ("walled", Visibility::Readable),
            ("widgets", Visibility::Readable),
        ]),
        refs_denied: BTreeSet::from(["walled"]),
        heads: BTreeMap::from([("gone", vec!["main"]), ("widgets", vec!["feature/x"])]),
        tags: BTreeMap::from([("tagged", vec!["v1.0"])]),
        contents: BTreeMap::from([
            (("widgets", "feature/x", "docs/a.md"), Presence::Present),
            (("tagged", "v1.0", "a.md"), Presence::Present),
            (("pinned", OID, "a.md"), Presence::Present),
        ]),
        commits: BTreeMap::from([(("pinned", OID), Presence::Present)]),
    }
}

#[test]
fn every_selector_and_visibility_becomes_its_fact() {
    let plan = plan_over(&[
        "https://codeberg.org/acme/bare",
        "https://codeberg.org/acme/deleted/src/branch/old-branch/a.md",
        "https://codeberg.org/acme/denied/src/branch/main/a.md",
        "https://codeberg.org/acme/gone/src/branch/main/missing.md",
        "https://codeberg.org/acme/legacy/src/main/docs/a.md",
        "https://codeberg.org/acme/missing/src/branch/main/a.md",
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
            "https://codeberg.org/acme/legacy/src/main/docs/a.md readable".to_owned(),
            "https://codeberg.org/acme/missing/src/branch/main/a.md missing".to_owned(),
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
