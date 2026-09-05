use std::collections::BTreeMap;
use std::sync::Arc;

use amiss_git::{GitResources, ObjectKind, Repository, ValueCap};
use amiss_wire::controls::{GitMode, ResourceName, TargetKind};
use amiss_wire::model::{Adapter, ForgeDialect, ObjectFormat, Oid, RepoPath};
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
use crate::resources::{Aggregate, ScanResources};
use crate::route::candidates;

mod anchor;
mod content;
mod forge;
mod history;
mod line;
mod site;
mod syntax;
mod transclusion;

pub(crate) use line::{LineRange, named_region_bytes, safe_line_number, selected_line_bytes};

use anchor::fragment_resolution;
use content::{CachedContent, read_target};
use syntax::{normalized_native_path, same_repo_suffix, split_components, unsupported_intent};

pub use amiss_wire::digest::RAW_EVIDENCE_DOMAIN;
pub const TARGET_PROJECTION_DOMAIN: &str = "amiss/scanner-target-projection";
pub const TARGET_LINE_PROJECTION_DOMAIN: &str = "amiss/scanner-target-line-projection";

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
    pub host: String,
    pub dialect: ForgeDialect,
    pub object_format: ObjectFormat,
    pub owner: String,
    pub repository: String,
    pub candidate_ref: String,
    pub default_ref: String,
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
        cache.bind(scan.cache_scope());
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
        if adapter == Adapter::AsciiDoc && (is_image || awaits_attribute(semantic)) {
            let (_, query, fragment) = split_components(semantic);
            return Ok((
                unsupported_intent(query, fragment),
                Resolution::UnsupportedSemantics(UnsupportedSemantics::AttributeDependent),
            ));
        }
        let (path_part, query, fragment) = split_components(semantic);

        if let Some(raw_fragment) = &fragment
            && decode_fragment(raw_fragment).is_none()
        {
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
            return Ok((
                intent,
                Resolution::Invalid {
                    reason: InvalidReference::FragmentEncoding,
                },
            ));
        }

        if path_part.starts_with("//") {
            return Ok((
                unsupported_intent(query, fragment),
                Resolution::UnsupportedSemantics(UnsupportedSemantics::NetworkPath),
            ));
        }
        if let Some(scheme) = scheme(path_part) {
            return absolute(self, context, path_part, scheme, query, fragment);
        }
        if path_part.starts_with('/') {
            return Ok((
                Intent {
                    kind: IntentKind::SiteRoute,
                    commit_oid: None,
                    repository_path: None,
                    target_kind: None,
                    external_scheme: None,
                    query,
                    fragment,
                },
                Resolution::UnsupportedSemantics(UnsupportedSemantics::SiteRoute),
            ));
        }
        if adapter == Adapter::AsciiDoc && names_a_page_identity(path_part) {
            return Ok((
                unsupported_intent(query, fragment),
                Resolution::UnsupportedSemantics(UnsupportedSemantics::AttributeDependent),
            ));
        }
        native(
            self,
            document_path,
            is_image,
            path_part,
            query,
            fragment,
            context.map(|identity| identity.dialect),
        )
    }

    pub(crate) fn resolve_scanned(
        &mut self,
        context: Option<&ForgeContext>,
        semantic: crate::semantic::View<'_>,
        adapter: Adapter,
        document_path: &RepoPath,
        occurrence: &crate::scan::ScannedOccurrence,
    ) -> Result<(Intent, Resolution, Option<String>), Error> {
        if occurrence.occurrence.construct == amiss_wire::controls::SourceConstruct::RstRefRole {
            return self.resolve_label(&occurrence.occurrence.semantic_destination, semantic);
        }
        let is_image = occurrence.occurrence.construct.is_image();
        let (intent, mut resolution) = self.resolve(
            context,
            adapter,
            document_path,
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
        && let Some(suffix) = same_repo_suffix(path_part, &identity.host)
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

/// Native destinations: empty targets the source document itself; one
/// terminal slash is an authored directory hint on a link and invalid on an
/// image; segments decode once and are contained relative to the source
/// document's parent while normalizing `.` and internal `..`.
fn native(
    resolver: &mut Resolver<'_>,
    document_path: &RepoPath,
    is_image: bool,
    path_part: &str,
    query: Option<String>,
    fragment: Option<String>,
    forge: Option<ForgeDialect>,
) -> Result<(Intent, Resolution), Error> {
    let terminal = |resolution: Resolution, query: Option<String>, fragment: Option<String>| {
        (unsupported_intent(query, fragment), resolution)
    };

    let (path, target_kind, route) = if path_part.is_empty() {
        (
            document_path.clone(),
            if is_image {
                TargetKind::Blob
            } else {
                TargetKind::Either
            },
            None,
        )
    } else {
        let (path, target_kind) = match normalized_native_path(document_path, is_image, path_part) {
            Ok(target) => target,
            Err(resolution) => return Ok(terminal(resolution, query, fragment)),
        };
        let route = routed(resolver.snapshot, &path, target_kind);
        (path, target_kind, Some(route))
    };
    let row = lookup(
        resolver,
        route.as_ref().unwrap_or(&path),
        target_kind,
        query.as_deref(),
        fragment.as_deref(),
        forge,
    )?;
    Ok((
        Intent {
            kind: IntentKind::RepositoryPath,
            commit_oid: None,
            repository_path: Some(path),
            target_kind: Some(target_kind),
            external_scheme: None,
            query,
            fragment,
        },
        row,
    ))
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

/// The path this reference is answered against. A destination the tree holds
/// is its own answer; otherwise the first router spelling that reaches an
/// ordinary file stands in for it. A promised directory is never re-spelled,
/// and a spelling reaches nothing that is not already a file, so this can only
/// turn an absent target into a present one.
fn routed(snapshot: &SnapshotDiscovery, path: &RepoPath, target_kind: TargetKind) -> RepoPath {
    if target_kind == TargetKind::Tree || snapshot.locate(path).is_some() {
        return path.clone();
    }
    candidates(path)
        .into_iter()
        .find(|(_, candidate)| {
            matches!(
                snapshot.locate(candidate),
                Some(Located::Entry(
                    GitMode::RegularFile | GitMode::ExecutableFile,
                    _
                ))
            )
        })
        .map_or_else(|| path.clone(), |(_, candidate)| candidate)
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

/// A located directory. A tree target has no content to read, which lets an
/// index answer for one without a tree identity.
fn tree_target(path: &RepoPath) -> Target<RepoPath> {
    Target::Tree { path: path.clone() }
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
            (GitMode::Tree, tree_target(path))
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
