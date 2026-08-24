use amiss_git::{Object, ObjectKind, parse_commit, parse_tree};
use amiss_wire::controls::{GitMode, TargetKind};
use amiss_wire::model::{ForgeDialect, Oid, RepoPath};
use amiss_wire::resolution::Missing;

use crate::discovery::empty_discovery;
use crate::{Error, GitDefect};

use super::{Resolution, Resolver};

pub(super) fn lookup(
    resolver: &mut Resolver<'_>,
    commit_oid: &Oid,
    path: &RepoPath,
    target_kind: TargetKind,
    query: Option<&str>,
    fragment: Option<&str>,
    forge: ForgeDialect,
) -> Result<Option<Resolution>, Error> {
    let Some(mut tree_oid) = commit_tree(resolver, commit_oid)? else {
        return Ok(None);
    };
    let mut components = path.as_bytes().split(|byte| *byte == b'/').peekable();
    let mut located = None;
    while let Some(component) = components.next() {
        let Some(object) = available_object(resolver, &tree_oid, ObjectKind::Tree)? else {
            return Ok(None);
        };
        let Ok(entries) = parse_tree(resolver.repo.object_format(), &object.body) else {
            return Ok(None);
        };
        resolver.scan.charge_historical_tree_entries(
            u64::try_from(entries.len()).unwrap_or(u64::MAX),
            resolver.git.limits().tree_entries_per_snapshot,
        )?;
        let Some(entry) = entries.into_iter().find(|entry| entry.name == component) else {
            return Ok(Some(Resolution::Missing(Missing::PathNotFound {
                path: path.clone(),
                near: None,
                same_object_at: None,
            })));
        };
        if components.peek().is_none() {
            located = Some((entry.mode, entry.oid));
            break;
        }
        if entry.mode != GitMode::Tree {
            return Ok(Some(Resolution::Missing(Missing::PathNotFound {
                path: path.clone(),
                near: None,
                same_object_at: None,
            })));
        }
        tree_oid = entry.oid;
    }

    let Some(entry) = located else {
        return Ok(None);
    };
    let mut snapshot = empty_discovery();
    snapshot.entries.insert(path.clone(), entry);
    let mut historical = Resolver {
        repo: resolver.repo,
        git: resolver.git,
        scan: resolver.scan,
        cache: resolver.cache,
        snapshot: &snapshot,
        commit_oid: Some(commit_oid.clone()),
    };
    match super::lookup(
        &mut historical,
        path,
        target_kind,
        query,
        fragment,
        Some(forge),
    ) {
        Ok(resolution) => Ok(Some(resolution)),
        Err(Error::Git(
            GitDefect::ObjectMissing | GitDefect::ObjectWrongKind | GitDefect::ObjectUnreadable,
        )) => Ok(None),
        Err(defect) => Err(defect),
    }
}

fn commit_tree(resolver: &mut Resolver<'_>, oid: &Oid) -> Result<Option<Oid>, Error> {
    if let Some(cached) = resolver.cache.historical_commits.get(oid) {
        return Ok(cached.clone());
    }
    let tree = match available_object(resolver, oid, ObjectKind::Commit)? {
        Some(object) => parse_commit(resolver.repo.object_format(), &object.body)
            .ok()
            .map(|commit| commit.tree),
        None => None,
    };
    resolver
        .cache
        .historical_commits
        .insert(oid.clone(), tree.clone());
    Ok(tree)
}

fn available_object(
    resolver: &mut Resolver<'_>,
    oid: &Oid,
    kind: ObjectKind,
) -> Result<Option<Object>, Error> {
    match resolver.repo.read_expected(resolver.git, oid, kind) {
        Ok(object) => Ok(Some(object)),
        Err(
            amiss_git::Error::ObjectMissing
            | amiss_git::Error::ObjectWrongKind
            | amiss_git::Error::ObjectUnreadable,
        ) => Ok(None),
        Err(defect) => Err(defect.into()),
    }
}
