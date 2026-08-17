use std::collections::{BTreeMap, HashMap};

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
    /// The destination after the format's own decoding, which is what a fetcher
    /// would request, kept only for a reference the engine leaves to another
    /// layer so that layer can read it without the tree.
    pub external_destination: Option<String>,
    pub raw_destination: String,
    pub raw_destination_digest: Digest,
    pub projection_digest: Digest,
    pub resolution: Resolution,
    pub fragment_span: Option<(usize, usize)>,
    pub path_span: Option<(usize, usize)>,
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
        IntentKind::SiteRoute | IntentKind::Label | IntentKind::Unsupported => {
            CorrelationIntent::Other {
                kind: intent.kind,
                raw: observation.raw_destination_digest,
                query,
                fragment,
            }
        }
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
    observations: impl Iterator<Item = Result<&'a Observation, Error>>,
) -> Result<ObservationGroups<'a>, Error> {
    let mut groups: ObservationGroups<'a> = BTreeMap::new();
    for observation in observations {
        let observation = observation?;
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
    Ok(groups)
}

/// Exact Git renames among unmatched document paths: a removed base blob and
/// an added candidate blob pair only when their mode and raw-evidence digest
/// agree and that pair occurs exactly once on each side. Duplicate content
/// creates no edge and is never tie-broken.
fn rename_pairs(
    base: &BTreeMap<RepoPath, (GitMode, Digest)>,
    candidate: &BTreeMap<RepoPath, (GitMode, Digest)>,
) -> BTreeMap<RepoPath, RepoPath> {
    let removed: Vec<(&RepoPath, &(GitMode, Digest))> = base
        .iter()
        .filter(|(path, _)| !candidate.contains_key(*path))
        .collect();
    let added: Vec<(&RepoPath, &(GitMode, Digest))> = candidate
        .iter()
        .filter(|(path, _)| !base.contains_key(*path))
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

struct ObservationPool {
    observations: Vec<Observation>,
    positions: BTreeMap<Digest, usize>,
}

impl ObservationPool {
    fn new(observations: Vec<Observation>) -> Result<Self, Error> {
        let mut positions = BTreeMap::new();
        for (position, observation) in observations.iter().enumerate() {
            if positions.insert(observation.id, position).is_some() {
                return Err(Error::Internal);
            }
        }
        Ok(Self {
            observations,
            positions,
        })
    }

    fn observation(&self, id: Digest) -> Result<&Observation, Error> {
        let Some(position) = self.positions.get(&id) else {
            return Err(Error::Internal);
        };
        let Some(observation) = self.observations.get(*position) else {
            return Err(Error::Internal);
        };
        if observation.id != id {
            return Err(Error::Internal);
        }
        Ok(observation)
    }

    fn into_order(mut self, ids: &[Digest]) -> Result<Vec<Observation>, Error> {
        if ids.len() != self.observations.len() {
            return Err(Error::Internal);
        }
        let mut targets = HashMap::with_capacity(ids.len());
        for (target, id) in ids.iter().enumerate() {
            if targets.insert(*id, target).is_some() {
                return Err(Error::Internal);
            }
        }
        if self
            .observations
            .iter()
            .any(|observation| !targets.contains_key(&observation.id))
        {
            return Err(Error::Internal);
        }
        self.observations.sort_by_cached_key(|observation| {
            targets.get(&observation.id).copied().unwrap_or(usize::MAX)
        });
        Ok(self.observations)
    }
}

type ComponentIds = BTreeMap<usize, (Vec<Digest>, Vec<Digest>)>;

struct ComponentForest {
    identities: Vec<Digest>,
    parents: Vec<usize>,
}

impl ComponentForest {
    fn root_position(&mut self, identity: Digest) -> Result<usize, Error> {
        let mut position = self
            .identities
            .binary_search(&identity)
            .map_err(|_missing| Error::Internal)?;
        loop {
            let parent = *self.parents.get(position).ok_or(Error::Internal)?;
            let grandparent = *self.parents.get(parent).ok_or(Error::Internal)?;
            if parent == grandparent {
                return Ok(parent);
            }
            *self.parents.get_mut(position).ok_or(Error::Internal)? = grandparent;
            position = parent;
        }
    }

    fn union(&mut self, left: Digest, right: Digest) -> Result<(), Error> {
        let left_root = self.root_position(left)?;
        let right_root = self.root_position(right)?;
        if left_root == right_root {
            return Ok(());
        }
        let (root, child) = if left_root < right_root {
            (left_root, right_root)
        } else {
            (right_root, left_root)
        };
        *self.parents.get_mut(child).ok_or(Error::Internal)? = root;
        Ok(())
    }
}

fn correlation_components(
    base: &ObservationPool,
    candidate: &ObservationPool,
    exact_ids: &[Digest],
    renames: &BTreeMap<RepoPath, RepoPath>,
) -> Result<ComponentIds, Error> {
    let unmatched = |id: &&Digest| exact_ids.binary_search(id).is_err();
    let mut identities: Vec<Digest> = base
        .positions
        .keys()
        .chain(candidate.positions.keys())
        .filter(unmatched)
        .copied()
        .collect();
    let identity_count = identities.len();
    identities.sort_unstable();
    identities.dedup();
    if identities.len() != identity_count {
        return Err(Error::Internal);
    }
    let parents = (0..identities.len()).collect();
    let mut components = ComponentForest {
        identities,
        parents,
    };
    let base_groups = observation_groups(
        base.positions
            .keys()
            .filter(unmatched)
            .map(|id| base.observation(*id)),
    )?;
    let candidate_groups = observation_groups(
        candidate
            .positions
            .keys()
            .filter(unmatched)
            .map(|id| candidate.observation(*id)),
    )?;
    connect_candidates(&mut components, &base_groups, &candidate_groups, renames)?;

    let mut grouped: ComponentIds = BTreeMap::new();
    for id in base.positions.keys().filter(unmatched) {
        grouped
            .entry(components.root_position(*id)?)
            .or_default()
            .0
            .push(*id);
    }
    for id in candidate.positions.keys().filter(unmatched) {
        grouped
            .entry(components.root_position(*id)?)
            .or_default()
            .1
            .push(*id);
    }
    Ok(grouped)
}

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
    let renames = rename_pairs(&base_documents, &candidate_documents);
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
    let mut comparisons: Vec<Comparison> =
        Vec::with_capacity(exact_count.saturating_add(components.len()));
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

fn connect_candidates(
    components: &mut ComponentForest,
    base_groups: &ObservationGroups<'_>,
    candidate_groups: &ObservationGroups<'_>,
    renames: &BTreeMap<RepoPath, RepoPath>,
) -> Result<(), Error> {
    for (key, base_documents) in base_groups {
        let Some(candidate_documents) = candidate_groups.get(key) else {
            continue;
        };
        for (base_document, base_group) in base_documents {
            if let Some(candidate_group) = candidate_documents.get(base_document) {
                connect(
                    components,
                    &base_group.observations,
                    &candidate_group.observations,
                )?;
            }
            let Some(candidate_document) = renames.get(*base_document) else {
                continue;
            };
            let Some(candidate_group) = candidate_documents.get(candidate_document) else {
                continue;
            };
            for (projection, base_ids) in &base_group.projections {
                if let Some(candidate_ids) = candidate_group.projections.get(projection) {
                    connect(components, base_ids, candidate_ids)?;
                }
            }
        }
    }
    Ok(())
}

/// Connects one complete bipartite edge set with a linear spanning tree.
/// Correlation consumes connected components rather than individual edges,
/// so this is equivalent to inserting every left-by-right pair.
fn connect(
    components: &mut ComponentForest,
    left: &[&Observation],
    right: &[&Observation],
) -> Result<(), Error> {
    let (Some(left_primary), Some(right_primary)) = (left.first(), right.first()) else {
        return Ok(());
    };
    for observation in left {
        components.union(observation.id, right_primary.id)?;
    }
    for observation in right.iter().skip(1) {
        components.union(left_primary.id, observation.id)?;
    }
    Ok(())
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
        (
            WireResolution::DeclaredUntracked(left_declared),
            WireResolution::DeclaredUntracked(right_declared),
        ) => {
            if left_declared == right_declared {
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
            | WireResolution::DeclaredUntracked(_)
            | WireResolution::UnsupportedTarget(_)
            | WireResolution::UnsupportedSemantics(_)
            | WireResolution::UnsupportedVersion(_)
            | WireResolution::Invalid(_)
            | WireResolution::External(_),
            WireResolution::Resolved(_)
            | WireResolution::Missing(_)
            | WireResolution::TypeMismatch(_)
            | WireResolution::DeclaredUntracked(_)
            | WireResolution::UnsupportedTarget(_)
            | WireResolution::UnsupportedSemantics(_)
            | WireResolution::UnsupportedVersion(_)
            | WireResolution::Invalid(_)
            | WireResolution::External(_),
        ) => (TargetChange::NotComparable, Impact::NotApplicable),
    }
}
