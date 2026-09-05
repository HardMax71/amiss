use std::collections::BTreeMap;

use amiss_wire::controls::Profile;
use amiss_wire::digest::Digest;
use amiss_wire::json::Value;
use amiss_wire::model::{RepoPath, RepoPathText};
use amiss_wire::report::model::ControlStateSource;
use amiss_wire::report::{FindingKind, FixKind};

use crate::claim::{ClaimMissingReason, ClaimVerdict};
use crate::scan::SpanDisplay;

use super::control::control_fact_finding;
use super::{Finding, FindingFix};

/// One document's defective value claims under one name: the group a claim
/// finding stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimGroup {
    pub kind: FindingKind,
    pub carrier: crate::claim::ClaimCarrier,
    pub document: RepoPath,
    pub name: String,
    pub member_count: u64,
    pub sources: Vec<ControlStateSource>,
    pub representative_span: Option<(usize, usize)>,
    pub representative_display: Option<SpanDisplay>,
    pub target_path: RepoPath,
    pub line: u64,
    pub expected_digest: Digest,
    pub observed: &'static str,
    pub observed_digest: Option<Digest>,
    pub observed_line: Option<Vec<u8>>,
}

/// The sorted distinct source digests with their multiplicities, in the
/// wire's control-source shape.
pub(super) fn sources_value(sources: &[ControlStateSource]) -> Value {
    Value::array(
        sources
            .iter()
            .map(|source| {
                Value::object(vec![
                    (
                        "multiplicity".to_owned(),
                        Value::Integer(i64::try_from(source.multiplicity).unwrap_or(i64::MAX)),
                    ),
                    (
                        "digest".to_owned(),
                        Value::string(source.digest.to_string()),
                    ),
                ])
            })
            .collect(),
    )
}

pub(crate) fn source_multiplicities(
    digests: impl IntoIterator<Item = Digest>,
) -> Vec<ControlStateSource> {
    let mut digests: Vec<Digest> = digests.into_iter().collect();
    digests.sort_unstable();
    digests
        .chunk_by(|left, right| left == right)
        .filter_map(|run| {
            run.first().copied().map(|digest| ControlStateSource {
                digest,
                multiplicity: u64::try_from(run.len()).unwrap_or(u64::MAX),
            })
        })
        .collect()
}

/// Groups defective outcomes by kind, document, and claim name, keeping the
/// least location as the representative. Attested claims group nothing.
#[must_use]
pub fn claim_groups(outcomes: &[crate::claim::ClaimOutcome]) -> Vec<ClaimGroup> {
    struct Keyed<'outcome> {
        outcome: &'outcome crate::claim::ClaimOutcome,
        kind: FindingKind,
        observed: &'static str,
        observed_digest: Option<Digest>,
        observed_line: Option<&'outcome [u8]>,
    }

    let mut keyed: BTreeMap<(&RepoPath, &str, FindingKind), Vec<Keyed<'_>>> = BTreeMap::new();
    for outcome in outcomes {
        let (kind, observed, observed_digest, observed_line) = match &outcome.verdict {
            ClaimVerdict::Attested => continue,
            ClaimVerdict::Broken {
                observed_digest,
                observed,
            } => (
                FindingKind::ClaimBroken,
                "line-differs",
                Some(*observed_digest),
                Some(observed.as_slice()),
            ),
            ClaimVerdict::TargetMissing(reason) => (
                FindingKind::ClaimTargetMissing,
                match reason {
                    ClaimMissingReason::Absent => "target-absent",
                    ClaimMissingReason::NotABlob => "target-not-a-blob",
                    ClaimMissingReason::LfsPointer => "target-lfs-pointer",
                    ClaimMissingReason::LineOutOfRange => "line-out-of-range",
                },
                None,
                None,
            ),
        };
        keyed
            .entry((&outcome.document, outcome.name.as_str(), kind))
            .or_default()
            .push(Keyed {
                outcome,
                kind,
                observed,
                observed_digest,
                observed_line,
            });
    }
    keyed
        .into_values()
        .filter_map(|mut members| {
            members.sort_by_key(|member| member.outcome.span);
            let sources =
                source_multiplicities(members.iter().map(|member| member.outcome.source_digest));
            let member_count = u64::try_from(members.len()).unwrap_or(u64::MAX);
            members.first().map(|representative| ClaimGroup {
                kind: representative.kind,
                carrier: representative.outcome.carrier,
                document: representative.outcome.document.clone(),
                name: representative.outcome.name.clone(),
                member_count,
                sources,
                representative_span: Some(representative.outcome.span),
                representative_display: Some(representative.outcome.display),
                target_path: representative.outcome.path.clone(),
                line: representative.outcome.line,
                expected_digest: representative.outcome.expected_digest,
                observed: representative.observed,
                observed_digest: representative.observed_digest,
                observed_line: representative.observed_line.map(<[u8]>::to_vec),
            })
        })
        .collect()
}

/// One defective claim group as a control-scoped finding carrying the claim
/// evidence family.
pub(super) fn claim_finding(group: &ClaimGroup, profile: Profile) -> Result<Finding, crate::Error> {
    let rule_id = format!("claim/value/{}", group.name);
    let evidence = Value::object(vec![
        ("kind".to_owned(), Value::string("claim".to_owned())),
        ("claim_kind".to_owned(), Value::string("value".to_owned())),
        ("name".to_owned(), Value::string(group.name.clone())),
        ("target_path".to_owned(), group.target_path.to_value()),
        (
            "line".to_owned(),
            Value::Integer(i64::try_from(group.line).unwrap_or(i64::MAX)),
        ),
        (
            "expected_digest".to_owned(),
            Value::string(group.expected_digest.to_string()),
        ),
        (
            "observed".to_owned(),
            Value::string(group.observed.to_owned()),
        ),
        (
            "observed_digest".to_owned(),
            group
                .observed_digest
                .map_or(Value::Null, |value| Value::string(value.to_string())),
        ),
        ("sources".to_owned(), sources_value(&group.sources)),
    ]);
    let mut finding = control_fact_finding(
        group.kind,
        &group.document,
        &rule_id,
        evidence,
        group.member_count,
        (group.representative_span, group.representative_display),
        profile,
    )?;
    finding.fix = claim_fix(group);
    Ok(finding)
}

/// The provable rewrite for a lone broken claim, or None: grouped members
/// share one finding but not one edit, and a target-missing claim has no
/// derivable content.
fn claim_fix(group: &ClaimGroup) -> Option<FindingFix> {
    if group.kind != FindingKind::ClaimBroken || group.member_count != 1 {
        return None;
    }
    let observed = group.observed_line.as_deref()?;
    let replacement = crate::claim::rewrite(
        &group.name,
        &group.target_path,
        group.line,
        observed,
        group.carrier,
    )?;
    let span = group.representative_span?;
    Some(FindingFix {
        path: RepoPathText::new(group.document.as_str()?.to_owned())?,
        span,
        replacement,
        kind: FixKind::ClaimValueRewrite,
    })
}
