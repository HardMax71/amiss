mod components;
mod model;

use std::collections::BTreeMap;

use amiss_wire::model::Digest;
use amiss_wire::model::RepoPath;
use amiss_wire::resolution::{Resolution, TaggedBlobTarget, Target, UnsupportedSemantics};

use components::{ObservationPool, correlation_components};
pub(crate) use components::{directory_pairs, unique_path_pairs};
pub use model::{
    Comparison, Impact, Observation, Outcome, Reason, Side, SourceChange, TargetChange,
};

use crate::Error;

/// Correlates the two sides: exact by equal observation identity, then
/// plausible bipartite edges under the correlation projection, components in
/// identity byte order, and one comparison per component. Every occurrence
/// appears exactly once as a primary or an alternative. Both sides are
/// consumed so their observations move into those rows without cloning.
///
/// # Errors
///
/// A duplicated observation identity within one side is an internal defect.
pub fn correlate(base: Side, candidate: Side) -> Result<Vec<Comparison>, Error> {
    let aligned = base.observations.len() == candidate.observations.len()
        && base
            .observations
            .iter()
            .zip(&candidate.observations)
            .all(|(left, right)| left.id == right.id);
    let mut comparisons = if aligned {
        let mut comparisons = Vec::with_capacity(base.observations.len());
        for (left, right) in base.observations.into_iter().zip(candidate.observations) {
            comparisons.push(exact_comparison(left, right));
        }
        comparisons
    } else {
        correlate_unaligned(base, candidate)?
    };
    comparisons.sort_unstable_by_key(|comparison| {
        (
            comparison
                .candidate
                .as_ref()
                .map(|observation| observation.id),
            comparison.base.as_ref().map(|observation| observation.id),
        )
    });
    if aligned {
        let mut previous = None;
        for comparison in &comparisons {
            let identity = comparison.base.as_ref().ok_or(Error::Internal)?.id;
            if previous == Some(identity) {
                return Err(Error::Internal);
            }
            previous = Some(identity);
        }
    }
    Ok(comparisons)
}

fn correlate_unaligned(base: Side, candidate: Side) -> Result<Vec<Comparison>, Error> {
    let Side {
        observations: base_observations,
        documents: base_documents,
    } = base;
    let Side {
        observations: candidate_observations,
        documents: candidate_documents,
    } = candidate;
    let base = ObservationPool::new(base_observations)?;
    let candidate = ObservationPool::new(candidate_observations)?;
    let renames = unique_path_pairs(&base_documents, &candidate_documents);
    let exact_ids: Vec<Digest> = base
        .positions
        .keys()
        .filter(|id| candidate.positions.contains_key(*id))
        .copied()
        .collect();
    let grouped = correlation_components(&base, &candidate, &exact_ids, &renames)?;

    let exact_count = exact_ids.len();
    let mut base_order = Vec::with_capacity(base.observations.len());
    let mut candidate_order = Vec::with_capacity(candidate.observations.len());
    base_order.extend_from_slice(&exact_ids);
    candidate_order.extend_from_slice(&exact_ids);
    let mut components = Vec::with_capacity(grouped.len());
    for (base_ids, candidate_ids) in grouped.into_values() {
        components.push((base_ids.len(), candidate_ids.len()));
        base_order.extend(base_ids);
        candidate_order.extend(candidate_ids);
    }

    let mut base = base.into_order(&base_order)?.into_iter();
    let mut candidate = candidate.into_order(&candidate_order)?.into_iter();
    let mut comparisons = Vec::with_capacity(exact_count.saturating_add(components.len()));
    for _ in 0..exact_count {
        let (Some(left), Some(right)) = (base.next(), candidate.next()) else {
            return Err(Error::Internal);
        };
        if left.id != right.id {
            return Err(Error::Internal);
        }
        comparisons.push(exact_comparison(left, right));
    }
    for (base_count, candidate_count) in components {
        let base_members: Vec<Observation> = base.by_ref().take(base_count).collect();
        let candidate_members: Vec<Observation> =
            candidate.by_ref().take(candidate_count).collect();
        if base_members.len() != base_count || candidate_members.len() != candidate_count {
            return Err(Error::Internal);
        }
        comparisons.push(match (base_members.len(), candidate_members.len()) {
            (1, 0) => isolated(
                base_members.into_iter().next().ok_or(Error::Internal)?,
                true,
            ),
            (0, 1) => isolated(
                candidate_members
                    .into_iter()
                    .next()
                    .ok_or(Error::Internal)?,
                false,
            ),
            _ => component_comparison(base_members, candidate_members, &renames),
        });
    }
    if base.next().is_some() || candidate.next().is_some() {
        return Err(Error::Internal);
    }
    Ok(comparisons)
}

fn exact_comparison(left: Observation, right: Observation) -> Comparison {
    let (target_change, impact) = derive(&left, &right, SourceChange::Equal);
    Comparison {
        outcome: Outcome::Exact,
        reason: Reason::SameExtractionKeyAndProjection,
        source_change: SourceChange::Equal,
        base: Some(left),
        candidate: Some(right),
        alternatives_base: Vec::new(),
        alternatives_candidate: Vec::new(),
        target_change,
        impact,
    }
}

fn component_comparison(
    mut base_members: Vec<Observation>,
    mut candidate_members: Vec<Observation>,
    renames: &BTreeMap<RepoPath, RepoPath>,
) -> Comparison {
    if let ([left], [right]) = (base_members.as_slice(), candidate_members.as_slice()) {
        let across_rename =
            left.document != right.document && renames.get(&left.document) == Some(&right.document);
        let (reason, source_change) = if across_rename {
            (
                Reason::ExactDocumentRenameUnchangedProjection,
                SourceChange::Equal,
            )
        } else if left.projection_digest == right.projection_digest {
            (Reason::SameIntentUnchangedProjection, SourceChange::Equal)
        } else {
            (Reason::SameIntentSourceChanged, SourceChange::Changed)
        };
        let (target_change, impact) = derive(left, right, source_change);
        return Comparison {
            outcome: Outcome::Candidate,
            reason,
            source_change,
            base: base_members.pop(),
            candidate: candidate_members.pop(),
            alternatives_base: Vec::new(),
            alternatives_candidate: Vec::new(),
            target_change,
            impact,
        };
    }

    let mut base_members = base_members.into_iter();
    let mut candidate_members = candidate_members.into_iter();
    Comparison {
        outcome: Outcome::Ambiguous,
        reason: Reason::MultipleCounterparts,
        source_change: SourceChange::Unknown,
        base: base_members.next(),
        candidate: candidate_members.next(),
        alternatives_base: base_members.collect(),
        alternatives_candidate: candidate_members.collect(),
        target_change: TargetChange::NotComparable,
        impact: Impact::ObservationCorrelationAmbiguous,
    }
}

fn isolated(observation: Observation, is_base: bool) -> Comparison {
    let (base, candidate) = if is_base {
        (Some(observation), None)
    } else {
        (None, Some(observation))
    };
    Comparison {
        outcome: Outcome::None,
        reason: if is_base {
            Reason::RemovedObservation
        } else {
            Reason::NewObservation
        },
        source_change: if is_base {
            SourceChange::Removed
        } else {
            SourceChange::Added
        },
        base,
        candidate,
        alternatives_base: Vec::new(),
        alternatives_candidate: Vec::new(),
        target_change: TargetChange::NotComparable,
        impact: if is_base {
            Impact::RemovedObservation
        } else {
            Impact::NewObservation
        },
    }
}

/// The content projection of the file a reference located, whether its
/// meaning was then evaluated or not. A fragment, code fragment or query the
/// run could not answer still names a file, and that file changing is the
/// same dependency change it would be for a plain link.
fn located_projection(resolution: &Resolution<RepoPath>) -> Option<Digest> {
    match resolution {
        Resolution::Resolved { target }
        | Resolution::UnsupportedSemantics(
            UnsupportedSemantics::Query(target) | UnsupportedSemantics::CodeFragment(target),
        ) => target.projection_digest(),
        Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(
            TaggedBlobTarget::Blob(blob),
        )) => blob.content.projection_digest(),
        Resolution::UnsupportedSemantics(
            UnsupportedSemantics::SiteRoute
            | UnsupportedSemantics::UnmodelledRoute
            | UnsupportedSemantics::NetworkPath
            | UnsupportedSemantics::AttributeDependent
            | UnsupportedSemantics::DuplicateLabel
            | UnsupportedSemantics::ExternalInventory
            | UnsupportedSemantics::RepositoryRoot,
        )
        | Resolution::Missing(_)
        | Resolution::DeclaredUntracked(_)
        | Resolution::TypeMismatch { .. }
        | Resolution::UnsupportedTarget(_)
        | Resolution::UnsupportedVersion { .. }
        | Resolution::Invalid { .. }
        | Resolution::External { .. } => None,
    }
}

/// The base-versus-candidate derivation for exact and candidate pairs, in the
/// closed table's order.
fn derive(
    base: &Observation,
    candidate: &Observation,
    source: SourceChange,
) -> (TargetChange, Impact) {
    let left = &base.resolution;
    let right = &candidate.resolution;
    let source_changed = source == SourceChange::Changed;

    let equal_impact = if source_changed {
        Impact::SubjectChanged
    } else {
        Impact::None
    };

    if let (Some(left_projection), Some(right_projection)) =
        (located_projection(left), located_projection(right))
    {
        if left_projection == right_projection {
            return (TargetChange::Equal, equal_impact);
        }
        let own_page = matches!(
            (left, right),
            (
                Resolution::Resolved { target: Target::Blob(left_blob) },
                Resolution::Resolved { target: Target::Blob(right_blob) },
            ) if left_blob.path == base.document && right_blob.path == candidate.document
        );
        if own_page {
            return (TargetChange::Changed, equal_impact);
        }
        let impact = if source_changed {
            Impact::DependencyAndSubjectCochanged
        } else {
            Impact::DependencyChangedSubjectUnchanged
        };
        return (TargetChange::Changed, impact);
    }
    match (left, right) {
        (Resolution::Resolved { .. }, Resolution::Resolved { .. }) => {
            (TargetChange::NotComparable, Impact::NotApplicable)
        }
        (Resolution::Missing(left_missing), Resolution::Missing(right_missing)) => {
            if left_missing == right_missing {
                (TargetChange::Equal, equal_impact)
            } else {
                (TargetChange::NotComparable, Impact::NotApplicable)
            }
        }
        (
            Resolution::DeclaredUntracked(left_declared),
            Resolution::DeclaredUntracked(right_declared),
        ) => {
            if left_declared == right_declared {
                (TargetChange::Equal, equal_impact)
            } else {
                (TargetChange::NotComparable, Impact::NotApplicable)
            }
        }
        (
            Resolution::TypeMismatch {
                target: left_target,
            },
            Resolution::TypeMismatch {
                target: right_target,
            },
        ) => {
            if left_target == right_target {
                (TargetChange::Equal, equal_impact)
            } else {
                (TargetChange::NotComparable, Impact::NotApplicable)
            }
        }
        (Resolution::Missing(_) | Resolution::TypeMismatch { .. }, Resolution::Resolved { .. }) => {
            (TargetChange::NewlyResolved, Impact::ReferenceResolved)
        }
        (Resolution::Resolved { .. }, Resolution::Missing(_) | Resolution::TypeMismatch { .. }) => {
            (TargetChange::BecameMissing, Impact::NotApplicable)
        }
        (
            Resolution::Resolved { .. }
            | Resolution::Missing(_)
            | Resolution::TypeMismatch { .. }
            | Resolution::DeclaredUntracked(_)
            | Resolution::UnsupportedTarget(_)
            | Resolution::UnsupportedSemantics(_)
            | Resolution::UnsupportedVersion { .. }
            | Resolution::Invalid { .. }
            | Resolution::External { .. },
            Resolution::Resolved { .. }
            | Resolution::Missing(_)
            | Resolution::TypeMismatch { .. }
            | Resolution::DeclaredUntracked(_)
            | Resolution::UnsupportedTarget(_)
            | Resolution::UnsupportedSemantics(_)
            | Resolution::UnsupportedVersion { .. }
            | Resolution::Invalid { .. }
            | Resolution::External { .. },
        ) => (TargetChange::NotComparable, Impact::NotApplicable),
    }
}
