use sha2::Digest as _;
use std::collections::{BTreeMap, HashMap};

use amiss_wire::controls::{GitMode, TargetKind};

use amiss_wire::extraction::SourceConstruct;
use amiss_wire::model::Digest;
use amiss_wire::model::{Adapter, Oid, RepoPath};
use amiss_wire::report::IntentKind;

use super::Observation;
use crate::{Error, observe};

/// The correlation projection. Repository and same-repository forge intents
/// collapse into one class that omits the raw spelling, so an escape-only
/// change still forms a candidate edge; intents without a safe semantic
/// identity retain their raw digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CorrelationIntent<'a> {
    Repository {
        commit_oid: Option<&'a Oid>,
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
    let query = intent.query.as_deref().map(|text| {
        Digest::from(
            sha2::Sha256::new_with_prefix(observe::LINK_QUERY_DOMAIN)
                .chain_update([0_u8])
                .chain_update(text.as_bytes())
                .finalize()
                .0,
        )
    });
    let fragment = intent.fragment.as_deref().map(|text| {
        Digest::from(
            sha2::Sha256::new_with_prefix(observe::LINK_FRAGMENT_DOMAIN)
                .chain_update([0_u8])
                .chain_update(text.as_bytes())
                .finalize()
                .0,
        )
    });
    match intent.kind {
        IntentKind::RepositoryPath
        | IntentKind::SameRepositoryGithub
        | IntentKind::SameRepositoryGitlab
        | IntentKind::SameRepositoryGitea
        | IntentKind::SameRepositoryBitbucketCloud
        | IntentKind::SameRepositoryBitbucketDataCenter => CorrelationIntent::Repository {
            commit_oid: intent.commit_oid.as_ref(),
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

/// Unique equal identities among removed and added paths. Duplicate identity
/// on either side creates no edge and is never tie-broken.
pub(crate) fn unique_path_pairs<I: Ord>(
    base: &BTreeMap<RepoPath, I>,
    candidate: &BTreeMap<RepoPath, I>,
) -> BTreeMap<RepoPath, RepoPath> {
    let removed = base
        .iter()
        .filter(|(path, _)| !candidate.contains_key(*path));
    let added = candidate
        .iter()
        .filter(|(path, _)| !base.contains_key(*path));
    let mut removed_by_identity: BTreeMap<&I, Option<&RepoPath>> = BTreeMap::new();
    for (path, identity) in removed {
        removed_by_identity
            .entry(identity)
            .and_modify(|unique| *unique = None)
            .or_insert(Some(path));
    }
    let mut added_by_identity: BTreeMap<&I, Option<&RepoPath>> = BTreeMap::new();
    for (path, identity) in added {
        added_by_identity
            .entry(identity)
            .and_modify(|unique| *unique = None)
            .or_insert(Some(path));
    }
    let mut pairs = BTreeMap::new();
    for (identity, from) in &removed_by_identity {
        let (Some(from), Some(Some(to))) = (from, added_by_identity.get(identity)) else {
            continue;
        };
        pairs.insert((*from).clone(), (*to).clone());
    }
    pairs
}

type Entries = BTreeMap<RepoPath, (GitMode, Oid)>;

/// The files beneath `directory`, spelled relative to it.
fn beneath<'a>(
    entries: &'a Entries,
    directory: &[u8],
) -> impl Iterator<Item = (&'a [u8], &'a (GitMode, Oid))> {
    let under = [directory, b"/"].concat();
    entries
        .range::<[u8], _>((
            std::ops::Bound::Included(under.as_slice()),
            std::ops::Bound::Unbounded,
        ))
        .map_while(move |(path, identity)| {
            path.as_bytes()
                .strip_prefix(under.as_slice())
                .map(|rest| (rest, identity))
        })
        .filter(|(_, (mode, _))| *mode != GitMode::Tree)
}

/// A removed directory's unique counterpart when the candidate names files
/// only, as an index does: the one added directory holding exactly its files,
/// provided no other removed directory held the same tree.
pub(crate) fn directory_pairs<'a>(
    base: &Entries,
    candidate: &Entries,
    missing: impl Iterator<Item = &'a RepoPath>,
) -> BTreeMap<RepoPath, RepoPath> {
    let directories: BTreeMap<&RepoPath, &Oid> = missing
        .filter_map(|path| {
            base.get(path.as_bytes())
                .filter(|(mode, _)| *mode == GitMode::Tree)
                .map(|(_, tree)| (path, tree))
        })
        .collect();
    if directories.is_empty() {
        return BTreeMap::new();
    }
    let gone = |path: &RepoPath| {
        !candidate.contains_key(path.as_bytes())
            && beneath(candidate, path.as_bytes()).next().is_none()
    };
    let mut trees: BTreeMap<&Oid, Vec<&RepoPath>> = BTreeMap::new();
    let mut holders: BTreeMap<&(GitMode, Oid), Vec<&RepoPath>> = BTreeMap::new();
    for (path, identity) in base {
        if identity.0 == GitMode::Tree {
            trees.entry(&identity.1).or_default().push(path);
        }
    }
    for (path, identity) in candidate {
        holders.entry(identity).or_default().push(path);
    }
    let mut pairs = BTreeMap::new();
    for (directory, tree) in directories {
        let files: Vec<_> = beneath(base, directory.as_bytes()).collect();
        let removed = trees
            .get(tree)
            .map_or(0, |held| held.iter().filter(|path| gone(path)).count());
        let Some((first, identity)) = files.first() else {
            continue;
        };
        let mut added = holders
            .get(identity)
            .into_iter()
            .flatten()
            .filter_map(|path| {
                path.as_bytes()
                    .strip_suffix(*first)
                    .and_then(|stem| stem.strip_suffix(b"/"))
            })
            .filter(|stem| {
                !base.contains_key(*stem)
                    && beneath(base, stem).next().is_none()
                    && beneath(candidate, stem).eq(files.iter().copied())
            });
        if let (1, Some(stem), None) = (removed, added.next(), added.next())
            && let Some(moved) = RepoPath::from_bytes(stem.to_vec())
        {
            pairs.insert(directory.clone(), moved);
        }
    }
    pairs
}

pub(super) struct ObservationPool {
    pub(super) observations: Vec<Observation>,
    pub(super) positions: BTreeMap<Digest, usize>,
}

impl ObservationPool {
    pub(super) fn new(observations: Vec<Observation>) -> Result<Self, Error> {
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

    pub(super) fn into_order(mut self, ids: &[Digest]) -> Result<Vec<Observation>, Error> {
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

pub(super) type ComponentIds = BTreeMap<usize, (Vec<Digest>, Vec<Digest>)>;

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

pub(super) fn correlation_components(
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
    identities.sort_unstable();
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
