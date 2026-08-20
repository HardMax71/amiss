use std::collections::BTreeMap;

use amiss_wire::controls::Profile;
use amiss_wire::digest::Digest;
use amiss_wire::json::Value;
use amiss_wire::report::{Disposition, ErrorDetail, FindingKind};
use amiss_wire::resolution::Resolution;

use super::claims::{ClaimGroup, claim_finding};
use super::control::{GovernedSeed, control_finding, governed_finding};
use super::debt::debt_pass;
use super::documents::document_findings;
use super::finding::{observation_location, observation_scope, simple};
use super::references::{comparison_findings, structural_findings};
use super::waiver::waiver_pass;
use super::{
    Attribution, DebtApplied, DocumentInput, Finding, FindingFact, LocationSide, PolicyStep,
    WaiverApplied, resolution_kinds,
};
use crate::correlate::{Comparison, Outcome};

/// The attribution an invalid reference carries on its report row: introduced
/// when the base held no equal invalid destination, pre-existing when it did,
/// unknown under an ambiguous pairing or for an alternative occurrence. The
/// feedback projection used to recompute this and discard it.
fn invalid_attributions(comparisons: &[Comparison]) -> BTreeMap<Digest, Attribution> {
    let mut rows = BTreeMap::new();
    for comparison in comparisons {
        if let Some(candidate) = &comparison.candidate
            && matches!(candidate.resolution, Resolution::Invalid(_))
        {
            let attribution = match comparison.outcome {
                Outcome::Ambiguous => Attribution::Unknown,
                Outcome::Exact | Outcome::Candidate | Outcome::None => comparison
                    .base
                    .as_ref()
                    .filter(|base| {
                        matches!(base.resolution, Resolution::Invalid(_))
                            && base.raw_destination_digest == candidate.raw_destination_digest
                    })
                    .map_or(Attribution::Introduced, |_base| Attribution::PreExisting),
            };
            rows.insert(candidate.id, attribution);
        }
        for candidate in &comparison.alternatives_candidate {
            if matches!(candidate.resolution, Resolution::Invalid(_)) {
                rows.insert(candidate.id, Attribution::Unknown);
            }
        }
    }
    rows
}

/// The exact ordinary-finding projection: document findings, occurrence
/// boundaries, structural aggregation by key with attribution, and the
/// comparison-derived removal, ambiguity, and impact findings. Analysis
/// errors never enter, and the result is in canonical finding-key order.
#[must_use]
pub fn evaluate(
    documents: &[DocumentInput],
    comparisons: &[Comparison],
    profile: Profile,
) -> Vec<Finding> {
    let (findings, _no_exceptions) = evaluate_with_policy(
        documents,
        comparisons,
        profile,
        &crate::policy::Effects::default(),
        &[],
        &[],
    );
    findings
}

/// The full projection with the candidate policy applied: the raise-only
/// repository and floor steps on structural candidate facts, exact debt and
/// waiver application with their defect findings, the weakening and coverage
/// control findings, and one unsupported-capability finding per candidate
/// document holding reserved governed definitions. The returned rows are the
/// exception-overlap errors; any row makes the run incomplete.
#[must_use]
pub fn evaluate_with_policy(
    documents: &[DocumentInput],
    comparisons: &[Comparison],
    profile: Profile,
    policy: &crate::policy::Effects,
    governed: &[GovernedSeed],
    claims: &[ClaimGroup],
) -> (Vec<Finding>, Vec<ErrorDetail>) {
    let mut findings = ordinary(documents, comparisons, profile);
    for seed in governed {
        findings.push(governed_finding(seed, profile));
    }
    for group in claims {
        findings.push(claim_finding(group, profile));
    }
    for finding in &mut findings {
        if finding.attribution == Attribution::Resolved || finding.candidate_fact.is_none() {
            continue;
        }
        apply_raise(finding, &policy.raised, "repository-policy", "repository");
        apply_raise(finding, &policy.floor_raised, "organization-floor", "floor");
        finding.configured_disposition = finding
            .steps
            .last()
            .map_or(finding.configured_disposition, |step| step.after);
        finding.effective_disposition = finding.configured_disposition;
    }
    if profile.introduced_only() {
        for finding in &mut findings {
            if finding.effective_disposition == Disposition::Fail
                && finding.attribution == Attribution::PreExisting
            {
                finding.steps.push(PolicyStep {
                    source: "built-in",
                    rule_id: format!(
                        "scanner-policy-defaults/{}/enforce-introduced",
                        finding.kind().as_ref()
                    ),
                    before: Disposition::Fail,
                    after: Disposition::Warn,
                });
                finding.effective_disposition = Disposition::Warn;
            }
        }
    }
    let (exception_findings, errors) = apply_exceptions(&mut findings, policy, profile);
    findings.extend(exception_findings);
    for seed in &policy.controls {
        findings.push(control_finding(seed, policy, profile));
    }
    findings.sort_by_key(|finding| finding.key.digest());
    (findings, errors)
}

pub(super) fn tree_value(tree: &amiss_wire::model::TreeIdentity) -> Value {
    Value::object(vec![
        (
            "object_format".to_owned(),
            Value::string(tree.object_format().as_ref().to_owned()),
        ),
        (
            "tree_oid".to_owned(),
            Value::string(tree.tree_oid().to_owned()),
        ),
    ])
}

/// Candidate findings exception items may target: exact keys with candidate
/// facts, excluding resolved projections and every scope exceptions cannot
/// touch. First insertion preserves the former linear `position` semantics if
/// an invalid directly constructed finding slice repeats a key.
fn exception_targets(findings: &[Finding]) -> BTreeMap<Digest, usize> {
    let mut targets = BTreeMap::new();
    for (index, finding) in findings.iter().enumerate() {
        if finding.candidate_fact.is_some() {
            targets.entry(finding.key.digest()).or_insert(index);
        }
    }
    targets
}

pub(super) fn candidate_digest_of(finding: &Finding) -> Option<Digest> {
    finding.candidate_fact.as_ref().map(FindingFact::digest)
}

/// Steps four and five with their defect findings: exact active debt, one
/// exact selected waiver, the closed defect rows in construction order, and
/// the overlap law that applies neither when both are valid.
fn apply_exceptions(
    findings: &mut [Finding],
    policy: &crate::policy::Effects,
    profile: Profile,
) -> (Vec<Finding>, Vec<ErrorDetail>) {
    let mut extra: Vec<Finding> = Vec::new();
    if policy.debt.is_none() && policy.waiver.is_none() {
        return (extra, Vec::new());
    }
    let Some(instant) = policy
        .time
        .as_ref()
        .map(|time| time.statement.evaluation_instant().clone())
    else {
        return (extra, Vec::new());
    };
    let targets = exception_targets(findings);
    let debt_valid = debt_pass(findings, &targets, policy, profile, &instant, &mut extra);
    let waiver_valid = waiver_pass(findings, &targets, policy, profile, &instant, &mut extra);
    let overlap = apply_valid_exceptions(findings, policy, &debt_valid, &waiver_valid);
    let errors = if overlap {
        vec![ErrorDetail {
            code: amiss_wire::report::AnalysisErrorCode::ExceptionOverlap,
            path: None,
            path_bytes: None,
            resource: None,
        }]
    } else {
        Vec::new()
    };
    (extra, errors)
}

/// The application and overlap law: a finding matched by both valid items
/// applies neither and fails control evaluation; a valid debt step is
/// retained even as a no-op; a waiver step is exactly `fail -> warn`.
fn apply_valid_exceptions(
    findings: &mut [Finding],
    policy: &crate::policy::Effects,
    debt_valid: &BTreeMap<Digest, usize>,
    waiver_valid: &BTreeMap<Digest, usize>,
) -> bool {
    let mut overlap = false;
    for finding in findings.iter_mut() {
        let debt_item = debt_valid.get(&finding.key.digest()).copied();
        let waiver_item = waiver_valid.get(&finding.key.digest()).copied();
        match (debt_item, waiver_item) {
            (Some(_), Some(_)) => {
                overlap = true;
            }
            (Some(index), None) => {
                let (Some(context), Some(item)) = (
                    policy.debt.as_ref(),
                    policy.debt.as_ref().and_then(|debt| debt.items.get(index)),
                ) else {
                    continue;
                };
                let current = finding
                    .steps
                    .last()
                    .map_or(finding.configured_disposition, |step| step.after);
                finding.steps.push(PolicyStep {
                    source: "debt-snapshot",
                    rule_id: format!("debt/{}", item.debt_id.as_str()),
                    before: current,
                    after: Disposition::Warn,
                });
                finding.effective_disposition = Disposition::Warn;
                finding.debt = Some(DebtApplied {
                    item: item.clone(),
                    snapshot_digest: context.digest,
                    adoption_tree: context.adoption_tree.clone(),
                });
            }
            (None, Some(index)) => {
                let (Some(context), Some(item)) = (
                    policy.waiver.as_ref(),
                    policy
                        .waiver
                        .as_ref()
                        .and_then(|waiver| waiver.items.get(index)),
                ) else {
                    continue;
                };
                let current = finding
                    .steps
                    .last()
                    .map_or(finding.configured_disposition, |step| step.after);
                if current == Disposition::Fail {
                    finding.steps.push(PolicyStep {
                        source: "waiver-bundle",
                        rule_id: format!("waiver/{}", item.waiver_id.as_str()),
                        before: Disposition::Fail,
                        after: Disposition::Warn,
                    });
                    finding.effective_disposition = Disposition::Warn;
                    finding.waiver = Some(WaiverApplied {
                        item: item.clone(),
                        bundle_digest: context.digest,
                    });
                }
            }
            (None, None) => {}
        }
    }
    overlap
}

/// Steps two and three: a matching rule applies only when strictly raising,
/// and each step's before equals the preceding after.
fn apply_raise(
    finding: &mut Finding,
    raised: &[(FindingKind, Disposition)],
    source: &'static str,
    prefix: &str,
) {
    let Some((_kind, target)) = raised.iter().find(|(kind, _)| *kind == finding.kind()) else {
        return;
    };
    let current = finding
        .steps
        .last()
        .map_or(finding.configured_disposition, |step| step.after);
    if *target > current {
        finding.steps.push(PolicyStep {
            source,
            rule_id: format!("{prefix}/{}", finding.kind().as_ref()),
            before: current,
            after: *target,
        });
    }
}

fn ordinary(
    documents: &[DocumentInput],
    comparisons: &[Comparison],
    profile: Profile,
) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();

    for document in documents {
        document_findings(document, profile, &mut findings);
    }

    let invalid = invalid_attributions(comparisons);
    for observation in comparisons.iter().flat_map(|comparison| {
        comparison
            .candidate
            .iter()
            .chain(&comparison.alternatives_candidate)
    }) {
        let attribution = if matches!(observation.resolution, Resolution::Invalid(_)) {
            invalid
                .get(&observation.id)
                .copied()
                .unwrap_or(Attribution::Unknown)
        } else {
            Attribution::NotApplicable
        };
        let mut emit = |kind: FindingKind| {
            findings.push(simple(
                kind,
                observation_scope(observation.id),
                attribution,
                vec![observation.id],
                observation_location(observation, LocationSide::Candidate),
                profile,
            ));
        };
        if let Some(kind) = resolution_kinds(&observation.resolution).boundary {
            emit(kind);
        }
        if observation.resolution.is_lfs_pointer() {
            emit(FindingKind::UnsupportedTargetKind);
        }
    }

    structural_findings(comparisons, profile, &mut findings);

    for comparison in comparisons {
        comparison_findings(comparison, profile, &mut findings);
    }

    findings.sort_by_key(|finding| finding.key.digest());
    findings
}
