use std::collections::BTreeMap;

use amiss_wire::controls::{GitMode, SourceConstruct, TargetKind};
use amiss_wire::digest::Digest;
use amiss_wire::model::{Adapter, RepoPath};
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::Resolution as WireResolution;

use crate::resolve::{Intent, Resolution};
use crate::{Error, observe};

/// One side's occurrence as correlation sees it: its identity, where it
/// lives, what it extracted, and how it resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    pub id: Digest,
    pub document: RepoPath,
    pub span: (usize, usize),
    pub display: crate::scan::SpanDisplay,
    pub block_kind: amiss_md::extract::BlockKind,
    pub node_path: Vec<usize>,
    pub adapter: Adapter,
    pub construct: SourceConstruct,
    pub intent: Intent,
    pub raw_destination_digest: Digest,
    pub projection_digest: Digest,
    pub resolution: Resolution,
}

/// One snapshot side: its observations and, for the rename rule, every
/// classified document's mode and raw-evidence digest.
#[derive(Clone, Debug, Default)]
pub struct Side {
    pub observations: Vec<Observation>,
    pub documents: BTreeMap<RepoPath, (GitMode, Digest)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Exact,
    Candidate,
    Ambiguous,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    SameExtractionKeyAndProjection,
    SameIntentUnchangedProjection,
    SameIntentSourceChanged,
    ExactDocumentRenameUnchangedProjection,
    MultipleCounterparts,
    NewObservation,
    RemovedObservation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceChange {
    Equal,
    Changed,
    Unknown,
    Added,
    Removed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetChange {
    Equal,
    Changed,
    NewlyResolved,
    BecameMissing,
    NotComparable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Impact {
    None,
    SubjectChanged,
    DependencyChangedSubjectUnchanged,
    DependencyAndSubjectCochanged,
    ReferenceResolved,
    NotApplicable,
    ObservationCorrelationAmbiguous,
    NewObservation,
    RemovedObservation,
}

impl Reason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SameExtractionKeyAndProjection => "same-extraction-key-and-projection",
            Self::SameIntentUnchangedProjection => "same-intent-unchanged-projection",
            Self::SameIntentSourceChanged => "same-intent-source-changed",
            Self::ExactDocumentRenameUnchangedProjection => {
                "exact-document-rename-unchanged-projection"
            }
            Self::MultipleCounterparts => "multiple-counterparts",
            Self::NewObservation => "new-observation",
            Self::RemovedObservation => "removed-observation",
        }
    }
}

/// One comparison row: a primary on each present side, alternatives only for
/// ambiguity, and the target derivation for exact and candidate pairs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    pub outcome: Outcome,
    pub reason: Reason,
    pub source_change: SourceChange,
    pub base: Option<Observation>,
    pub candidate: Option<Observation>,
    pub alternatives_base: Vec<Observation>,
    pub alternatives_candidate: Vec<Observation>,
    pub target_change: TargetChange,
    pub impact: Impact,
}

/// The `CorrelationIntent` projection. Repository and same-repository forge
/// intents collapse into one class that omits the raw spelling, so an
/// escape-only change still forms a candidate edge; external, site-route, and
/// unsupported intents keep their raw digest because no safer semantic
/// identity exists for them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CorrelationIntent<'a> {
    Repository {
        path: Option<&'a RepoPath>,
        target_kind: TargetKind,
        query: Option<Digest>,
        fragment: Option<Digest>,
    },
    External {
        raw: Digest,
        scheme: &'a str,
        query: Option<Digest>,
        fragment: Option<Digest>,
    },
    Other {
        kind: IntentKind,
        raw: Digest,
        query: Option<Digest>,
        fragment: Option<Digest>,
    },
}

fn correlation_intent(observation: &Observation) -> CorrelationIntent<'_> {
    let intent = &observation.intent;
    let query = observe::query_digest(intent);
    let fragment = observe::fragment_digest(intent);
    match intent.kind {
        IntentKind::RepositoryPath
        | IntentKind::SameRepositoryGithub
        | IntentKind::SameRepositoryGitlab
        | IntentKind::SameRepositoryGitea => CorrelationIntent::Repository {
            path: intent.repository_path.as_ref(),
            target_kind: intent.target_kind.unwrap_or(TargetKind::Either),
            query,
            fragment,
        },
        IntentKind::ExternalUrl => CorrelationIntent::External {
            raw: observation.raw_destination_digest,
            scheme: intent.external_scheme.as_deref().unwrap_or_default(),
            query,
            fragment,
        },
        IntentKind::SiteRoute | IntentKind::Unsupported => CorrelationIntent::Other {
            kind: intent.kind,
            raw: observation.raw_destination_digest,
            query,
            fragment,
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CorrelationKey<'a> {
    adapter: Adapter,
    construct: SourceConstruct,
    intent: CorrelationIntent<'a>,
}

impl<'a> From<&'a Observation> for CorrelationKey<'a> {
    fn from(observation: &'a Observation) -> Self {
        Self {
            adapter: observation.adapter,
            construct: observation.construct,
            intent: correlation_intent(observation),
        }
    }
}

#[derive(Default)]
struct DocumentGroup<'a> {
    observations: Vec<&'a Observation>,
    projections: BTreeMap<Digest, Vec<&'a Observation>>,
}

type ObservationGroups<'a> =
    BTreeMap<CorrelationKey<'a>, BTreeMap<&'a RepoPath, DocumentGroup<'a>>>;

fn observation_groups<'a>(
    observations: impl Iterator<Item = &'a Observation>,
) -> ObservationGroups<'a> {
    let mut groups: ObservationGroups<'a> = BTreeMap::new();
    for observation in observations {
        let document = groups
            .entry(CorrelationKey::from(observation))
            .or_default()
            .entry(&observation.document)
            .or_default();
        document.observations.push(observation);
        document
            .projections
            .entry(observation.projection_digest)
            .or_default()
            .push(observation);
    }
    groups
}

/// Exact Git renames among unmatched document paths: a removed base blob and
/// an added candidate blob pair only when their mode and raw-evidence digest
/// agree and that pair occurs exactly once on each side. Duplicate content
/// creates no edge and is never tie-broken.
fn rename_pairs(base: &Side, candidate: &Side) -> BTreeMap<RepoPath, RepoPath> {
    let removed: Vec<(&RepoPath, &(GitMode, Digest))> = base
        .documents
        .iter()
        .filter(|(path, _)| !candidate.documents.contains_key(*path))
        .collect();
    let added: Vec<(&RepoPath, &(GitMode, Digest))> = candidate
        .documents
        .iter()
        .filter(|(path, _)| !base.documents.contains_key(*path))
        .collect();
    let mut removed_by_identity: BTreeMap<(GitMode, Digest), Vec<&RepoPath>> = BTreeMap::new();
    for (path, identity) in removed {
        removed_by_identity.entry(*identity).or_default().push(path);
    }
    let mut added_by_identity: BTreeMap<(GitMode, Digest), Vec<&RepoPath>> = BTreeMap::new();
    for (path, identity) in added {
        added_by_identity.entry(*identity).or_default().push(path);
    }
    let mut pairs = BTreeMap::new();
    for (identity, removed_paths) in &removed_by_identity {
        let Some(added_paths) = added_by_identity.get(identity) else {
            continue;
        };
        if let ([from], [to]) = (removed_paths.as_slice(), added_paths.as_slice()) {
            pairs.insert((*from).clone(), (*to).clone());
        }
    }
    pairs
}

/// Correlates the two sides: exact by equal observation identity, then
/// plausible bipartite edges under the correlation projection, components in
/// identity byte order, and one comparison per component. Every occurrence
/// appears exactly once as a primary or an alternative.
///
/// # Errors
///
/// A duplicated observation identity within one side is an internal defect.
pub fn correlate(base: &Side, candidate: &Side) -> Result<Vec<Comparison>, Error> {
    let mut base_by_id: BTreeMap<Digest, &Observation> = BTreeMap::new();
    for observation in &base.observations {
        if base_by_id.insert(observation.id, observation).is_some() {
            return Err(Error::Internal);
        }
    }
    let mut candidate_by_id: BTreeMap<Digest, &Observation> = BTreeMap::new();
    for observation in &candidate.observations {
        if candidate_by_id
            .insert(observation.id, observation)
            .is_some()
        {
            return Err(Error::Internal);
        }
    }

    let mut comparisons: Vec<Comparison> = Vec::new();
    let exact_ids: Vec<Digest> = base_by_id
        .keys()
        .filter(|id| candidate_by_id.contains_key(*id))
        .copied()
        .collect();
    for id in &exact_ids {
        let (Some(left), Some(right)) = (base_by_id.remove(id), candidate_by_id.remove(id)) else {
            return Err(Error::Internal);
        };
        let (target_change, impact) = derive(left, right, SourceChange::Equal);
        comparisons.push(Comparison {
            outcome: Outcome::Exact,
            reason: Reason::SameExtractionKeyAndProjection,
            source_change: SourceChange::Equal,
            base: Some(left.clone()),
            candidate: Some(right.clone()),
            alternatives_base: Vec::new(),
            alternatives_candidate: Vec::new(),
            target_change,
            impact,
        });
    }

    let renames = rename_pairs(base, candidate);
    let mut parent: BTreeMap<Digest, Digest> = BTreeMap::new();
    for id in base_by_id.keys().chain(candidate_by_id.keys()) {
        parent.insert(*id, *id);
    }

    let base_groups = observation_groups(base_by_id.values().copied());
    let candidate_groups = observation_groups(candidate_by_id.values().copied());
    connect_candidates(&mut parent, &base_groups, &candidate_groups, &renames);

    let mut grouped: BTreeMap<Digest, (Vec<&Observation>, Vec<&Observation>)> = BTreeMap::new();
    for (id, observation) in &base_by_id {
        grouped
            .entry(root(&parent, *id))
            .or_default()
            .0
            .push(observation);
    }
    for (id, observation) in &candidate_by_id {
        grouped
            .entry(root(&parent, *id))
            .or_default()
            .1
            .push(observation);
    }
    for (base_members, candidate_members) in grouped.into_values() {
        comparisons.push(
            match (base_members.as_slice(), candidate_members.as_slice()) {
                ([lone], []) => isolated(lone, true),
                ([], [lone]) => isolated(lone, false),
                _ => component_comparison(base_members, candidate_members, &renames),
            },
        );
    }

    comparisons.sort_by(|left, right| {
        let key = |comparison: &Comparison| {
            (
                comparison
                    .candidate
                    .as_ref()
                    .map(|observation| observation.id),
                comparison.base.as_ref().map(|observation| observation.id),
            )
        };
        key(left).cmp(&key(right))
    });
    Ok(comparisons)
}

fn connect_candidates(
    parent: &mut BTreeMap<Digest, Digest>,
    base_groups: &ObservationGroups<'_>,
    candidate_groups: &ObservationGroups<'_>,
    renames: &BTreeMap<RepoPath, RepoPath>,
) {
    for (key, base_documents) in base_groups {
        let Some(candidate_documents) = candidate_groups.get(key) else {
            continue;
        };
        for (base_document, base_group) in base_documents {
            if let Some(candidate_group) = candidate_documents.get(base_document) {
                connect(
                    parent,
                    &base_group.observations,
                    &candidate_group.observations,
                );
            }
            let Some(candidate_document) = renames.get(*base_document) else {
                continue;
            };
            let Some(candidate_group) = candidate_documents.get(candidate_document) else {
                continue;
            };
            for (projection, base_ids) in &base_group.projections {
                if let Some(candidate_ids) = candidate_group.projections.get(projection) {
                    connect(parent, base_ids, candidate_ids);
                }
            }
        }
    }
}

fn root(parent: &BTreeMap<Digest, Digest>, id: Digest) -> Digest {
    let mut at = id;
    while let Some(next) = parent.get(&at) {
        if *next == at {
            return at;
        }
        at = *next;
    }
    at
}

fn union(parent: &mut BTreeMap<Digest, Digest>, left: Digest, right: Digest) {
    let left_root = root(parent, left);
    let right_root = root(parent, right);
    if left_root == right_root {
        return;
    }
    let (low, high) = if left_root < right_root {
        (left_root, right_root)
    } else {
        (right_root, left_root)
    };
    parent.insert(high, low);
}

/// Connects one complete bipartite edge set with a linear spanning tree.
/// Correlation consumes connected components rather than individual edges,
/// so this is equivalent to inserting every left-by-right pair.
fn connect(parent: &mut BTreeMap<Digest, Digest>, left: &[&Observation], right: &[&Observation]) {
    let (Some(left_primary), Some(right_primary)) = (left.first(), right.first()) else {
        return;
    };
    for observation in left {
        union(parent, observation.id, right_primary.id);
    }
    for observation in right.iter().skip(1) {
        union(parent, left_primary.id, observation.id);
    }
}

fn component_comparison(
    mut base_members: Vec<&Observation>,
    mut candidate_members: Vec<&Observation>,
    renames: &BTreeMap<RepoPath, RepoPath>,
) -> Comparison {
    base_members.sort_by_key(|observation| observation.id);
    base_members.dedup_by_key(|observation| observation.id);
    candidate_members.sort_by_key(|observation| observation.id);
    candidate_members.dedup_by_key(|observation| observation.id);

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
            base: Some((*left).clone()),
            candidate: Some((*right).clone()),
            alternatives_base: Vec::new(),
            alternatives_candidate: Vec::new(),
            target_change,
            impact,
        };
    }

    let primary_base = base_members.first().copied();
    let primary_candidate = candidate_members.first().copied();
    Comparison {
        outcome: Outcome::Ambiguous,
        reason: Reason::MultipleCounterparts,
        source_change: SourceChange::Unknown,
        base: primary_base.cloned(),
        candidate: primary_candidate.cloned(),
        alternatives_base: base_members.iter().skip(1).map(|o| (*o).clone()).collect(),
        alternatives_candidate: candidate_members
            .iter()
            .skip(1)
            .map(|o| (*o).clone())
            .collect(),
        target_change: TargetChange::NotComparable,
        impact: Impact::ObservationCorrelationAmbiguous,
    }
}

fn isolated(observation: &Observation, is_base: bool) -> Comparison {
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
        base: is_base.then(|| observation.clone()),
        candidate: (!is_base).then(|| observation.clone()),
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

    match (left, right) {
        (WireResolution::Resolved(left_target), WireResolution::Resolved(right_target)) => {
            let (Some(left_projection), Some(right_projection)) = (
                left_target.projection_digest(),
                right_target.projection_digest(),
            ) else {
                return (TargetChange::NotComparable, Impact::NotApplicable);
            };
            if left_projection == right_projection {
                return (TargetChange::Equal, equal_impact);
            }
            let impact = if source_changed {
                Impact::DependencyAndSubjectCochanged
            } else {
                Impact::DependencyChangedSubjectUnchanged
            };
            (TargetChange::Changed, impact)
        }
        (WireResolution::Missing(left_missing), WireResolution::Missing(right_missing)) => {
            if left_missing == right_missing {
                (TargetChange::Equal, equal_impact)
            } else {
                (TargetChange::NotComparable, Impact::NotApplicable)
            }
        }
        (WireResolution::TypeMismatch(left_target), WireResolution::TypeMismatch(right_target)) => {
            if left_target == right_target {
                (TargetChange::Equal, equal_impact)
            } else {
                (TargetChange::NotComparable, Impact::NotApplicable)
            }
        }
        (
            WireResolution::Missing(_) | WireResolution::TypeMismatch(_),
            WireResolution::Resolved(_),
        ) => (TargetChange::NewlyResolved, Impact::ReferenceResolved),
        (
            WireResolution::Resolved(_),
            WireResolution::Missing(_) | WireResolution::TypeMismatch(_),
        ) => (TargetChange::BecameMissing, Impact::NotApplicable),
        (
            WireResolution::Resolved(_)
            | WireResolution::Missing(_)
            | WireResolution::TypeMismatch(_)
            | WireResolution::UnsupportedTarget(_)
            | WireResolution::UnsupportedSemantics(_)
            | WireResolution::UnsupportedVersion(_)
            | WireResolution::Invalid(_)
            | WireResolution::External(_),
            WireResolution::Resolved(_)
            | WireResolution::Missing(_)
            | WireResolution::TypeMismatch(_)
            | WireResolution::UnsupportedTarget(_)
            | WireResolution::UnsupportedSemantics(_)
            | WireResolution::UnsupportedVersion(_)
            | WireResolution::Invalid(_)
            | WireResolution::External(_),
        ) => (TargetChange::NotComparable, Impact::NotApplicable),
    }
}
