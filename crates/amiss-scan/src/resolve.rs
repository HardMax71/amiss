use std::collections::BTreeMap;

use amiss_git::{GitResources, ObjectKind, Repository, ValueCap};
use amiss_wire::controls::{ContentAvailability, EntryKind, GitMode, ResourceName, TargetKind};
use amiss_wire::digest::{Digest, hb, hj};
use amiss_wire::json::Value;
use amiss_wire::model::{ForgeDialect, Oid, RepoPath};
use amiss_wire::report::{IntentKind, ResolutionCode};

use crate::discovery::{Located, SnapshotDiscovery};
use crate::document::classify;
use crate::resources::ScanResources;
use crate::{Error, lfs};

/// Trusted same-repository URL dialects. Generic URI classification and
/// target reads remain in this parent module.
mod forge;

pub use amiss_wire::digest::RAW_EVIDENCE_DOMAIN;
pub const TARGET_PROJECTION_DOMAIN: &str = "amiss/scanner-target-projection";

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

/// One occurrence's sole resolution row, with exactly the entry and content
/// fields its status and code retain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    pub code: ResolutionCode,
    pub path: Option<RepoPath>,
    pub entry_kind: Option<EntryKind>,
    pub git_mode: Option<GitMode>,
    pub raw_digest: Option<Digest>,
    pub projection_digest: Option<Digest>,
    pub content_availability: ContentAvailability,
}

/// The trusted run context for same-repository recognition: the declared
/// host and dialect, lowercase owner and repository, the two exact full
/// branch refs, and the candidate commit for OID-pinned dialect forms.
/// Without it every forge URL is foreign.
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

/// Referenced targets are read once per distinct path; the aggregate budget
/// charges on the first read only.
#[derive(Debug, Default)]
pub struct TargetCache {
    read: BTreeMap<RepoPath, Content>,
}

#[derive(Clone, Debug)]
enum Content {
    Ordinary { raw: Digest, projection: Digest },
    Pointer { raw: Digest },
}

fn null_row(code: ResolutionCode) -> Resolution {
    Resolution {
        code,
        path: None,
        entry_kind: None,
        git_mode: None,
        raw_digest: None,
        projection_digest: None,
        content_availability: ContentAvailability::NotApplicable,
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
    let (path_part, query, fragment) = split_components(semantic);

    if let Some(raw_fragment) = &fragment
        && decode_fragment(raw_fragment).is_none()
    {
        return Ok((
            unsupported_intent(query, fragment.clone()),
            null_row(ResolutionCode::InvalidFragmentEncoding),
        ));
    }

    if path_part.starts_with("//") {
        return Ok((
            unsupported_intent(query, fragment),
            null_row(ResolutionCode::NetworkPathUnsupported),
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
            null_row(ResolutionCode::SiteRouteUnsupported),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DecodeDefect {
    Escape,
    Control,
    Slash,
    Backslash,
}

const fn defect_code(defect: DecodeDefect) -> ResolutionCode {
    match defect {
        DecodeDefect::Escape => ResolutionCode::InvalidPercentEncoding,
        DecodeDefect::Control => ResolutionCode::DecodedPathControl,
        DecodeDefect::Slash => ResolutionCode::EncodedSlash,
        DecodeDefect::Backslash => ResolutionCode::BackslashSeparator,
    }
}

/// One percent decode, never repeated: `%25` becomes a literal `%` and stays
/// one.
fn decode_bytes(text: &str) -> Result<Vec<u8>, DecodeDefect> {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut at = 0_usize;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%' {
            let high = bytes.get(at.saturating_add(1)).copied();
            let low = bytes.get(at.saturating_add(2)).copied();
            let (Some(high), Some(low)) = (high, low) else {
                return Err(DecodeDefect::Escape);
            };
            let (Some(high), Some(low)) = (hex_value(high), hex_value(low)) else {
                return Err(DecodeDefect::Escape);
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
fn decode_segment(segment: &str) -> Result<Vec<u8>, DecodeDefect> {
    let out = decode_bytes(segment)?;
    for &byte in &out {
        match byte {
            b'/' => return Err(DecodeDefect::Slash),
            b'\\' => return Err(DecodeDefect::Backslash),
            0..=0x1f | 0x7f => return Err(DecodeDefect::Control),
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
/// spelling of the declared host opens same-repository recognition, the
/// github.com literal when no identity was declared; everything else
/// syntactically valid is external.
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
            null_row(ResolutionCode::InvalidUri),
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
    let recognition_host = context.map_or("github.com", |identity| identity.host.as_str());
    if let Some(suffix) = same_repo_suffix(path_part, recognition_host) {
        return forge::resolve(
            repo, git, scan, cache, snapshot, context, suffix, query, fragment,
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
        null_row(ResolutionCode::ExternalUrl),
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
) -> Result<(Intent, Resolution), Error> {
    let terminal = |code: ResolutionCode, query: Option<String>, fragment: Option<String>| {
        (unsupported_intent(query, fragment), null_row(code))
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
        );
    }
    if path_part.contains('\\') {
        return Ok(terminal(
            ResolutionCode::BackslashSeparator,
            query,
            fragment,
        ));
    }

    let mut segments: Vec<&str> = path_part.split('/').collect();
    let trailing_slash = segments.len() > 1 && segments.last() == Some(&"");
    if trailing_slash {
        segments.pop();
    }
    if segments.iter().any(|segment| segment.is_empty()) {
        return Ok(terminal(ResolutionCode::InvalidReference, query, fragment));
    }
    let target_kind = if trailing_slash {
        if is_image {
            return Ok(terminal(ResolutionCode::InvalidReference, query, fragment));
        }
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
        let decoded = match decode_segment(segment) {
            Ok(bytes) => bytes,
            Err(defect) => return Ok(terminal(defect_code(defect), query, fragment)),
        };
        match decoded.as_slice() {
            b"." => {}
            b".." => {
                if resolved.pop().is_none() {
                    return Ok(terminal(ResolutionCode::PathTraversal, query, fragment));
                }
            }
            _ => resolved.push(decoded),
        }
    }
    if resolved.is_empty() {
        return Ok(terminal(ResolutionCode::InvalidReference, query, fragment));
    }
    let Some(joined) = RepoPath::from_bytes(resolved.join(&b'/')) else {
        return Ok(terminal(ResolutionCode::InvalidReference, query, fragment));
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
        None,
    )?;
    Ok((intent, row))
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
        None,
    )?;
    Ok((intent, row))
}

/// A resolved directory. A tree target has no content to read: it participates
/// in structural resolution only, so it carries no digest and no availability,
/// which is what lets an index answer for one without a tree identity.
fn tree_row(path: &RepoPath) -> Resolution {
    Resolution {
        code: ResolutionCode::ExactPath,
        path: Some(path.clone()),
        entry_kind: Some(EntryKind::Tree),
        git_mode: Some(GitMode::Tree),
        raw_digest: None,
        projection_digest: None,
        content_availability: ContentAvailability::NotApplicable,
    }
}

/// A resolved regular file, with its content read and digested under the caps.
fn blob_row(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    cache: &mut TargetCache,
    path: &RepoPath,
    mode: GitMode,
    oid: &Oid,
) -> Result<Resolution, Error> {
    let content = read_target(repo, git, scan, cache, path, mode, oid)?;
    let (raw, projection, availability) = match content {
        Content::Ordinary { raw, projection } => {
            (Some(raw), Some(projection), ContentAvailability::Available)
        }
        Content::Pointer { raw } => (Some(raw), None, ContentAvailability::LfsPointerOnly),
    };
    Ok(Resolution {
        code: ResolutionCode::ExactPath,
        path: Some(path.clone()),
        entry_kind: Some(EntryKind::Blob),
        git_mode: Some(mode),
        raw_digest: raw,
        projection_digest: projection,
        content_availability: availability,
    })
}

/// Steps four through ten: exact lookup, special entries, kind compatibility,
/// content availability, query semantics, fragment semantics, and only then
/// `resolved/exact-path`. Compatible entry fields survive the query and
/// fragment boundary rows.
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
    let special = |code: ResolutionCode, entry_kind: EntryKind, mode: GitMode| Resolution {
        code,
        path: Some(path.clone()),
        entry_kind: Some(entry_kind),
        git_mode: Some(mode),
        raw_digest: None,
        projection_digest: None,
        content_availability: ContentAvailability::NotRead,
    };
    let (mode, entry) = match snapshot.locate(path) {
        None => {
            let mut row = null_row(ResolutionCode::PathNotFound);
            row.path = Some(path.clone());
            return Ok(row);
        }
        Some(Located::Entry(GitMode::Symlink, _)) => {
            return Ok(special(
                ResolutionCode::SymlinkEntry,
                EntryKind::Symlink,
                GitMode::Symlink,
            ));
        }
        Some(Located::Entry(GitMode::Gitlink, _)) => {
            return Ok(special(
                ResolutionCode::GitlinkEntry,
                EntryKind::Gitlink,
                GitMode::Gitlink,
            ));
        }
        Some(Located::ImpliedTree | Located::Entry(GitMode::Tree, _)) => {
            (GitMode::Tree, tree_row(path))
        }
        Some(Located::Entry(mode @ (GitMode::RegularFile | GitMode::ExecutableFile), oid)) => {
            (mode, blob_row(repo, git, scan, cache, path, mode, oid)?)
        }
    };

    let is_tree = mode == GitMode::Tree;
    let compatible = match target_kind {
        TargetKind::Blob => !is_tree,
        TargetKind::Tree => is_tree,
        TargetKind::Either => true,
    };
    if !compatible {
        return Ok(Resolution {
            code: ResolutionCode::TargetTypeMismatch,
            ..entry
        });
    }

    if query.is_some() {
        let accepted = !is_tree
            && classify(path.as_bytes())
                .is_some_and(|class| class != crate::Classification::PlainAdvisory)
            && snapshot.is_scanned_structured(path);
        if !accepted {
            return Ok(Resolution {
                code: ResolutionCode::UnsupportedQuerySemantics,
                ..entry
            });
        }
    }

    if let Some(raw_fragment) = fragment
        && !raw_fragment.is_empty()
    {
        let decoded = decode_fragment(raw_fragment).unwrap_or_default();
        let code = if (forge.is_some() && is_tree) || line_fragment(forge, &decoded) {
            ResolutionCode::CodeFragmentUnevaluated
        } else if !is_tree && classify(path.as_bytes()).is_some() {
            ResolutionCode::UnsupportedFragmentSemantics
        } else {
            ResolutionCode::CodeFragmentUnevaluated
        };
        return Ok(Resolution { code, ..entry });
    }
    Ok(entry)
}

/// Line-fragment syntax after one decode, in the dialect's spelling:
/// `L<n>` alone, or the range form the forge renders. First digit nonzero,
/// at most sixteen digits, each number within the safe range, and a range
/// end at least its start. Native references keep the GitHub grammar.
fn line_fragment(forge: Option<ForgeDialect>, decoded: &str) -> bool {
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
    let Some(rest) = decoded.strip_prefix('L') else {
        return false;
    };
    let range = match forge {
        None | Some(ForgeDialect::Github | ForgeDialect::Gitea) => rest.split_once("-L"),
        Some(ForgeDialect::Gitlab) => rest.split_once('-'),
    };
    match range {
        None => number(rest).is_some(),
        Some((start, end)) => match (number(start), number(end)) {
            (Some(from), Some(to)) => to >= from,
            _ => false,
        },
    }
}

/// Reads one referenced regular blob once per distinct path, under the
/// per-target cap and the snapshot aggregate. Pointer content keeps its raw
/// digest and no projection; ordinary content carries both.
fn read_target(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    cache: &mut TargetCache,
    path: &RepoPath,
    mode: GitMode,
    oid: &Oid,
) -> Result<Content, Error> {
    if let Some(content) = cache.read.get(path) {
        return Ok(content.clone());
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
        Content::Pointer { raw }
    } else {
        let projection = hj(
            TARGET_PROJECTION_DOMAIN,
            &Value::Object(vec![
                (
                    "git_mode".to_owned(),
                    Value::String(mode.as_str().to_owned()),
                ),
                ("raw_digest".to_owned(), Value::String(raw.to_string())),
            ]),
        );
        Content::Ordinary { raw, projection }
    };
    cache.read.insert(path.clone(), content.clone());
    Ok(content)
}
