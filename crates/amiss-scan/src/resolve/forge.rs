use amiss_wire::controls::TargetKind;
use amiss_wire::model::{ForgeDialect, ObjectFormat, Oid, RepoPath};
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::{ExternalReference, InvalidReference, VersionScope};
use amiss_wire::uri::decode_component;

use crate::Error;

use super::syntax::{invalid_path_byte, unsupported_intent};
use super::{ForgeContext, Intent, Resolution, Resolver, lookup};

pub(super) fn resolve(
    resolver: &mut Resolver<'_>,
    context: &ForgeContext,
    suffix: &str,
    query: Option<String>,
    fragment: Option<String>,
) -> Result<(Intent, Resolution), Error> {
    let route = match context.dialect {
        ForgeDialect::Github => github(context, suffix),
        ForgeDialect::Gitlab => gitlab(context, suffix),
        ForgeDialect::Gitea => gitea(context, suffix),
        ForgeDialect::BitbucketCloud => bitbucket_cloud(context, suffix),
        ForgeDialect::BitbucketDataCenter => {
            bitbucket_data_center(context, suffix, query.as_deref())
        }
    };
    match route {
        ForgeRoute::Foreign => Ok(foreign_row(query, fragment)),
        ForgeRoute::Unsupported(resolution) => {
            Ok((unsupported_intent(query, fragment), resolution))
        }
        ForgeRoute::Same(matched) => {
            let ForgeMatch {
                intent_kind,
                target_kind,
                version,
                path,
            } = matched;
            let query_is_forge_state = context.dialect == ForgeDialect::BitbucketDataCenter
                || context.dialect == ForgeDialect::BitbucketCloud
                    && query.as_deref() == Some("fileviewer=file-view-default");
            let lookup_query = query.as_deref().filter(|_value| !query_is_forge_state);
            let (commit_oid, resolution) = match version {
                ForgeVersion::Candidate => (
                    None,
                    lookup(
                        resolver,
                        &path,
                        target_kind,
                        lookup_query,
                        fragment.as_deref(),
                        Some(context.dialect),
                    )?,
                ),
                ForgeVersion::OtherNamedRef => (
                    None,
                    Resolution::UnsupportedVersion(VersionScope::KnownPath { path: path.clone() }),
                ),
                ForgeVersion::Commit(oid) => {
                    let resolution = super::history::lookup(
                        resolver,
                        &oid,
                        &path,
                        target_kind,
                        lookup_query,
                        fragment.as_deref(),
                        context.dialect,
                    )?
                    .unwrap_or_else(|| {
                        Resolution::UnsupportedVersion(VersionScope::KnownCommit {
                            commit_oid: oid.clone(),
                            path: path.clone(),
                        })
                    });
                    (Some(oid), resolution)
                }
            };
            Ok((
                Intent {
                    kind: intent_kind,
                    commit_oid,
                    repository_path: Some(path),
                    target_kind: Some(target_kind),
                    external_scheme: None,
                    query,
                    fragment,
                },
                resolution,
            ))
        }
    }
}

enum ForgeRoute {
    Foreign,
    Unsupported(Resolution),
    Same(ForgeMatch),
}

struct ForgeMatch {
    intent_kind: IntentKind,
    target_kind: TargetKind,
    version: ForgeVersion,
    path: RepoPath,
}

enum ForgeVersion {
    Candidate,
    OtherNamedRef,
    Commit(Oid),
}

#[derive(Clone, Copy)]
enum TailVersions {
    Named,
    NamedOrCommit,
}

/// A recognized URL that is not this repository: a valid external HTTPS
/// destination whose repository is someone else's.
fn foreign_row(query: Option<String>, fragment: Option<String>) -> (Intent, Resolution) {
    (
        Intent {
            kind: IntentKind::ExternalUrl,
            commit_oid: None,
            repository_path: None,
            target_kind: None,
            external_scheme: Some("https".to_owned()),
            query,
            fragment,
        },
        Resolution::External(ExternalReference::ForeignRepository),
    )
}

/// Foreign unless proven trusted: exact accepted `blob`/`tree` forms, literal
/// ASCII owner and repository folded only `A`-`Z`, each later segment decoded
/// exactly once, the trusted refs matched by whole segments, and the
/// remaining path validated before the candidate-or-default decision.
fn github(identity: &ForgeContext, suffix: &str) -> ForgeRoute {
    let segments: Vec<&str> = suffix.split('/').collect();
    let (Some(owner), Some(repository), Some(form)) =
        (segments.first(), segments.get(1), segments.get(2))
    else {
        return ForgeRoute::Foreign;
    };
    if !repository_pair_matches(identity, owner, repository) {
        return ForgeRoute::Foreign;
    }
    let target_kind = match *form {
        "blob" => TargetKind::Blob,
        "tree" => TargetKind::Tree,
        _ => return ForgeRoute::Foreign,
    };

    let tolerate_terminal_slash = target_kind == TargetKind::Tree;
    same_route(
        IntentKind::SameRepositoryGithub,
        target_kind,
        versioned_split(
            identity,
            tolerate_terminal_slash,
            segments.get(3..).unwrap_or_default(),
            TailVersions::NamedOrCommit,
        ),
    )
}

/// GitLab's canonical form: every segment before the reserved `-` separator
/// names the project (nested group segments, then the name), the form
/// follows the separator, and the ref/path tail splits exactly like
/// GitHub's. No owner segment or name may be a bare `-`, so the first `-`
/// at index two or later is the separator or the URL is nobody's; anything
/// without one, including the legacy pre-separator form and `/-/raw/`, is
/// foreign.
fn gitlab(identity: &ForgeContext, suffix: &str) -> ForgeRoute {
    let segments: Vec<&str> = suffix.split('/').collect();
    let literal_ascii = |text: &str| !text.is_empty() && text.is_ascii() && !text.contains('%');
    let Some(separator) = segments.iter().position(|segment| *segment == "-") else {
        return ForgeRoute::Foreign;
    };
    if separator < 2 {
        return ForgeRoute::Foreign;
    }
    let name_at = separator.saturating_sub(1);
    let owner_segments = segments.get(..name_at).unwrap_or_default();
    let identity_segments = identity.owner.split('/');
    let owner_matches = owner_segments.len() == identity_segments.clone().count()
        && owner_segments
            .iter()
            .zip(identity_segments)
            .all(|(url, own)| literal_ascii(url) && url.eq_ignore_ascii_case(own));
    let project = segments.get(name_at).copied().unwrap_or_default();
    if !owner_matches
        || !literal_ascii(project)
        || !project.eq_ignore_ascii_case(&identity.repository)
    {
        return ForgeRoute::Foreign;
    }
    let target_kind = match segments.get(separator.saturating_add(1)) {
        Some(&"blob") => TargetKind::Blob,
        Some(&"tree") => TargetKind::Tree,
        Some(_) | None => return ForgeRoute::Foreign,
    };

    let tail = segments
        .get(separator.saturating_add(2)..)
        .unwrap_or_default();
    same_route(
        IntentKind::SameRepositoryGitlab,
        target_kind,
        versioned_split(
            identity,
            target_kind == TargetKind::Tree,
            tail,
            TailVersions::NamedOrCommit,
        ),
    )
}

/// The gitea family's typed forms, shared by Gitea, Forgejo, and Codeberg:
/// `owner/name/src/branch/<branch...>/<path...>` splits through the trusted
/// refs, `src/commit/<oid>/<path...>` resolves exactly when the full
/// lowercase OID is the candidate commit and is version-scoped out
/// otherwise, and `src/tag/...` is always version-scoped out because no tag
/// is trusted. The form has no blob or tree axis, so the target kind is
/// `either`, or `tree` under a directory-hint slash. The untyped legacy
/// `src/<ref>/` form and every other selector are foreign: only the spellings
/// the forge's own browser emits are pinned.
fn gitea(identity: &ForgeContext, suffix: &str) -> ForgeRoute {
    let Some(segments) = source_segments(identity, suffix) else {
        return ForgeRoute::Foreign;
    };
    let Some(selector) = segments.get(3) else {
        return ForgeRoute::Foreign;
    };
    let raw_tail = segments.get(5..).unwrap_or_default();
    let directory_hint = raw_tail.len() > 1 && raw_tail.last() == Some(&"");
    let target_kind = if directory_hint {
        TargetKind::Tree
    } else {
        TargetKind::Either
    };
    let split = match *selector {
        "branch" => {
            let branch_tail = segments.get(4..).unwrap_or_default();
            versioned_split(identity, directory_hint, branch_tail, TailVersions::Named)
        }
        "commit" => {
            let pinned = segments.get(4).copied().unwrap_or_default();
            let Some(commit_oid) = [ObjectFormat::Sha1, ObjectFormat::Sha256]
                .into_iter()
                .find_map(|format| Oid::new(format, pinned.to_owned()))
            else {
                return ForgeRoute::Foreign;
            };
            if commit_oid.object_format() != identity.object_format {
                return ForgeRoute::Unsupported(Resolution::UnsupportedVersion(
                    VersionScope::UnknownPath,
                ));
            }
            decoded_tail(directory_hint, raw_tail).and_then(|decoded| {
                contained_path(&decoded).map(|path| (ForgeVersion::Commit(commit_oid), path))
            })
        }
        "tag" => Err(Resolution::UnsupportedVersion(VersionScope::UnknownPath)),
        _ => return ForgeRoute::Foreign,
    };
    same_route(IntentKind::SameRepositoryGitea, target_kind, split)
}

/// Bitbucket Cloud's source form has one commitish segment, so the path split
/// is known even when that segment names an untrusted branch or tag.
fn bitbucket_cloud(identity: &ForgeContext, suffix: &str) -> ForgeRoute {
    let Some(segments) = source_segments(identity, suffix) else {
        return ForgeRoute::Foreign;
    };

    let raw_tail = segments.get(3..).unwrap_or_default();
    let directory_hint = raw_tail.len() > 2 && raw_tail.last() == Some(&"");
    let target_kind = if directory_hint {
        TargetKind::Tree
    } else {
        TargetKind::Either
    };
    same_route(
        IntentKind::SameRepositoryBitbucketCloud,
        target_kind,
        bitbucket_cloud_split(identity, directory_hint, raw_tail),
    )
}

fn bitbucket_cloud_split(
    identity: &ForgeContext,
    directory_hint: bool,
    raw_tail: &[&str],
) -> Result<(ForgeVersion, RepoPath), Resolution> {
    let decoded = decoded_tail(directory_hint, raw_tail)?;
    let version = decoded
        .first()
        .ok_or(Resolution::Invalid(InvalidReference::Syntax))?;
    let candidate = identity
        .candidate_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(identity.candidate_ref.as_str())
        .as_bytes();
    let default = identity
        .default_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(identity.default_ref.as_str())
        .as_bytes();
    let candidate_matches = version.as_slice() == candidate;
    let default_matches = version.as_slice() == default;
    let commit_oid = decoded_oid(version);
    if commit_oid
        .as_ref()
        .is_some_and(|oid| oid.object_format() != identity.object_format)
        || commit_oid.is_some() && (candidate_matches || default_matches)
    {
        return Err(Resolution::UnsupportedVersion(VersionScope::UnknownPath));
    }
    let literal_oid = raw_tail
        .first()
        .is_some_and(|raw| raw.as_bytes() == version.as_slice());
    let version = match commit_oid.filter(|_oid| literal_oid) {
        Some(oid) => ForgeVersion::Commit(oid),
        None if candidate_matches => ForgeVersion::Candidate,
        None => ForgeVersion::OtherNamedRef,
    };
    Ok((
        version,
        contained_path(decoded.get(1..).unwrap_or_default())?,
    ))
}

/// Data Center binds a project or personal repository in the path and the
/// selected revision in `at`, while the emitted history link uses the exact
/// `until` and `untilPath` pair. An installation context may precede either
/// repository route.
fn bitbucket_data_center(identity: &ForgeContext, suffix: &str, query: Option<&str>) -> ForgeRoute {
    let segments: Vec<&str> = suffix.split('/').collect();
    let literal_ascii = |text: &str| {
        !text.is_empty() && text.is_ascii() && !text.contains('%') && !matches!(text, "." | "..")
    };
    let Some(marker) = segments
        .iter()
        .position(|segment| matches!(*segment, "projects" | "users"))
    else {
        return ForgeRoute::Foreign;
    };
    let [scope, owner, "repos", repository, "browse"] = segments
        .get(marker..marker.saturating_add(5))
        .unwrap_or_default()
    else {
        return ForgeRoute::Foreign;
    };
    let route_owner = if *scope == "projects" {
        owner.strip_prefix('~').unwrap_or(owner)
    } else {
        owner
    };
    if segments
        .get(..marker)
        .is_some_and(|prefix| prefix.iter().any(|segment| !literal_ascii(segment)))
        || !repository_pair_matches(identity, route_owner, repository)
    {
        return ForgeRoute::Foreign;
    }
    let raw_tail = segments.get(marker.saturating_add(5)..).unwrap_or_default();
    let directory_hint = raw_tail.len() > 1 && raw_tail.last() == Some(&"");
    let target_kind = if directory_hint {
        TargetKind::Tree
    } else {
        TargetKind::Either
    };
    let split = decoded_tail(directory_hint, raw_tail)
        .and_then(|decoded| contained_path(&decoded))
        .and_then(|path| {
            bitbucket_data_center_version(identity, query, &path).map(|version| (version, path))
        });
    same_route(
        IntentKind::SameRepositoryBitbucketDataCenter,
        target_kind,
        split,
    )
}

fn bitbucket_data_center_version(
    identity: &ForgeContext,
    query: Option<&str>,
    path: &RepoPath,
) -> Result<ForgeVersion, Resolution> {
    let unsupported =
        || Resolution::UnsupportedVersion(VersionScope::KnownPath { path: path.clone() });
    let decode = |value: &str| {
        let mut decoded = Vec::with_capacity(value.len());
        decode_component(value, &mut decoded, |byte| match byte {
            b'\\' => Some(InvalidReference::BackslashSeparator),
            0..=0x1f | 0x7f => Some(InvalidReference::DecodedPathControl),
            _ => None,
        })
        .map(|()| decoded)
        .map_err(Resolution::Invalid)
    };
    let Some(query) = query else {
        return Ok(if identity.default_ref == identity.candidate_ref {
            ForgeVersion::Candidate
        } else {
            ForgeVersion::OtherNamedRef
        });
    };
    if let Some(raw_revision) = query.strip_prefix("at=")
        && !raw_revision.contains('&')
    {
        let revision = decode(raw_revision)?;
        let oid = decoded_oid(&revision);
        if oid
            .as_ref()
            .is_some_and(|value| value.object_format() != identity.object_format)
        {
            return Err(unsupported());
        }
        if let Some(oid) = oid.filter(|_value| raw_revision.as_bytes() == revision.as_slice()) {
            return Ok(ForgeVersion::Commit(oid));
        }
        if revision == identity.candidate_ref.as_bytes() {
            return Ok(ForgeVersion::Candidate);
        }
        if revision
            .strip_prefix(b"refs/heads/")
            .is_some_and(|name| !name.is_empty())
            || revision
                .strip_prefix(b"refs/tags/")
                .is_some_and(|name| !name.is_empty())
        {
            return Ok(ForgeVersion::OtherNamedRef);
        }
        return Err(unsupported());
    }
    if let Some(history) = query.strip_prefix("until=")
        && let Some((raw_revision, raw_path)) = history.split_once("&untilPath=")
        && !raw_path.contains('&')
    {
        let decoded_path = decode(raw_path)?;
        let revision = decoded_oid(raw_revision.as_bytes())
            .filter(|value| value.object_format() == identity.object_format);
        if decoded_path == path.as_bytes()
            && let Some(oid) = revision
        {
            return Ok(ForgeVersion::Commit(oid));
        }
    }
    Err(unsupported())
}

fn decoded_oid(value: &[u8]) -> Option<Oid> {
    let format = match value.len() {
        40 => ObjectFormat::Sha1,
        64 => ObjectFormat::Sha256,
        _ => return None,
    };
    Oid::new(format, std::str::from_utf8(value).ok()?.to_owned())
}

fn repository_pair_matches(identity: &ForgeContext, owner: &str, repository: &str) -> bool {
    [owner, repository]
        .iter()
        .all(|text| !text.is_empty() && text.is_ascii() && !text.contains('%'))
        && owner.eq_ignore_ascii_case(&identity.owner)
        && repository.eq_ignore_ascii_case(&identity.repository)
}

fn source_segments<'a>(identity: &ForgeContext, suffix: &'a str) -> Option<Vec<&'a str>> {
    let segments: Vec<&str> = suffix.split('/').collect();
    let (Some(owner), Some(repository), Some(&"src")) =
        (segments.first(), segments.get(1), segments.get(2))
    else {
        return None;
    };
    repository_pair_matches(identity, owner, repository).then_some(segments)
}

fn same_route(
    intent_kind: IntentKind,
    target_kind: TargetKind,
    split: Result<(ForgeVersion, RepoPath), Resolution>,
) -> ForgeRoute {
    split.map_or_else(ForgeRoute::Unsupported, |(version, path)| {
        ForgeRoute::Same(ForgeMatch {
            intent_kind,
            target_kind,
            version,
            path,
        })
    })
}

/// Splits a decoded tail through the two trusted refs and, where the forge
/// form permits it, one literal full object ID. Ref/ID ambiguity is refused.
fn versioned_split(
    identity: &ForgeContext,
    tolerate_terminal_slash: bool,
    raw_tail: &[&str],
    versions: TailVersions,
) -> Result<(ForgeVersion, RepoPath), Resolution> {
    let decoded = decoded_tail(tolerate_terminal_slash, raw_tail)?;
    let candidate = identity
        .candidate_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(identity.candidate_ref.as_str());
    let default = identity
        .default_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(identity.default_ref.as_str());
    let candidate_split = split_after(&decoded, candidate);
    let default_split = split_after(&decoded, default);
    let oid_length = match identity.object_format {
        ObjectFormat::Sha1 => 40,
        ObjectFormat::Sha256 => 64,
    };
    let decoded_oid = decoded
        .first()
        .filter(|_segment| matches!(versions, TailVersions::NamedOrCommit))
        .filter(|segment| segment.len() == oid_length)
        .and_then(|segment| std::str::from_utf8(segment).ok())
        .and_then(|segment| Oid::new(identity.object_format, segment.to_owned()));

    if decoded_oid.is_some() && (candidate_split.is_some() || default_split.is_some()) {
        return Err(Resolution::UnsupportedVersion(VersionScope::UnknownPath));
    }
    let literal_oid = raw_tail
        .first()
        .zip(decoded.first())
        .is_some_and(|(raw, value)| raw.as_bytes() == value.as_slice());
    if let Some(commit_oid) = decoded_oid.filter(|_oid| literal_oid) {
        let path = contained_path(decoded.get(1..).unwrap_or_default())?;
        return Ok((ForgeVersion::Commit(commit_oid), path));
    }
    match (candidate_split, default_split) {
        (Some(after_candidate), Some(_after_default)) if candidate == default => {
            Ok((ForgeVersion::Candidate, contained_path(after_candidate)?))
        }
        (Some(after), None) => Ok((ForgeVersion::Candidate, contained_path(after)?)),
        (None, Some(after)) => Ok((ForgeVersion::OtherNamedRef, contained_path(after)?)),
        (Some(_), Some(_)) | (None, None) => {
            Err(Resolution::UnsupportedVersion(VersionScope::UnknownPath))
        }
    }
}

/// One decode per segment, empties refused, a lone terminal empty segment
/// dropped where the dialect's form tolerates a directory-hint slash.
fn decoded_tail(
    tolerate_terminal_slash: bool,
    raw_tail: &[&str],
) -> Result<Vec<Vec<u8>>, Resolution> {
    let tail = if tolerate_terminal_slash && raw_tail.len() > 1 && raw_tail.last() == Some(&"") {
        raw_tail
            .get(..raw_tail.len().saturating_sub(1))
            .unwrap_or_default()
    } else {
        raw_tail
    };
    let mut decoded: Vec<Vec<u8>> = Vec::with_capacity(tail.len());
    for segment in tail {
        if segment.is_empty() {
            return Err(Resolution::Invalid(InvalidReference::Syntax));
        }
        let mut bytes = Vec::with_capacity(segment.len());
        decode_component(segment, &mut bytes, invalid_path_byte).map_err(Resolution::Invalid)?;
        decoded.push(bytes);
    }
    Ok(decoded)
}

/// The remaining segments as a contained repository path: nonempty, no dot
/// segments, and inside the frozen byte grammar.
fn contained_path(remaining: &[Vec<u8>]) -> Result<RepoPath, Resolution> {
    if remaining.is_empty() {
        return Err(Resolution::Invalid(InvalidReference::Syntax));
    }
    if remaining
        .iter()
        .any(|segment| segment == b"." || segment == b"..")
    {
        return Err(Resolution::Invalid(InvalidReference::PathTraversal));
    }
    RepoPath::from_bytes(remaining.join(&b'/')).ok_or(Resolution::Invalid(InvalidReference::Syntax))
}

fn split_after<'a>(decoded: &'a [Vec<u8>], reference: &str) -> Option<&'a [Vec<u8>]> {
    let mut consumed = 0_usize;
    for expected in reference.split('/') {
        if decoded.get(consumed).map(Vec::as_slice) != Some(expected.as_bytes()) {
            return None;
        }
        consumed = consumed.saturating_add(1);
    }
    decoded.get(consumed..)
}
