use std::collections::BTreeMap;

use amiss_wire::controls::Profile;
use amiss_wire::digest::Digest;
use amiss_wire::report::{Disposition, FindingKind};

use crate::correlate::{Comparison, Impact, Observation, Outcome};

use super::finding::{
    built_in_step, missing_fix, observation_location, observation_scope, reference_fact,
    reference_scope, simple,
};
use super::{
    Attribution, Finding, FindingKey, Location, LocationSide, PolicyStep, resolution_kinds,
};

/// The adoption-reproduction projection: every structural key among the
/// observations, with its occurrence count and the fact digest computed at
/// that count. Exactly one occurrence with the accepted fact digest is the
/// reproduction requirement.
#[must_use]
pub fn structural_facts(observations: &[Observation]) -> BTreeMap<Digest, (u64, Digest)> {
    let mut groups: BTreeMap<Digest, KeyGroup<'_>> = BTreeMap::new();
    for observation in observations {
        collect_structural(&mut groups, observation, false);
    }
    groups
        .into_iter()
        .filter_map(|(digest, group)| {
            let first = group.candidate.first()?;
            let multiplicity = u64::try_from(group.candidate.len()).unwrap_or(u64::MAX);
            let fact = reference_fact(&group.key, first, multiplicity);
            Some((digest, (multiplicity, fact.digest())))
        })
        .collect()
}

struct KeyGroup<'a> {
    key: FindingKey,
    base: Vec<&'a Observation>,
    candidate: Vec<&'a Observation>,
}

fn collect_structural<'a>(
    groups: &mut BTreeMap<Digest, KeyGroup<'a>>,
    observation: &'a Observation,
    is_base: bool,
) {
    let Some(kind) = resolution_kinds(&observation.resolution).structural else {
        return;
    };
    let key = FindingKey::new(kind, reference_scope(observation));
    let digest = key.digest();
    let group = groups.entry(digest).or_insert_with(|| KeyGroup {
        key,
        base: Vec::new(),
        candidate: Vec::new(),
    });
    if is_base {
        group.base.push(observation);
    } else {
        group.candidate.push(observation);
    }
}

/// Step three: structural kinds aggregate independently by key across both
/// sides, one finding per key with at least one included side. Attribution
/// follows fact presence and equality, and a base-only projection is forced
/// to record so a deletion cannot retain an old blocking failure.
pub(super) fn structural_findings(
    comparisons: &[Comparison],
    profile: Profile,
    findings: &mut Vec<Finding>,
) {
    let mut groups: BTreeMap<Digest, KeyGroup<'_>> = BTreeMap::new();
    for observation in comparisons.iter().flat_map(|comparison| {
        comparison
            .candidate
            .iter()
            .chain(&comparison.alternatives_candidate)
    }) {
        collect_structural(&mut groups, observation, false);
    }
    for observation in comparisons
        .iter()
        .flat_map(|comparison| comparison.base.iter().chain(&comparison.alternatives_base))
    {
        collect_structural(&mut groups, observation, true);
    }

    for (_digest, group) in groups {
        let kind = group.key.kind();
        let base_fact = group.base.first().map(|observation| {
            reference_fact(
                &group.key,
                observation,
                u64::try_from(group.base.len()).unwrap_or(u64::MAX),
            )
        });
        let candidate_fact = group.candidate.first().map(|observation| {
            reference_fact(
                &group.key,
                observation,
                u64::try_from(group.candidate.len()).unwrap_or(u64::MAX),
            )
        });
        let attribution = match (&base_fact, &candidate_fact) {
            (None, Some(_)) => Attribution::Introduced,
            (Some(_), None) => Attribution::Resolved,
            (Some(left), Some(right)) if left == right => Attribution::PreExisting,
            (Some(_), Some(_)) => Attribution::Unknown,
            (None, None) => Attribution::NotApplicable,
        };
        if attribution == Attribution::NotApplicable {
            continue;
        }

        let members = if group.candidate.is_empty() {
            &group.base
        } else {
            &group.candidate
        };
        let mut ids: Vec<Digest> = members.iter().map(|observation| observation.id).collect();
        ids.sort_unstable();
        let member_count = u64::try_from(members.len()).unwrap_or(u64::MAX);
        let representative = members
            .iter()
            .min_by(|left, right| {
                (&left.document, left.span, left.id).cmp(&(&right.document, right.span, right.id))
            })
            .copied();
        let side = if group.candidate.is_empty() {
            LocationSide::Base
        } else {
            LocationSide::Candidate
        };
        let location = representative.map_or(
            Location {
                side,
                path: None,
                span: None,
                display: None,
            },
            |observation| observation_location(observation, side),
        );

        let configured = if attribution == Attribution::Resolved {
            Disposition::Record
        } else {
            kind.built_in_disposition(profile)
        };
        let fix = missing_fix(&group.candidate);
        findings.push(Finding {
            key: group.key,
            attribution,
            base_fact,
            candidate_fact,
            member_count,
            observation_ids: ids,
            location,
            configured_disposition: configured,
            effective_disposition: configured,
            fix,
            debt: None,
            waiver: None,
            steps: if attribution == Attribution::Resolved {
                vec![PolicyStep {
                    source: "resolved-projection",
                    rule_id: "resolved-projection".to_owned(),
                    before: Disposition::Record,
                    after: Disposition::Record,
                }]
            } else {
                vec![built_in_step(kind, profile)]
            },
        });
    }
}

/// Step four: one removal per base-only comparison, one ambiguity per
/// ambiguous comparison, and the three named impact findings only.
pub(super) fn comparison_findings(
    comparison: &Comparison,
    profile: Profile,
    findings: &mut Vec<Finding>,
) {
    if comparison.outcome == Outcome::None
        && comparison.base.is_some()
        && comparison.candidate.is_none()
    {
        if let Some(base) = &comparison.base {
            findings.push(simple(
                FindingKind::ExplicitReferenceRemoved,
                observation_scope(base.id),
                Attribution::NotApplicable,
                vec![base.id],
                observation_location(base, LocationSide::Base),
                profile,
            ));
        }
        return;
    }
    let primary = comparison
        .candidate
        .as_ref()
        .map(|observation| (observation, LocationSide::Candidate))
        .or_else(|| {
            comparison
                .base
                .as_ref()
                .map(|observation| (observation, LocationSide::Base))
        });
    let Some((primary, side)) = primary else {
        return;
    };
    if comparison.outcome == Outcome::Ambiguous {
        findings.push(simple(
            FindingKind::ObservationCorrelationAmbiguous,
            observation_scope(primary.id),
            Attribution::NotApplicable,
            vec![primary.id],
            observation_location(primary, side),
            profile,
        ));
        return;
    }
    let impact_kind = match comparison.impact {
        Impact::DependencyChangedSubjectUnchanged => {
            Some(FindingKind::DependencyChangedSubjectUnchanged)
        }
        Impact::DependencyAndSubjectCochanged => Some(FindingKind::DependencyAndSubjectCochanged),
        Impact::SubjectChanged => Some(FindingKind::SubjectChanged),
        Impact::None
        | Impact::ReferenceResolved
        | Impact::NotApplicable
        | Impact::ObservationCorrelationAmbiguous
        | Impact::NewObservation
        | Impact::RemovedObservation => None,
    };
    if let Some(kind) = impact_kind {
        findings.push(simple(
            kind,
            observation_scope(primary.id),
            Attribution::NotApplicable,
            vec![primary.id],
            observation_location(primary, side),
            profile,
        ));
    }
}
