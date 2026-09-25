use amiss_wire::envelope::document_digest;
use sha2::Digest as _;
use std::collections::BTreeMap;

use amiss_wire::controls::FindingOccurrence;
use amiss_wire::controls::OccurrenceKind;
use amiss_wire::controls::RepositoryTargetIntent;
use amiss_wire::controls::TargetIntentKind;
use amiss_wire::controls::{FactSchema, FindingKeyInputSchema, Profile, TargetKind};
use amiss_wire::model::Digest;
use amiss_wire::model::RepoPath;
use amiss_wire::report::model::{
    EmptyRepositoryPath, FindingFactEvidence, FindingFactInput, PolicySource,
    ReportFindingKeyInput, RepositoryIntentPath,
};
use amiss_wire::report::{Disposition, FindingKind};
use amiss_wire::resolution::{MissingTag, Resolution, ResolutionTag, TargetTag};

use crate::correlate::{Comparison, Impact, Observation, Outcome};
use crate::observe;

use super::finding::{built_in_step, missing_fix, observation_location, reference_fact, simple};
use super::{
    Attribution, FACT_DOMAIN, FINDING_KEY_DOMAIN, Finding, FindingKeyScope, Location, LocationSide,
    PolicyStep, resolution_kinds,
};

/// The adoption-reproduction projection: every structural key among the
/// observations, with its occurrence count and the fact digest computed at
/// that count. Exactly one occurrence with the accepted fact digest is the
/// reproduction requirement.
///
/// # Errors
/// Returns [`crate::Error::Internal`] if a finding key or fact cannot be serialized.
pub fn structural_facts(
    observations: &[Observation],
) -> Result<BTreeMap<Digest, (u64, Digest)>, crate::Error> {
    let mut groups: BTreeMap<Digest, KeyGroup<'_>> = BTreeMap::new();
    for observation in observations {
        collect_structural(&mut groups, observation, false)?;
    }
    groups
        .into_iter()
        .map(|(digest, group)| {
            let first = group.candidate.first().ok_or(crate::Error::Internal)?;
            let multiplicity = u64::try_from(group.candidate.len()).unwrap_or(u64::MAX);
            let input = FindingFactInput {
                evidence: FindingFactEvidence::<RepoPath, _>::Reference {
                    occurrence_multiplicity: multiplicity,
                    resolution: &first.resolution,
                },
                finding_kind: group.key.finding_kind,
                key_input: &group.key,
                schema: FactSchema::Current,
            };
            let fact_digest = document_digest(FACT_DOMAIN, &input).ok_or(crate::Error::Internal)?;
            Ok((digest, (multiplicity, fact_digest)))
        })
        .collect()
}

struct KeyGroup<'a> {
    key: ReportFindingKeyInput<RepoPath>,
    base: Vec<&'a Observation>,
    candidate: Vec<&'a Observation>,
}

fn collect_structural<'a>(
    groups: &mut BTreeMap<Digest, KeyGroup<'a>>,
    observation: &'a Observation,
    is_base: bool,
) -> Result<Option<Digest>, crate::Error> {
    let Some(kind) = resolution_kinds(&observation.resolution).structural else {
        return Ok(None);
    };
    let intent = &observation.intent;
    let key = ReportFindingKeyInput {
        finding_kind: kind,
        schema: FindingKeyInputSchema::Current,
        scope: FindingKeyScope::Reference {
            document: observation.document.clone(),
            normalized_target_intent: RepositoryTargetIntent {
                commit_oid: intent.commit_oid.clone(),
                fragment_digest: intent.fragment.as_deref().map(|text| {
                    Digest::from(
                        sha2::Sha256::new_with_prefix(observe::LINK_FRAGMENT_DOMAIN)
                            .chain_update([0_u8])
                            .chain_update(text.as_bytes())
                            .finalize()
                            .0,
                    )
                }),
                kind: TargetIntentKind::RepositoryPath,
                path: intent.repository_path.clone().map_or(
                    RepositoryIntentPath::Empty(EmptyRepositoryPath::Empty),
                    RepositoryIntentPath::Path,
                ),
                query_digest: intent.query.as_deref().map(|text| {
                    Digest::from(
                        sha2::Sha256::new_with_prefix(observe::LINK_QUERY_DOMAIN)
                            .chain_update([0_u8])
                            .chain_update(text.as_bytes())
                            .finalize()
                            .0,
                    )
                }),
                target_kind: intent.target_kind.unwrap_or(TargetKind::Either),
            },
            occurrence: FindingOccurrence {
                kind: OccurrenceKind::SourceProjection,
                source_projection_digest: observation.projection_digest,
            },
            source_construct: observation.construct,
        },
    };
    let digest = document_digest(FINDING_KEY_DOMAIN, &key).ok_or(crate::Error::Internal)?;
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
    Ok(Some(digest))
}

/// Both sides fail the same way when as many occurrences fail in the same
/// shape. Hints and the target's content are evidence about a failure, so a
/// new suggestion or an edited target leaves a broken reference as broken as
/// it was.
fn same_failure(base: &[&Observation], candidate: &[&Observation]) -> bool {
    base.len() == candidate.len()
        && base
            .first()
            .zip(candidate.first())
            .is_some_and(|(left, right)| {
                failure_shape(&left.resolution) == failure_shape(&right.resolution)
            })
}

fn failure_shape(
    resolution: &Resolution<RepoPath>,
) -> (ResolutionTag, Option<MissingTag>, Option<TargetTag>) {
    (
        ResolutionTag::from(resolution),
        if let Resolution::Missing(missing) = resolution {
            Some(MissingTag::from(missing))
        } else {
            None
        },
        if let Resolution::TypeMismatch { target } = resolution {
            Some(TargetTag::from(target))
        } else {
            None
        },
    )
}

/// The side a group reports from, its members' sorted ids and count, and the
/// location of its first member.
fn reported_members(group: &KeyGroup<'_>) -> (Vec<Digest>, u64, Location) {
    let (members, side) = if group.candidate.is_empty() {
        (&group.base, LocationSide::Base)
    } else {
        (&group.candidate, LocationSide::Candidate)
    };
    let mut ids: Vec<Digest> = members.iter().map(|observation| observation.id).collect();
    ids.sort_unstable();
    let location = members
        .iter()
        .min_by(|left, right| {
            (&left.document, left.span, left.id).cmp(&(&right.document, right.span, right.id))
        })
        .map_or(
            Location {
                side,
                path: None,
                span: None,
                display: None,
            },
            |observation| observation_location(observation, side),
        );
    (
        ids,
        u64::try_from(members.len()).unwrap_or(u64::MAX),
        location,
    )
}

/// Step three: structural kinds aggregate by key across both sides, one
/// finding per key with at least one included side. A base occurrence the
/// correlator paired with a candidate failing the same way joins that
/// candidate's key, so a reworded block or a renamed document keeps its
/// failure pre-existing. Attribution follows fact presence and the failure's
/// shape, and a base-only projection is forced to record so a deletion cannot
/// retain an old blocking failure.
pub(super) fn structural_findings(
    comparisons: &[Comparison],
    profile: Profile,
    findings: &mut Vec<Finding>,
) -> Result<(), crate::Error> {
    let mut groups: BTreeMap<Digest, KeyGroup<'_>> = BTreeMap::new();
    for comparison in comparisons {
        let continued = comparison
            .candidate
            .as_ref()
            .map(|observation| collect_structural(&mut groups, observation, false))
            .transpose()?
            .flatten()
            .filter(|_| matches!(comparison.outcome, Outcome::Exact | Outcome::Candidate));
        for observation in &comparison.alternatives_candidate {
            collect_structural(&mut groups, observation, false)?;
        }
        if let Some(base) = &comparison.base {
            let kind = resolution_kinds(&base.resolution).structural;
            match continued
                .and_then(|digest| groups.get_mut(&digest))
                .filter(|group| Some(group.key.finding_kind) == kind)
            {
                Some(group) => group.base.push(base),
                None => {
                    collect_structural(&mut groups, base, true)?;
                }
            }
        }
        for observation in &comparison.alternatives_base {
            collect_structural(&mut groups, observation, true)?;
        }
    }

    for (digest, group) in groups {
        let kind = group.key.finding_kind;
        let [base_fact, candidate_fact] = [&group.base, &group.candidate].map(|members| {
            members
                .first()
                .map(|observation| {
                    reference_fact(
                        &group.key,
                        observation,
                        u64::try_from(members.len()).unwrap_or(u64::MAX),
                    )
                })
                .transpose()
        });
        let (base_fact, candidate_fact) = (base_fact?, candidate_fact?);
        let attribution = match (&base_fact, &candidate_fact) {
            (None, Some(_)) => Attribution::Introduced,
            (Some(_), None) => Attribution::Resolved,
            (Some(_), Some(_)) if same_failure(&group.base, &group.candidate) => {
                Attribution::PreExisting
            }
            (Some(_), Some(_)) => Attribution::Unknown,
            (None, None) => Attribution::NotApplicable,
        };
        if attribution == Attribution::NotApplicable {
            continue;
        }

        let (ids, member_count, location) = reported_members(&group);

        let configured = if attribution == Attribution::Resolved {
            Disposition::Record
        } else {
            kind.built_in_disposition(profile)
        };
        findings.push(Finding {
            key_input: group.key,
            finding_key: digest,
            attribution,
            base_fact,
            candidate_fact,
            member_count,
            observation_ids: ids,
            location,
            configured_disposition: configured,
            effective_disposition: configured,
            fix: missing_fix(&group.candidate),
            debt: None,
            waiver: None,
            steps: if attribution == Attribution::Resolved {
                vec![PolicyStep {
                    source: PolicySource::ResolvedProjection,
                    rule_id: "resolved-projection".to_owned(),
                    before: Disposition::Record,
                    after: Disposition::Record,
                }]
            } else {
                vec![built_in_step(kind, profile)]
            },
        });
    }
    Ok(())
}

/// Step four: one removal per base-only comparison, one ambiguity per
/// ambiguous comparison, and the three named impact findings only.
pub(super) fn comparison_findings(
    comparison: &Comparison,
    profile: Profile,
    findings: &mut Vec<Finding>,
) -> Result<(), crate::Error> {
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
        return Ok(());
    };
    let kind = if comparison.outcome == Outcome::None && comparison.candidate.is_none() {
        Some(FindingKind::ExplicitReferenceRemoved)
    } else if comparison.outcome == Outcome::Ambiguous {
        Some(FindingKind::ObservationCorrelationAmbiguous)
    } else {
        match comparison.impact {
            Impact::DependencyChangedSubjectUnchanged => {
                Some(FindingKind::DependencyChangedSubjectUnchanged)
            }
            Impact::DependencyAndSubjectCochanged => {
                Some(FindingKind::DependencyAndSubjectCochanged)
            }
            Impact::SubjectChanged => Some(FindingKind::SubjectChanged),
            Impact::None
            | Impact::ReferenceResolved
            | Impact::NotApplicable
            | Impact::ObservationCorrelationAmbiguous
            | Impact::NewObservation
            | Impact::RemovedObservation => None,
        }
    };
    findings.extend(
        kind.map(|kind| {
            simple(
                kind,
                FindingKeyScope::Observation {
                    observation_id: primary.id,
                },
                Attribution::NotApplicable,
                vec![primary.id],
                observation_location(primary, side),
                profile,
            )
        })
        .transpose()?,
    );
    Ok(())
}
