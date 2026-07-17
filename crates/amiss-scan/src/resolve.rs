use std::collections::BTreeMap;
use std::sync::Arc;

use amiss_git::{GitResources, ObjectKind, Repository, ValueCap};
use amiss_md::lines::scan;
use amiss_wire::controls::{GitMode, ResourceName, TargetKind};
use amiss_wire::digest::{Digest, hb, hj};
use amiss_wire::json::Value;
use amiss_wire::model::{ForgeDialect, Oid, RepoPath};
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::{
    BlobContent, BlobMode, BlobTarget, ExternalReference, InvalidReference, Missing,
    Resolution as WireResolution, Target, UnsupportedSemantics, UnsupportedTarget,
};

use crate::discovery::{Located, SnapshotDiscovery};
use crate::document::classify;
use crate::resources::ScanResources;
use crate::{Error, lfs};

/// Trusted same-repository URL dialects. Generic URI classification and
/// target reads remain in this parent module.
mod forge;

pub use amiss_wire::digest::RAW_EVIDENCE_DOMAIN;
pub const TARGET_PROJECTION_DOMAIN: &str = "amiss/scanner-target-projection";
pub const TARGET_LINE_PROJECTION_DOMAIN: &str = "amiss/scanner-target-line-projection";

const MAX_SAFE: u64 = 9_007_199_254_740_991;

/// The occurrence's target intent, fixed after component splitting and before
/// any repository lookup. This, not the eventual resolution, fixes identity
/// and summary membership.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Intent {
    pub kind: IntentKind,
    pub repository_path: Option<RepoPath>,
    pub target_kind: Option<TargetKind>,
    pub external_scheme: Option<String>,
    pub query: Option<String>,
    pub fragment: Option<String>,
}

/// One occurrence's typed resolution against a binary-safe repository path.
pub type Resolution = WireResolution<RepoPath>;

/// The trusted run context for same-repository recognition: the declared
/// host and dialect, lowercase owner and repository, the two exact full
/// branch refs, and the candidate commit for OID-pinned dialect forms.
/// Without it every absolute forge URL remains an external URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeContext {
    pub host: String,
    pub dialect: ForgeDialect,
    pub owner: String,
    pub repository: String,
    pub candidate_ref: String,
    pub default_ref: String,
    pub candidate_oid: Option<String>,
}

/// The recognition opening: `https://`, the declared host byte-exact, then
/// the path separator. Anything less exact is not this repository's forge.
fn same_repo_suffix<'a>(path_part: &'a str, host: &str) -> Option<&'a str> {
    path_part
        .strip_prefix("https://")?
        .strip_prefix(host)?
        .strip_prefix('/')
}

/// Referenced targets are read once per path and Git object within one scan
/// resource scope. Reusing a cache with another scope clears its evidence.
#[derive(Debug, Default)]
pub struct TargetCache {
    scope: Option<Arc<()>>,
    read: BTreeMap<RepoPath, CachedContent>,
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
        self.scope = Some(Arc::clone(scope));
    }
}

#[derive(Debug)]
struct CachedContent {
    mode: GitMode,
    oid: Oid,
    content: Content,
}

#[derive(Debug)]
enum Content {
    Ordinary {
        raw_digest: Digest,
        projection_digest: Digest,
        body: Box<[u8]>,
        line_projections: BTreeMap<LineRange, Option<Digest>>,
    },
    LfsPointer {
        raw_digest: Digest,
    },
}

impl Content {
    const fn evidence(&self) -> BlobContent {
        match self {
            Self::Ordinary {
                raw_digest,
                projection_digest,
                ..
            } => BlobContent::Available {
                raw_digest: *raw_digest,
                projection_digest: *projection_digest,
            },
            Self::LfsPointer { raw_digest } => BlobContent::LfsPointer {
                raw_digest: *raw_digest,
            },
        }
    }
}

fn unsupported_intent(query: Option<String>, fragment: Option<String>) -> Intent {
    Intent {
        kind: IntentKind::Unsupported,
        repository_path: None,
        target_kind: None,
        external_scheme: None,
        query,
        fragment,
    }
}

/// Resolves one occurrence's semantic destination against one snapshot,
/// following the total precedence: split, validate the fragment encoding,
/// classify, decode and contain a repository path, look it up, then apply
/// query and fragment semantics. The first terminal row wins.
///
/// # Errors
///
/// A target read defect or a snapshot budget crossing; every syntactic or
/// structural outcome is a `Resolution`, never an error.
#[expect(
    clippy::too_many_arguments,
    reason = "the resolver context is the contract's"
)]
pub fn resolve(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    cache: &mut TargetCache,
    snapshot: &SnapshotDiscovery,
    context: Option<&ForgeContext>,
    document_path: &RepoPath,
    is_image: bool,
    semantic: &str,
) -> Result<(Intent, Resolution), Error> {
    cache.bind(scan.cache_scope());
    let (path_part, query, fragment) = split_components(semantic);

    if let Some(raw_fragment) = &fragment
        && decode_fragment(raw_fragment).is_none()
    {
        return Ok((
            unsupported_intent(query, fragment.clone()),
            Resolution::Invalid(InvalidReference::FragmentEncoding),
        ));
    }

    if path_part.starts_with("//") {
        return Ok((
            unsupported_intent(query, fragment),
            Resolution::UnsupportedSemantics(UnsupportedSemantics::NetworkPath),
        ));
    }
    if let Some(scheme) = scheme_of(&path_part) {
        return absolute(
            repo, git, scan, cache, snapshot, context, &path_part, &scheme, query, fragment,
        );
    }
    if path_part.starts_with('/') {
        return Ok((
            Intent {
                kind: IntentKind::SiteRoute,
                repository_path: None,
                target_kind: None,
                external_scheme: None,
                query,
                fragment,
            },
            Resolution::UnsupportedSemantics(UnsupportedSemantics::SiteRoute),
        ));
    }
    native(
        repo,
        git,
        scan,
        cache,
        snapshot,
        document_path,
        is_image,
        &path_part,
        query,
        fragment,
        context.map(|identity| identity.dialect),
    )
}

/// RFC 3986 order: the first `#` opens the fragment through end; within the
/// prefix the first `?` opens the query. `a?x?y#z?u` has query `x?y` and
/// fragment `z?u`. A field is absent exactly when its delimiter is.
fn split_components(semantic: &str) -> (String, Option<String>, Option<String>) {
    let (before, fragment) = match semantic.split_once('#') {
        Some((before, after)) => (before, Some(after.to_owned())),
        None => (semantic, None),
    };
    let (path, query) = match before.split_once('?') {
        Some((path, after)) => (path, Some(after.to_owned())),
        None => (before, None),
    };
    (path.to_owned(), query, fragment)
}

fn scheme_of(path_part: &str) -> Option<String> {
    let mut bytes = path_part.bytes();
    let first = bytes.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    let mut length = 1_usize;
    for byte in bytes {
        match byte {
            b':' => {
                return path_part.get(..length).map(str::to_owned);
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'.' | b'-' => {
                length = length.saturating_add(1);
            }
            _ => return None,
        }
    }
    None
}

/// One percent decode, never repeated: `%25` becomes a literal `%` and stays
/// one.
fn decode_bytes(text: &str) -> Result<Vec<u8>, Resolution> {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut at = 0_usize;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%' {
            let high = bytes.get(at.saturating_add(1)).copied();
            let low = bytes.get(at.saturating_add(2)).copied();
            let (Some(high), Some(low)) = (high, low) else {
                return Err(Resolution::Invalid(InvalidReference::PercentEncoding));
            };
            let (Some(high), Some(low)) = (hex_value(high), hex_value(low)) else {
                return Err(Resolution::Invalid(InvalidReference::PercentEncoding));
            };
            out.push(high.wrapping_shl(4) | low);
            at = at.saturating_add(3);
            continue;
        }
        out.push(byte);
        at = at.saturating_add(1);
    }
    Ok(out)
}

/// Decodes one path segment to its raw bytes. The input holds no raw
/// separator, so a decoded slash could only create one; a decoded backslash,
/// control, or NUL is a defect either way. Bytes outside UTF-8 are ordinary
/// path bytes.
fn decode_segment(segment: &str) -> Result<Vec<u8>, Resolution> {
    let out = decode_bytes(segment)?;
    for &byte in &out {
        match byte {
            b'/' => return Err(Resolution::Invalid(InvalidReference::EncodedSlash)),
            b'\\' => return Err(Resolution::Invalid(InvalidReference::BackslashSeparator)),
            0..=0x1f | 0x7f => {
                return Err(Resolution::Invalid(InvalidReference::DecodedPathControl));
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Decodes a fragment: only invalid escapes, invalid UTF-8, and control bytes
/// invalidate it; separators are ordinary fragment characters.
fn decode_fragment(fragment: &str) -> Option<String> {
    let out = decode_bytes(fragment).ok()?;
    if out.iter().any(|&byte| matches!(byte, 0..=0x1f | 0x7f)) {
        return None;
    }
    String::from_utf8(out).ok()
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte.wrapping_sub(b'0')),
        b'a'..=b'f' => Some(byte.wrapping_sub(b'a').wrapping_add(10)),
        b'A'..=b'F' => Some(byte.wrapping_sub(b'A').wrapping_add(10)),
        _ => None,
    }
}

/// Absolute URIs under `uri-reference`: ASCII generic syntax, no
/// normalization, two-hex-digit escapes, and for HTTP(S) a `//` plus nonempty
/// authority. Only the emitted scheme is lowercased. The exact `https://`
/// spelling of the declared host opens same-repository recognition; without
/// a declared forge context every syntactically valid absolute URI is
/// external.
#[expect(
    clippy::too_many_arguments,
    reason = "the resolver context is the contract's"
)]
fn absolute(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    cache: &mut TargetCache,
    snapshot: &SnapshotDiscovery,
    context: Option<&ForgeContext>,
    path_part: &str,
    scheme: &str,
    query: Option<String>,
    fragment: Option<String>,
) -> Result<(Intent, Resolution), Error> {
    let invalid = |query: Option<String>, fragment: Option<String>| {
        (
            unsupported_intent(query, fragment),
            Resolution::Invalid(InvalidReference::Uri),
        )
    };
    if !uri_bytes_valid(path_part) || query.as_deref().is_some_and(|text| !uri_bytes_valid(text)) {
        return Ok(invalid(query, fragment));
    }
    let lower = scheme.to_ascii_lowercase();
    let after_scheme = path_part
        .get(scheme.len().saturating_add(1)..)
        .unwrap_or_default();
    if lower == "http" || lower == "https" {
        let Some(rest) = after_scheme.strip_prefix("//") else {
            return Ok(invalid(query, fragment));
        };
        let authority_end = rest.find('/').unwrap_or(rest.len());
        let authority = rest.get(..authority_end).unwrap_or_default();
        if authority.is_empty() || !authority_valid(authority) {
            return Ok(invalid(query, fragment));
        }
    }
    if let Some(identity) = context
        && let Some(suffix) = same_repo_suffix(path_part, &identity.host)
    {
        return forge::resolve(
            repo, git, scan, cache, snapshot, identity, suffix, query, fragment,
        );
    }
    Ok((
        Intent {
            kind: IntentKind::ExternalUrl,
            repository_path: None,
            target_kind: None,
            external_scheme: Some(lower),
            query,
            fragment,
        },
        Resolution::External(ExternalReference::Url),
    ))
}

/// The ASCII RFC 3986 generic-syntax charset with two-hex-digit escapes:
/// unreserved, gen-delims, and sub-delims only, so a space, angle bracket,
/// quote, or non-ASCII byte is an invalid URI rather than data.
fn uri_bytes_valid(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut at = 0_usize;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%' {
            let pair = (
                bytes.get(at.saturating_add(1)).copied().and_then(hex_value),
                bytes.get(at.saturating_add(2)).copied().and_then(hex_value),
            );
            if !matches!(pair, (Some(_), Some(_))) {
                return false;
            }
            at = at.saturating_add(3);
            continue;
        }
        let allowed = byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'.'
                    | b'_'
                    | b'~'
                    | b':'
                    | b'/'
                    | b'?'
                    | b'['
                    | b']'
                    | b'@'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b','
                    | b';'
                    | b'='
            );
        if !allowed {
            return false;
        }
        at = at.saturating_add(1);
    }
    true
}

fn authority_valid(authority: &str) -> bool {
    if !authority.is_ascii() {
        return false;
    }
    if let Some(host) = authority.strip_prefix('[') {
        let Some((inside, port)) = host.split_once(']') else {
            return false;
        };
        return !inside.is_empty()
            && (port.is_empty()
                || port
                    .strip_prefix(':')
                    .is_some_and(|p| p.bytes().all(|b| b.is_ascii_digit())));
    }
    !authority.contains(['[', ']'])
}

/// Native destinations: empty targets the source document itself; one
/// terminal slash is an authored directory hint on a link and invalid on an
/// image; segments decode once and are contained relative to the source
/// document's parent while normalizing `.` and internal `..`.
#[expect(
    clippy::too_many_arguments,
    reason = "the resolver context is the contract's"
)]
fn native(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    cache: &mut TargetCache,
    snapshot: &SnapshotDiscovery,
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

    if path_part.is_empty() {
        return self_target(
            repo,
            git,
            scan,
            cache,
            snapshot,
            document_path,
            is_image,
            query.as_deref(),
            fragment.as_deref(),
            forge,
        );
    }
    let (joined, target_kind) = match normalized_native_path(document_path, is_image, path_part) {
        Ok(target) => target,
        Err(resolution) => return Ok(terminal(resolution, query, fragment)),
    };

    let intent = Intent {
        kind: IntentKind::RepositoryPath,
        repository_path: Some(joined.clone()),
        target_kind: Some(target_kind),
        external_scheme: None,
        query: query.clone(),
        fragment: fragment.clone(),
    };
    let row = lookup(
        repo,
        git,
        scan,
        cache,
        snapshot,
        &joined,
        target_kind,
        query.as_deref(),
        fragment.as_deref(),
        forge,
    )?;
    Ok((intent, row))
}

fn normalized_native_path(
    document_path: &RepoPath,
    is_image: bool,
    path_part: &str,
) -> Result<(RepoPath, TargetKind), Resolution> {
    if path_part.contains('\\') {
        return Err(Resolution::Invalid(InvalidReference::BackslashSeparator));
    }
    let mut segments: Vec<&str> = path_part.split('/').collect();
    let trailing_slash = segments.len() > 1 && segments.last() == Some(&"");
    if trailing_slash {
        segments.pop();
    }
    if segments.iter().any(|segment| segment.is_empty()) || (trailing_slash && is_image) {
        return Err(Resolution::Invalid(InvalidReference::Syntax));
    }
    let target_kind = if trailing_slash {
        TargetKind::Tree
    } else if is_image {
        TargetKind::Blob
    } else {
        TargetKind::Either
    };

    let raw_document = document_path.as_bytes();
    let mut resolved: Vec<Vec<u8>> = match raw_document.iter().rposition(|byte| *byte == b'/') {
        Some(split) => raw_document
            .get(..split)
            .unwrap_or_default()
            .split(|byte| *byte == b'/')
            .map(<[u8]>::to_vec)
            .collect(),
        None => Vec::new(),
    };
    for segment in segments {
        let decoded = decode_segment(segment)?;
        match decoded.as_slice() {
            b"." => {}
            b".." => {
                if resolved.pop().is_none() {
                    return Err(Resolution::Invalid(InvalidReference::PathTraversal));
                }
            }
            _ => resolved.push(decoded),
        }
    }
    let Some(joined) = RepoPath::from_bytes(resolved.join(&b'/')) else {
        return Err(Resolution::Invalid(InvalidReference::Syntax));
    };
    Ok((joined, target_kind))
}

/// An empty native destination targets the source document itself, whether
/// or not a query or fragment is present.
#[expect(
    clippy::too_many_arguments,
    reason = "the resolver context is the contract's"
)]
fn self_target(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    cache: &mut TargetCache,
    snapshot: &SnapshotDiscovery,
    document_path: &RepoPath,
    is_image: bool,
    query: Option<&str>,
    fragment: Option<&str>,
    forge: Option<ForgeDialect>,
) -> Result<(Intent, Resolution), Error> {
    let self_kind = if is_image {
        TargetKind::Blob
    } else {
        TargetKind::Either
    };
    let intent = Intent {
        kind: IntentKind::RepositoryPath,
        repository_path: Some(document_path.clone()),
        target_kind: Some(self_kind),
        external_scheme: None,
        query: query.map(str::to_owned),
        fragment: fragment.map(str::to_owned),
    };
    let row = lookup(
        repo,
        git,
        scan,
        cache,
        snapshot,
        document_path,
        self_kind,
        query,
        fragment,
        forge,
    )?;
    Ok((intent, row))
}

/// A located directory. A tree target has no content to read, which lets an
/// index answer for one without a tree identity.
fn tree_target(path: &RepoPath) -> Target<RepoPath> {
    Target::Tree { path: path.clone() }
}

/// A located regular file, with its content read and digested under the caps.
fn blob_target(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    cache: &mut TargetCache,
    path: &RepoPath,
    mode: GitMode,
    oid: &Oid,
) -> Result<Target<RepoPath>, Error> {
    let content = read_target(repo, git, scan, cache, path, mode, oid)?;
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
#[expect(
    clippy::too_many_arguments,
    reason = "the resolver context is the contract's"
)]
fn lookup(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    cache: &mut TargetCache,
    snapshot: &SnapshotDiscovery,
    path: &RepoPath,
    target_kind: TargetKind,
    query: Option<&str>,
    fragment: Option<&str>,
    forge: Option<ForgeDialect>,
) -> Result<Resolution, Error> {
    let (mode, entry) = match snapshot.locate(path) {
        None => {
            return Ok(Resolution::Missing(Missing::PathNotFound {
                path: path.clone(),
            }));
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
            (mode, blob_target(repo, git, scan, cache, path, mode, oid)?)
        }
    };

    let is_tree = mode == GitMode::Tree;
    let compatible = match target_kind {
        TargetKind::Blob => !is_tree,
        TargetKind::Tree => is_tree,
        TargetKind::Either => true,
    };
    if !compatible {
        return Ok(Resolution::TypeMismatch(entry));
    }

    if query.is_some() {
        let accepted = !is_tree
            && classify(path.as_bytes())
                .is_some_and(|class| class != crate::Classification::PlainAdvisory)
            && snapshot.is_scanned_structured(path);
        if !accepted {
            return Ok(Resolution::UnsupportedSemantics(
                UnsupportedSemantics::Query(entry),
            ));
        }
    }

    if let Some(raw_fragment) = fragment
        && !raw_fragment.is_empty()
    {
        let decoded = decode_fragment(raw_fragment).unwrap_or_default();
        if is_tree {
            return Ok(Resolution::UnsupportedSemantics(
                UnsupportedSemantics::CodeFragment(entry),
            ));
        }
        if let Some(range) = line_fragment(forge, &decoded) {
            let Target::Blob(blob) = entry else {
                return Err(Error::Internal);
            };
            return line_resolution(scan, cache, path, mode, blob, range);
        }
        if classify(path.as_bytes()).is_some() {
            let Target::Blob(blob) = entry else {
                return Err(Error::Internal);
            };
            return Ok(Resolution::UnsupportedSemantics(
                UnsupportedSemantics::Fragment(blob),
            ));
        }
        return Ok(Resolution::UnsupportedSemantics(
            UnsupportedSemantics::CodeFragment(entry),
        ));
    }
    Ok(Resolution::Resolved(entry))
}

/// An inclusive, one-indexed selection of raw source lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct LineRange {
    first: u64,
    last: u64,
}

fn line_resolution(
    scan_resources: &mut ScanResources,
    cache: &mut TargetCache,
    path: &RepoPath,
    mode: GitMode,
    mut blob: BlobTarget<RepoPath>,
    range: LineRange,
) -> Result<Resolution, Error> {
    let Some(cached) = cache.read.get_mut(path) else {
        return Err(Error::Internal);
    };
    if cached.mode != mode || cached.content.evidence() != blob.content {
        return Err(Error::Internal);
    }
    let Content::Ordinary {
        body,
        line_projections,
        ..
    } = &mut cached.content
    else {
        return Ok(Resolution::UnsupportedSemantics(
            UnsupportedSemantics::CodeFragment(Target::Blob(blob)),
        ));
    };

    let projection = if let Some(cached) = line_projections.get(&range).copied() {
        cached
    } else {
        scan_resources.charge_line_fragment_bytes(u64::try_from(body.len()).unwrap_or(u64::MAX))?;
        let projection = selected_line_bytes(body, range).map(|selected| {
            target_projection(
                TARGET_LINE_PROJECTION_DOMAIN,
                mode,
                hb(RAW_EVIDENCE_DOMAIN, selected),
            )
        });
        line_projections.insert(range, projection);
        projection
    };

    let Some(projection_digest) = projection else {
        return Ok(Resolution::Missing(Missing::LineFragmentOutOfRange {
            path: path.clone(),
        }));
    };
    let BlobContent::Available { raw_digest, .. } = blob.content else {
        return Err(Error::Internal);
    };
    blob.content = BlobContent::Available {
        raw_digest,
        projection_digest,
    };
    Ok(Resolution::Resolved(Target::Blob(blob)))
}

/// Line-fragment syntax after one decode, in the dialect's spelling:
/// `L<n>` alone, or the range form the forge renders. First digit nonzero,
/// at most sixteen digits, each number within the safe range, and a range
/// end at least its start. A native reference uses the declared run dialect,
/// falling back to the GitHub/Gitea spelling when no forge context exists.
fn line_fragment(forge: Option<ForgeDialect>, decoded: &str) -> Option<LineRange> {
    fn number(text: &str) -> Option<u64> {
        let bytes = text.as_bytes();
        if bytes.is_empty() || bytes.len() > 16 || bytes.first() == Some(&b'0') {
            return None;
        }
        if !bytes.iter().all(u8::is_ascii_digit) {
            return None;
        }
        text.parse::<u64>().ok().filter(|value| *value <= MAX_SAFE)
    }
    let rest = decoded.strip_prefix('L')?;
    let range = match forge {
        None | Some(ForgeDialect::Github | ForgeDialect::Gitea) => rest.split_once("-L"),
        Some(ForgeDialect::Gitlab) => rest.split_once('-'),
    };
    match range {
        None => number(rest).map(|line| LineRange {
            first: line,
            last: line,
        }),
        Some((start, end)) => match (number(start), number(end)) {
            (Some(first), Some(last)) if last >= first => Some(LineRange { first, last }),
            _ => None,
        },
    }
}

/// Returns the exact byte span from the first selected line through the last,
/// including every original CRLF, bare CR, or LF terminator. The shared line
/// scanner deliberately does not synthesize an empty line after a final
/// terminator, so a range beyond the bytes is absent rather than empty.
fn selected_line_bytes(source: &[u8], range: LineRange) -> Option<&[u8]> {
    let mut line_number = 0_u64;
    let mut selection_start = None;
    for line in scan(source) {
        line_number = line_number.saturating_add(1);
        if line_number == range.first {
            selection_start = Some(line.start);
        }
        if line_number == range.last {
            return source.get(selection_start?..line.end);
        }
    }
    None
}

fn target_projection(domain: &str, mode: GitMode, raw_digest: Digest) -> Digest {
    hj(
        domain,
        &Value::Object(vec![
            (
                "git_mode".to_owned(),
                Value::String(mode.as_str().to_owned()),
            ),
            (
                "raw_digest".to_owned(),
                Value::String(raw_digest.to_string()),
            ),
        ]),
    )
}

/// Reads one referenced regular blob once per exact path, mode, and object
/// identity in the bound scan scope. Pointer content keeps its raw digest and
/// no projection; ordinary content carries both.
fn read_target(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    cache: &mut TargetCache,
    path: &RepoPath,
    mode: GitMode,
    oid: &Oid,
) -> Result<BlobContent, Error> {
    if let Some(cached) = cache.read.get(path)
        && cached.mode == mode
        && &cached.oid == oid
    {
        return Ok(cached.content.evidence());
    }
    let cap = ValueCap {
        resource: ResourceName::ReferencedTargetBlobBytes,
        limit: scan.limits().referenced_target_blob_bytes,
    };
    let object = repo
        .read_expected_capped(git, oid, ObjectKind::Blob, cap)
        .map_err(Error::from)?;
    scan.charge_target_bytes(u64::try_from(object.body.len()).unwrap_or(u64::MAX))?;
    let raw = hb(RAW_EVIDENCE_DOMAIN, &object.body);
    let content = if lfs::is_pointer(&object.body) {
        Content::LfsPointer { raw_digest: raw }
    } else {
        Content::Ordinary {
            raw_digest: raw,
            projection_digest: target_projection(TARGET_PROJECTION_DOMAIN, mode, raw),
            body: object.body.into_boxed_slice(),
            line_projections: BTreeMap::new(),
        }
    };
    let evidence = content.evidence();
    cache.read.insert(
        path.clone(),
        CachedContent {
            mode,
            oid: oid.clone(),
            content,
        },
    );
    Ok(evidence)
}
