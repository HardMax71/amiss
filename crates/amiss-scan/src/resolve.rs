use std::collections::BTreeMap;
use std::sync::Arc;

use amiss_git::{GitResources, ObjectKind, Repository, ValueCap};
use amiss_wire::controls::{GitMode, ResourceName, TargetKind};
use amiss_wire::extraction::SourceConstruct;
use amiss_wire::model::{
    Adapter, BranchRef, ForgeDialect, ObjectFormat, Oid, RepoPath, RepositoryIdentity,
};
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::{
    BlobMode, BlobTarget, DeclaredUntracked, ExternalReference, InvalidReference, Missing,
    Resolution as WireResolution, Target, UnsupportedSemantics, UnsupportedTarget, VersionScope,
};
use amiss_wire::uri::{absolute_valid, decode_fragment, scheme};

use crate::Error;
use crate::declared::Declarations;
use crate::discovery::{Located, SnapshotDiscovery};
use crate::document::{Classification, classify};
use crate::published::anchors;
use crate::published::redirected;
use crate::published::unplaced;
use crate::resources::{Aggregate, ScanResources};
use crate::route::{directory, generator_alias, template_expression};

mod anchor;
mod content;
mod forge;
mod history;
mod line;
mod site;
pub(crate) mod syntax;
mod transclusion;

pub(crate) use line::{named_region_bytes, selected_line_bytes};

use crate::published::routed;
use crate::route::normalized_path_under;
use anchor::{fragment_resolution, linked_label};
use content::{CachedContent, read_target};
use syntax::{same_repo_suffix, split_components, unreadable, unsupported_intent};

pub const TARGET_PROJECTION_DOMAIN: &str = "amiss/scanner-target-projection";
pub const TARGET_LINE_PROJECTION_DOMAIN: &str = "amiss/scanner-target-line-projection";

/// A target's heading identities, built once and then answered from memory.
/// `Unevaluable` records that the parse was refused or unaffordable, which is
/// not the same as a document that publishes nothing.
#[derive(Debug)]
enum Anchors {
    Unread,
    Unevaluable,
    Published(anchor::AnchorIndex),
    Partial(anchor::AnchorIndex),
}

/// An inclusive, one-indexed selection of raw source lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct LineRange {
    pub(crate) first: u64,
    pub(crate) last: u64,
}

/// The occurrence's target intent, fixed after component splitting and before
/// any repository lookup. This, not the eventual resolution, fixes identity
/// and summary membership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Intent {
    pub kind: IntentKind,
    pub commit_oid: Option<Oid>,
    pub repository_path: Option<RepoPath>,
    pub target_kind: Option<TargetKind>,
    pub external_scheme: Option<String>,
    pub query: Option<String>,
    pub fragment: Option<String>,
}

/// One occurrence's typed resolution against a binary-safe repository path.
pub type Resolution = WireResolution<RepoPath>;

/// The trusted run context for same-repository recognition: the declared
/// host, dialect and object format, lowercase owner and repository, the two
/// exact full branch refs.
/// Without it every absolute forge URL remains an external URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeContext {
    pub dialect: ForgeDialect,
    pub object_format: ObjectFormat,
    pub repository: RepositoryIdentity,
    pub candidate_ref: Option<BranchRef>,
    pub default_ref: Option<BranchRef>,
}

/// Referenced targets are read once per path and Git object within one scan
/// resource scope. Reusing a cache with another scope clears its evidence.
#[derive(Debug, Default)]
pub struct TargetCache {
    scope: Option<Arc<()>>,
    read: BTreeMap<RepoPath, CachedContent>,
    historical_read: BTreeMap<Oid, BTreeMap<RepoPath, CachedContent>>,
    declarations: BTreeMap<RepoPath, Declarations>,
    historical_commits: BTreeMap<Oid, Option<Oid>>,
}

impl TargetCache {
    fn bind(&mut self, scope: &Arc<()>) {
        if self
            .scope
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, scope))
        {
            return;
        }
        self.read.clear();
        self.historical_read.clear();
        self.declarations.clear();
        self.historical_commits.clear();
        self.scope = Some(Arc::clone(scope));
    }
}

/// One snapshot-bound resolution session and its shared target evidence.
pub struct Resolver<'a> {
    repo: &'a Repository,
    git: &'a mut GitResources,
    pub(crate) scan: &'a mut ScanResources,
    cache: &'a mut TargetCache,
    snapshot: &'a SnapshotDiscovery,
    commit_oid: Option<Oid>,
}

impl<'a> Resolver<'a> {
    pub fn new(
        repo: &'a Repository,
        git: &'a mut GitResources,
        scan: &'a mut ScanResources,
        cache: &'a mut TargetCache,
        snapshot: &'a SnapshotDiscovery,
    ) -> Self {
        cache.bind(&scan.cache_scope);
        Self {
            repo,
            git,
            scan,
            cache,
            snapshot,
            commit_oid: None,
        }
    }

    /// Resolves one semantic destination against the bound snapshot.
    ///
    /// # Errors
    ///
    /// A target read defect or a snapshot budget crossing.
    pub fn resolve(
        &mut self,
        context: Option<&ForgeContext>,
        adapter: Adapter,
        document_path: &RepoPath,
        is_image: bool,
        semantic: &str,
    ) -> Result<(Intent, Resolution), Error> {
        resolve_destination(
            self,
            context,
            adapter,
            document_path,
            None,
            is_image,
            semantic,
        )
    }

    pub(crate) fn resolve_scanned(
        &mut self,
        context: Option<&ForgeContext>,
        semantic: crate::semantic::View<'_>,
        adapter: Adapter,
        document_path: &RepoPath,
        occurrence: &crate::scanned::ScannedOccurrence,
    ) -> Result<(Intent, Resolution, Option<String>), Error> {
        if occurrence.occurrence.construct == SourceConstruct::RstRefRole {
            return self.resolve_label(&occurrence.occurrence.semantic_destination, semantic);
        }
        let is_image = occurrence.occurrence.construct.is_image();
        let (intent, mut resolution) = resolve_destination(
            self,
            context,
            adapter,
            document_path,
            Some(occurrence.occurrence.construct),
            is_image,
            &occurrence.occurrence.semantic_destination,
        )?;
        if intent.kind == IntentKind::SiteRoute
            && matches!(
                resolution,
                Resolution::UnsupportedSemantics(UnsupportedSemantics::SiteRoute)
                    | Resolution::Invalid {
                        reason: InvalidReference::FragmentEncoding
                    }
            )
            && let Some(evidence) = site::resolve(
                self,
                semantic,
                &occurrence.occurrence.semantic_destination,
                is_image,
            )?
        {
            resolution = evidence;
        }
        let destination = matches!(
            resolution,
            Resolution::External {
                reason: ExternalReference::Url | ExternalReference::ForeignRepository
            } | Resolution::UnsupportedVersion {
                scope: VersionScope::KnownCommit { .. }
            }
        )
        .then(|| occurrence.occurrence.semantic_destination.clone());
        Ok((intent, resolution, destination))
    }
}

/// Absolute URIs under `uri-reference`: ASCII generic syntax, no
/// normalization, two-hex-digit escapes, and for HTTP(S) a `//` plus nonempty
/// authority. Only the emitted scheme is lowercased. The exact `https://`
/// spelling of the declared host opens same-repository recognition; without
/// a declared forge context every syntactically valid absolute URI is
/// external.
fn absolute(
    resolver: &mut Resolver<'_>,
    context: Option<&ForgeContext>,
    path_part: &str,
    scheme: &str,
    query: Option<String>,
    fragment: Option<String>,
) -> Result<(Intent, Resolution), Error> {
    let invalid = |query: Option<String>, fragment: Option<String>| {
        (
            unsupported_intent(query, fragment),
            Resolution::Invalid {
                reason: InvalidReference::Uri,
            },
        )
    };
    if !absolute_valid(path_part, scheme, query.as_deref()) {
        return Ok(invalid(query, fragment));
    }
    if let Some(identity) = context
        && let Some(suffix) = same_repo_suffix(path_part, identity.repository.host())
    {
        return forge::resolve(resolver, identity, suffix, query, fragment);
    }
    Ok((
        Intent {
            kind: IntentKind::ExternalUrl,
            commit_oid: None,
            repository_path: None,
            target_kind: None,
            external_scheme: Some(scheme.to_ascii_lowercase()),
            query,
            fragment,
        },
        Resolution::External {
            reason: ExternalReference::Url,
        },
    ))
}

/// One semantic destination against the bound snapshot, with the construct
/// that wrote it when the caller knows one. A generator declared in the tree
/// anchors the destination somewhere other than beside the document, and is
/// asked before the scheme and site-route readings, since an Antora module
/// coordinate spells like a scheme and a Sphinx docname like a route.
fn resolve_destination(
    resolver: &mut Resolver<'_>,
    context: Option<&ForgeContext>,
    adapter: Adapter,
    document_path: &RepoPath,
    construct: Option<SourceConstruct>,
    is_image: bool,
    semantic: &str,
) -> Result<(Intent, Resolution), Error> {
    let (path_part, query, fragment) = split_components(semantic);
    let beside = directory(document_path.as_bytes());
    let mut anchors = anchors(
        resolver.snapshot,
        adapter,
        document_path,
        construct,
        is_image,
        path_part,
    );
    if template_expression(semantic)
        || (adapter == Adapter::AsciiDoc
            && ((is_image && anchors.is_empty()) || awaits_attribute(semantic)))
    {
        return Ok((
            unsupported_intent(query, fragment),
            Resolution::UnsupportedSemantics(UnsupportedSemantics::AttributeDependent),
        ));
    }

    if fragment
        .as_deref()
        .is_some_and(|raw| decode_fragment(raw).is_none())
    {
        return Ok(undecodable_fragment(path_part, query, fragment));
    }

    if let Some(resolution) = unreadable(path_part) {
        return Ok((unsupported_intent(query, fragment), resolution));
    }
    if anchors.is_empty() {
        if let Some(scheme) = scheme(path_part) {
            return absolute(resolver, context, path_part, scheme, query, fragment);
        }
        if path_part.starts_with('/') {
            return Ok((
                Intent {
                    kind: IntentKind::SiteRoute,
                    ..unsupported_intent(query, fragment)
                },
                Resolution::UnsupportedSemantics(UnsupportedSemantics::SiteRoute),
            ));
        }
        if generator_alias(path_part) {
            return Ok((
                unsupported_intent(query, fragment),
                Resolution::UnsupportedSemantics(UnsupportedSemantics::AttributeDependent),
            ));
        }
        anchors.push((beside.to_vec(), path_part.to_owned()));
    }
    if adapter == Adapter::AsciiDoc && names_a_page_identity(path_part) {
        return Ok((
            unsupported_intent(query, fragment),
            Resolution::UnsupportedSemantics(UnsupportedSemantics::AttributeDependent),
        ));
    }
    let forge = context.map(|identity| identity.dialect);
    if path_part.is_empty() {
        let target_kind = if is_image {
            TargetKind::Blob
        } else {
            TargetKind::Either
        };
        let row = lookup(
            resolver,
            document_path,
            target_kind,
            query.as_deref(),
            fragment.as_deref(),
            forge,
        )?;
        let named = matches!(
            row,
            Resolution::Missing(Missing::HeadingAnchorNotFound { .. })
        )
        .then(|| fragment.as_deref().and_then(decode_fragment))
        .flatten();
        let row = linked_label(resolver, adapter, document_path, named.as_deref(), row)?;
        return Ok((
            repository_intent(document_path.clone(), target_kind, query, fragment),
            row,
        ));
    }
    let bare = (!is_image && query.is_none() && fragment.is_none()).then_some(path_part);
    let (intent, row) = native(
        resolver,
        is_image,
        document_path,
        &anchors,
        query,
        fragment,
        forge,
    )?;
    let named = matches!(row, Resolution::Missing(Missing::PathNotFound { .. }))
        .then_some(bare)
        .flatten();
    Ok((
        intent,
        linked_label(resolver, adapter, document_path, named, row)?,
    ))
}

/// Native destinations: one terminal slash is an authored directory hint on a
/// link and invalid on an image; segments decode once and are contained under
/// each anchoring directory in turn while normalizing `.` and internal `..`.
/// The reading from the document's own directory fixes the intent wherever a
/// rule kept one, since that is the path the author wrote, and the first
/// anchor the tree holds, as written or under a router spelling, is the
/// target that answers. A path a declared generator serves from its own build
/// is undecided rather than absent, since no tree holds that answer.
fn native(
    resolver: &mut Resolver<'_>,
    is_image: bool,
    document: &RepoPath,
    anchors: &[(Vec<u8>, String)],
    query: Option<String>,
    fragment: Option<String>,
    forge: Option<ForgeDialect>,
) -> Result<(Intent, Resolution), Error> {
    let beside = directory(document.as_bytes());
    let authored = anchors
        .iter()
        .find(|(parent, _)| parent == beside)
        .or_else(|| anchors.first())
        .ok_or(Error::Internal)?;
    let (path, target_kind) = match normalized_path_under(&authored.0, is_image, &authored.1) {
        Ok(target) => target,
        Err(resolution) => return Ok((unsupported_intent(query, fragment), resolution)),
    };
    let served = anchors
        .iter()
        .find_map(|(parent, relative)| {
            let (candidate, kind) = normalized_path_under(parent, is_image, relative).ok()?;
            let route = routed(resolver.snapshot, &candidate, kind);
            resolver.snapshot.locate(&route).is_some().then_some(route)
        })
        .or_else(|| redirected(resolver.snapshot, document, anchors, is_image));
    let row = lookup(
        resolver,
        served.as_ref().unwrap_or(&path),
        target_kind,
        query.as_deref(),
        fragment.as_deref(),
        forge,
    )?;
    let undecided = matches!(&row, Resolution::Missing(Missing::PathNotFound { path, .. })
        if unplaced(resolver.snapshot, document, path));
    let row = if undecided {
        Resolution::UnsupportedSemantics(UnsupportedSemantics::UnmodelledRoute)
    } else {
        row
    };
    Ok((repository_intent(path, target_kind, query, fragment), row))
}

/// A fragment whose escapes do not decode ends the reading before the tree is
/// asked, and a leading slash still names the site route it named.
fn undecodable_fragment(
    path_part: &str,
    query: Option<String>,
    fragment: Option<String>,
) -> (Intent, Resolution) {
    let intent = if path_part.starts_with('/') && !path_part.starts_with("//") {
        Intent {
            kind: IntentKind::SiteRoute,
            commit_oid: None,
            repository_path: None,
            target_kind: None,
            external_scheme: None,
            query,
            fragment,
        }
    } else {
        unsupported_intent(query, fragment)
    };
    (
        intent,
        Resolution::Invalid {
            reason: InvalidReference::FragmentEncoding,
        },
    )
}

fn repository_intent(
    path: RepoPath,
    target_kind: TargetKind,
    query: Option<String>,
    fragment: Option<String>,
) -> Intent {
    Intent {
        kind: IntentKind::RepositoryPath,
        commit_oid: None,
        repository_path: Some(path),
        target_kind: Some(target_kind),
        external_scheme: None,
        query,
        fragment,
    }
}

/// A page identity is answered by a site catalogue this engine does not build.
fn names_a_page_identity(path_part: &str) -> bool {
    path_part
        .rsplit('/')
        .next()
        .is_some_and(|segment| !segment.is_empty() && !segment.contains('.'))
}

/// An attribute value, and the `imagesdir` an image macro needs, arrive at
/// build time.
fn awaits_attribute(semantic: &str) -> bool {
    let mut rest = semantic;
    while let Some(open) = rest.find('{') {
        let after = rest.get(open.saturating_add(1)..).unwrap_or_default();
        if let Some(close) = after.find('}')
            && close > 0
            && after.get(..close).is_some_and(|name| {
                name.chars().all(|character| {
                    character.is_alphanumeric() || character == '-' || character == '_'
                })
            })
        {
            return true;
        }
        rest = after;
    }
    false
}

/// The last question a path the tree does not hold is asked. Only ignore files
/// on its own ancestor chain can name it, and the nearest one answers, so the
/// report carries the declaration closest to the target.
fn declared_untracked(resolver: &mut Resolver<'_>, path: &RepoPath) -> Result<Resolution, Error> {
    let raw = path.as_bytes();
    let separators = raw
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(index, byte)| (*byte == b'/').then_some(index));
    for split in separators.map(Some).chain([None]) {
        let (directory, relative) = match split {
            Some(index) => (
                raw.get(..index).unwrap_or_default(),
                raw.get(index.saturating_add(1)..).unwrap_or_default(),
            ),
            None => ([].as_slice(), raw),
        };
        let mut spelled = directory.to_vec();
        if !spelled.is_empty() {
            spelled.push(b'/');
        }
        spelled.extend_from_slice(b".gitignore");
        let Some(ignore_path) = RepoPath::from_bytes(spelled) else {
            continue;
        };
        if declares(resolver, &ignore_path, relative)? {
            return Ok(Resolution::DeclaredUntracked(DeclaredUntracked {
                path: path.clone(),
                declared_by: ignore_path,
            }));
        }
    }
    Ok(Resolution::Missing(Missing::PathNotFound {
        path: path.clone(),
        near: case_neighbor(resolver.snapshot, path),
        same_object_at: None,
    }))
}

/// The one tracked path equal to the missed one apart from case, when exactly
/// one exists. A repository holding both spellings names a real ambiguity and
/// stays bare, and so does a path nothing in the tree comes close to.
fn case_neighbor(snapshot: &SnapshotDiscovery, path: &RepoPath) -> Option<RepoPath> {
    let raw = path.as_bytes();
    let mut matches = snapshot
        .entries
        .keys()
        .filter(|entry| entry.as_bytes().eq_ignore_ascii_case(raw));
    let candidate = matches.next()?;
    matches.next().is_none().then(|| candidate.clone())
}

fn declares(
    resolver: &mut Resolver<'_>,
    ignore_path: &RepoPath,
    relative: &[u8],
) -> Result<bool, Error> {
    if let Some(cached) = resolver.cache.declarations.get(ignore_path) {
        return Ok(cached.declares(relative));
    }
    let Some(Located::Entry(GitMode::RegularFile | GitMode::ExecutableFile, oid)) =
        resolver.snapshot.locate(ignore_path)
    else {
        return Ok(false);
    };
    let oid = oid.clone();
    let cap = ValueCap {
        resource: ResourceName::IgnoreDeclarationBlobBytes,
        limit: resolver.scan.limits().ignore_declaration_blob_bytes,
    };
    let object = resolver
        .repo
        .read_expected_capped(resolver.git, &oid, ObjectKind::Blob, cap)
        .map_err(Error::from)?;
    resolver.scan.charge(
        Aggregate::IgnoreDeclarationBytes,
        u64::try_from(object.body.len()).unwrap_or(u64::MAX),
    )?;
    let parsed = Declarations::parse(&object.body);
    let answer = parsed.declares(relative);
    resolver
        .cache
        .declarations
        .insert(ignore_path.clone(), parsed);
    Ok(answer)
}

/// A located regular file, with its content read and digested under the caps.
fn blob_target(
    resolver: &mut Resolver<'_>,
    path: &RepoPath,
    mode: GitMode,
    oid: &Oid,
) -> Result<Target<RepoPath>, Error> {
    let content = read_target(resolver, path, mode, oid)?;
    let mode = match mode {
        GitMode::RegularFile => BlobMode::Regular,
        GitMode::ExecutableFile => BlobMode::Executable,
        GitMode::Tree | GitMode::Symlink | GitMode::Gitlink => return Err(Error::Internal),
    };
    Ok(Target::Blob(BlobTarget {
        path: path.clone(),
        mode,
        content,
    }))
}

/// Steps four through ten: exact lookup, special entries, kind compatibility,
/// content availability, query semantics, fragment semantics, and only then
/// a resolved target. The typed target survives query and fragment boundary
/// outcomes so downstream consumers retain the evidence they can evaluate.
pub(super) fn lookup(
    resolver: &mut Resolver<'_>,
    path: &RepoPath,
    target_kind: TargetKind,
    query: Option<&str>,
    fragment: Option<&str>,
    forge: Option<ForgeDialect>,
) -> Result<Resolution, Error> {
    let (mode, entry) = match resolver.snapshot.locate(path) {
        None => {
            return declared_untracked(resolver, path);
        }
        Some(Located::Entry(GitMode::Symlink, _)) => {
            return Ok(Resolution::UnsupportedTarget(UnsupportedTarget::Symlink {
                path: path.clone(),
            }));
        }
        Some(Located::Entry(GitMode::Gitlink, _)) => {
            return Ok(Resolution::UnsupportedTarget(UnsupportedTarget::Gitlink {
                path: path.clone(),
            }));
        }
        Some(Located::ImpliedTree | Located::Entry(GitMode::Tree, _)) => {
            (GitMode::Tree, Target::Tree { path: path.clone() })
        }
        Some(Located::Entry(mode @ (GitMode::RegularFile | GitMode::ExecutableFile), oid)) => {
            let oid = oid.clone();
            (mode, blob_target(resolver, path, mode, &oid)?)
        }
    };

    if let Some(refusal) = refusal(
        resolver.snapshot,
        path,
        mode,
        target_kind,
        query,
        entry.clone(),
    ) {
        return Ok(refusal);
    }

    match fragment {
        Some(raw_fragment) if !raw_fragment.is_empty() => {
            let decoded = decode_fragment(raw_fragment).unwrap_or_default();
            fragment_resolution(resolver, path, mode, entry, forge, &decoded)
        }
        Some(_) | None => Ok(Resolution::Resolved { target: entry }),
    }
}

/// The two answers a located target can carry before its fragment is read: a
/// promised kind the entry is not, and a query the run cannot evaluate.
fn refusal(
    snapshot: &SnapshotDiscovery,
    path: &RepoPath,
    mode: GitMode,
    target_kind: TargetKind,
    query: Option<&str>,
    entry: Target<RepoPath>,
) -> Option<Resolution> {
    let is_tree = mode == GitMode::Tree;
    let compatible = match target_kind {
        TargetKind::Blob => !is_tree,
        TargetKind::Tree => is_tree,
        TargetKind::Either => true,
    };
    if !compatible {
        return Some(Resolution::TypeMismatch { target: entry });
    }
    let evaluable = !is_tree
        && classify(path.as_bytes()).is_some_and(|class| class != Classification::PlainAdvisory)
        && snapshot.is_scanned_structured(path);
    match query {
        Some(_) if !evaluable => Some(Resolution::UnsupportedSemantics(
            UnsupportedSemantics::Query(entry),
        )),
        Some(_) | None => None,
    }
}
