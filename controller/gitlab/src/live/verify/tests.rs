#![cfg(test)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use amiss_controller::ProviderError;
use amiss_fixtures::{external_facts, external_plan};
use amiss_wire::digest::hj;
use amiss_wire::external::{ExternalReason, ExternalVerdict, assess};
use amiss_wire::json::Value;

use super::super::transport::Budget;
use super::{GitLabVerification, PRODUCER_NAME, Presence, RefFamily, Visibility, verify_external};

#[derive(Default)]
struct ScriptedRest {
    visibility: BTreeMap<&'static str, Visibility>,
    heads: BTreeMap<&'static str, Vec<&'static str>>,
    tags: BTreeMap<&'static str, Vec<&'static str>>,
    files: BTreeMap<(&'static str, &'static str, &'static str), Presence>,
    trees: BTreeMap<(&'static str, &'static str, &'static str), Presence>,
    commits: BTreeMap<(&'static str, &'static str), Presence>,
    refs_denied: BTreeSet<&'static str>,
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

fn scripted<K: Ord + Copy>(table: &BTreeMap<K, Presence>, key: K) -> Presence {
    *table.get(&key).unwrap_or(&Presence::Absent)
}

impl GitLabVerification for ScriptedRest {
    fn budget(&self) -> Result<Budget, ProviderError> {
        Budget::after(Duration::from_secs(30), 4 * 1024 * 1024)
    }

    fn project_visibility(
        &self,
        project: &str,
        budget: Budget,
    ) -> Result<(Visibility, Budget), ProviderError> {
        self.spend()?;
        Ok((
            *self.visibility.get(project).unwrap_or(&Visibility::Missing),
            budget,
        ))
    }

    fn matching_refs(
        &self,
        project: &str,
        family: RefFamily,
        prefix: &str,
        budget: Budget,
    ) -> Result<(Option<Vec<String>>, Budget), ProviderError> {
        self.spend()?;
        if self.refs_denied.contains(project) {
            return Ok((None, budget));
        }
        let table = match family {
            RefFamily::Heads => &self.heads,
            RefFamily::Tags => &self.tags,
        };
        Ok((
            Some(
                table
                    .get(project)
                    .into_iter()
                    .flatten()
                    .filter(|candidate| candidate.starts_with(prefix))
                    .map(|candidate| (*candidate).to_owned())
                    .collect(),
            ),
            budget,
        ))
    }

    fn file_presence(
        &self,
        project: &str,
        reference: &str,
        path: &str,
        budget: Budget,
    ) -> Result<(Presence, Budget), ProviderError> {
        self.spend()?;
        Ok((scripted(&self.files, (project, reference, path)), budget))
    }

    fn tree_presence(
        &self,
        project: &str,
        reference: &str,
        path: &str,
        budget: Budget,
    ) -> Result<(Presence, Budget), ProviderError> {
        self.spend()?;
        Ok((scripted(&self.trees, (project, reference, path)), budget))
    }

    fn commit_presence(
        &self,
        project: &str,
        revision: &str,
        budget: Budget,
    ) -> Result<(Presence, Budget), ProviderError> {
        self.spend()?;
        Ok((scripted(&self.commits, (project, revision)), budget))
    }
}

const OID: &str = "0123456789abcdef0123456789abcdef01234567";

fn matrix_rest() -> ScriptedRest {
    ScriptedRest {
        visibility: BTreeMap::from([
            ("acme/agreed", Visibility::Readable),
            ("acme/bare", Visibility::Readable),
            ("acme/deleted", Visibility::Readable),
            ("acme/denied", Visibility::Denied),
            ("acme/gone", Visibility::Readable),
            ("acme/group/widgets", Visibility::Readable),
            ("acme/head", Visibility::Readable),
            ("acme/hollow", Visibility::Readable),
            ("acme/large", Visibility::Readable),
            ("acme/pinned", Visibility::Readable),
            ("acme/refless", Visibility::Readable),
            ("acme/shadow", Visibility::Readable),
            ("acme/tagged", Visibility::Readable),
            ("acme/tickets", Visibility::Readable),
            ("acme/trees", Visibility::Readable),
        ]),
        refs_denied: BTreeSet::from(["acme/refless"]),
        heads: BTreeMap::from([
            ("acme/agreed", vec!["v1.0"]),
            ("acme/gone", vec!["main"]),
            ("acme/group/widgets", vec!["feature/x"]),
            ("acme/hollow", vec!["main"]),
            ("acme/large", vec!["main"]),
            ("acme/shadow", vec!["feature"]),
            ("acme/trees", vec!["main"]),
        ]),
        tags: BTreeMap::from([
            ("acme/agreed", vec!["v1.0"]),
            ("acme/shadow", vec!["feature/x"]),
            ("acme/tagged", vec!["v1.0"]),
        ]),
        files: BTreeMap::from([
            (("acme/agreed", "v1.0", "a.md"), Presence::Present),
            (
                ("acme/group/widgets", "feature/x", "docs/a.md"),
                Presence::Present,
            ),
            (("acme/head", "HEAD", "README.md"), Presence::Present),
            (("acme/large", "main", "big.bin"), Presence::Unknown),
            (("acme/pinned", OID, "a.md"), Presence::Present),
            (("acme/tagged", "v1.0", "a.md"), Presence::Present),
        ]),
        trees: BTreeMap::from([
            (("acme/hollow", "main", "void"), Presence::Unknown),
            (("acme/trees", "main", "docs"), Presence::Present),
        ]),
        commits: BTreeMap::from([
            (("acme/head", "HEAD"), Presence::Present),
            (("acme/pinned", OID), Presence::Present),
        ]),
        ..ScriptedRest::default()
    }
}

#[test]
fn every_visibility_and_resolution_becomes_its_fact() {
    let plan = external_plan(&[
        "https://github.com/acme/elsewhere",
        "https://gitlab.com/acme/agreed/-/blob/v1.0/a.md",
        "https://gitlab.com/acme/bare",
        "https://gitlab.com/acme/deleted/-/blob/old-branch/a.md",
        "https://gitlab.com/acme/denied/-/blob/main/a.md",
        "https://gitlab.com/acme/gone/-/blob/main/missing.md",
        "https://gitlab.com/acme/group/widgets/-/blob/feature/x/docs/a.md",
        "https://gitlab.com/acme/head/-/blob/HEAD/README.md",
        "https://gitlab.com/acme/hollow/-/tree/main/void",
        "https://gitlab.com/acme/large/-/blob/main/big.bin",
        "https://gitlab.com/acme/legacy/blob/main/a.md",
        "https://gitlab.com/acme/missing/-/blob/main/a.md",
        &format!("https://gitlab.com/acme/pinned/-/blob/{OID}/a.md"),
        "https://gitlab.com/acme/refless/-/blob/main/a.md",
        "https://gitlab.com/acme/shadow/-/blob/feature/x/y.md",
        "https://gitlab.com/acme/tagged/-/blob/v1.0/a.md",
        "https://gitlab.com/acme/tickets/-/issues/5",
        "https://gitlab.com/acme/trees/-/tree/main/docs/",
    ])
    .expect("the report fixture yields a plan");
    let evidence = verify_external(&matrix_rest(), &plan, "gitlab.com", "0.0.0", "t0")
        .expect("evidence is produced");
    assert_eq!(
        external_facts(&evidence).expect("the evidence fixture has complete facts"),
        vec![
            "https://gitlab.com/acme/agreed/-/blob/v1.0/a.md readable resolved".to_owned(),
            "https://gitlab.com/acme/bare readable".to_owned(),
            "https://gitlab.com/acme/deleted/-/blob/old-branch/a.md readable revision-missing"
                .to_owned(),
            "https://gitlab.com/acme/denied/-/blob/main/a.md denied".to_owned(),
            "https://gitlab.com/acme/gone/-/blob/main/missing.md readable path-missing".to_owned(),
            "https://gitlab.com/acme/group/widgets/-/blob/feature/x/docs/a.md readable resolved"
                .to_owned(),
            "https://gitlab.com/acme/head/-/blob/HEAD/README.md readable resolved".to_owned(),
            "https://gitlab.com/acme/hollow/-/tree/main/void readable".to_owned(),
            "https://gitlab.com/acme/large/-/blob/main/big.bin readable".to_owned(),
            "https://gitlab.com/acme/missing/-/blob/main/a.md missing".to_owned(),
            format!("https://gitlab.com/acme/pinned/-/blob/{OID}/a.md readable resolved"),
            "https://gitlab.com/acme/refless/-/blob/main/a.md readable".to_owned(),
            "https://gitlab.com/acme/shadow/-/blob/feature/x/y.md readable".to_owned(),
            "https://gitlab.com/acme/tagged/-/blob/v1.0/a.md readable resolved".to_owned(),
            "https://gitlab.com/acme/tickets/-/issues/5 readable".to_owned(),
            "https://gitlab.com/acme/trees/-/tree/main/docs/ readable resolved".to_owned(),
        ],
    );
    let document = amiss_wire::external::parse_evidence(&evidence).expect("the evidence is valid");
    assert_eq!(document.producer.name, PRODUCER_NAME);
    assert_eq!(
        document.plan_payload_digest,
        amiss_wire::external::parse_plan(&plan)
            .expect("the plan is valid")
            .payload_digest,
        "the evidence binds the exact plan"
    );
}

/// The tail is sliced verbatim from the URL, so its segments still wear
/// the URL's percent-escapes while the forge stores the decoded names.
/// Each segment decodes exactly once after splitting on the slashes the
/// URL wrote: %20 finds the file whose name holds the space, %2520 the
/// name holding a literal %20, and %2F names a slashed ref inside one
/// segment. The file route then takes the whole path as one parameter, so
/// a decoded slash cannot be told apart from a written one, which is one
/// more reason a spelling whose escaped slash rewrites segmentation is
/// only ever confirmed, never refuted: veiled would otherwise be a false
/// revision-missing and coupled a false path-missing against a live page.
#[test]
fn escaped_spellings_resolve_and_never_refute() {
    let plan = external_plan(&[
        "https://gitlab.com/acme/coupled/-/blob/main/x%2Fy.md",
        "https://gitlab.com/acme/doubled/-/blob/main/My%2520File.md",
        "https://gitlab.com/acme/slashed/-/blob/release%2Fx/a.md",
        "https://gitlab.com/acme/spaced/-/blob/main/My%20File.md",
        "https://gitlab.com/acme/veiled/-/blob/release%2Fx/a.md",
    ])
    .expect("the report fixture yields a plan");
    let rest = ScriptedRest {
        visibility: BTreeMap::from([
            ("acme/coupled", Visibility::Readable),
            ("acme/doubled", Visibility::Readable),
            ("acme/slashed", Visibility::Readable),
            ("acme/spaced", Visibility::Readable),
            ("acme/veiled", Visibility::Readable),
        ]),
        heads: BTreeMap::from([
            ("acme/coupled", vec!["main"]),
            ("acme/doubled", vec!["main"]),
            ("acme/slashed", vec!["release/x"]),
            ("acme/spaced", vec!["main"]),
        ]),
        files: BTreeMap::from([
            (("acme/doubled", "main", "My%20File.md"), Presence::Present),
            (("acme/slashed", "release/x", "a.md"), Presence::Present),
            (("acme/spaced", "main", "My File.md"), Presence::Present),
        ]),
        ..ScriptedRest::default()
    };
    let evidence =
        verify_external(&rest, &plan, "gitlab.com", "0.0.0", "t0").expect("evidence is produced");
    assert_eq!(
        external_facts(&evidence).expect("the evidence fixture has complete facts"),
        vec![
            "https://gitlab.com/acme/coupled/-/blob/main/x%2Fy.md readable".to_owned(),
            "https://gitlab.com/acme/doubled/-/blob/main/My%2520File.md readable resolved"
                .to_owned(),
            "https://gitlab.com/acme/slashed/-/blob/release%2Fx/a.md readable resolved".to_owned(),
            "https://gitlab.com/acme/spaced/-/blob/main/My%20File.md readable resolved".to_owned(),
            "https://gitlab.com/acme/veiled/-/blob/release%2Fx/a.md readable".to_owned(),
        ],
    );
}

/// A standing unavailability ends the walk with the rows already learned:
/// partial evidence beats none, and the skipped rest stays unproven.
#[test]
fn a_rate_limit_keeps_the_partial_evidence() {
    let plan = external_plan(&[
        "https://gitlab.com/acme/first",
        "https://gitlab.com/acme/second",
    ])
    .expect("the report fixture yields a plan");
    let rest = ScriptedRest {
        visibility: BTreeMap::from([
            ("acme/first", Visibility::Readable),
            ("acme/second", Visibility::Readable),
        ]),
        unavailable_from: Some(1),
        ..ScriptedRest::default()
    };
    let evidence =
        verify_external(&rest, &plan, "gitlab.com", "0.0.0", "t0").expect("partial evidence");
    assert_eq!(
        external_facts(&evidence).expect("the evidence fixture has complete facts"),
        vec!["https://gitlab.com/acme/first readable".to_owned()],
    );
}

/// The whole chain: scripted facts become evidence the engine judges.
#[test]
fn the_evidence_reaches_verdicts_through_the_engine() {
    let plan = external_plan(&[
        "https://gitlab.com/acme/gone/-/blob/main/missing.md",
        "https://gitlab.com/acme/private",
    ])
    .expect("the report fixture yields a plan");
    let rest = ScriptedRest {
        visibility: BTreeMap::from([
            ("acme/gone", Visibility::Readable),
            ("acme/private", Visibility::Missing),
        ]),
        heads: BTreeMap::from([("acme/gone", vec!["main"])]),
        ..ScriptedRest::default()
    };
    let evidence =
        verify_external(&rest, &plan, "gitlab.com", "0.0.0", "t0").expect("evidence is produced");
    let assessment = assess(&plan, &evidence, "0.0.0", hj("t", &Value::Null))
        .expect("the engine judges the evidence");
    let document =
        amiss_wire::external::parse_assessment(&assessment).expect("the assessment is valid");
    let verdicts: Vec<_> = document
        .payload
        .verdicts
        .iter()
        .map(|row| (row.verdict, row.reason))
        .collect();
    assert_eq!(
        verdicts,
        vec![
            (ExternalVerdict::Refuted, Some(ExternalReason::PathMissing)),
            (
                ExternalVerdict::Unproven,
                Some(ExternalReason::RepositoryUnseen)
            ),
        ]
    );
}
