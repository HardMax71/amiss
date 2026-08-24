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
            let resolution = match version {
                ForgeVersion::Candidate => lookup(
                    resolver,
                    &path,
                    target_kind,
                    query.as_deref(),
                    fragment.as_deref(),
                    Some(context.dialect),
                )?,
                ForgeVersion::OtherNamedRef => {
                    Resolution::UnsupportedVersion(VersionScope::KnownPath { path: path.clone() })
                }
                ForgeVersion::Commit(commit_oid) => {
                    Resolution::UnsupportedVersion(VersionScope::KnownCommit {
                        commit_oid,
                        path: path.clone(),
                    })
                }
            };
            Ok((
                Intent {
                    kind: intent_kind,
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
    let literal_ascii = |text: &str| !text.is_empty() && text.is_ascii() && !text.contains('%');
    if !literal_ascii(owner)
        || !literal_ascii(repository)
        || !owner.eq_ignore_ascii_case(&identity.owner)
        || !repository.eq_ignore_ascii_case(&identity.repository)
    {
        return ForgeRoute::Foreign;
    }
    let target_kind = match *form {
        "blob" => TargetKind::Blob,
        "tree" => TargetKind::Tree,
        _ => return ForgeRoute::Foreign,
    };

    let tolerate_terminal_slash = target_kind == TargetKind::Tree;
    match versioned_split(
        identity,
        tolerate_terminal_slash,
        segments.get(3..).unwrap_or_default(),
        TailVersions::NamedOrCommit,
    ) {
        Ok((version, path)) => ForgeRoute::Same(ForgeMatch {
            intent_kind: IntentKind::SameRepositoryGithub,
            target_kind,
            version,
            path,
        }),
        Err(resolution) => ForgeRoute::Unsupported(resolution),
    }
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
    match versioned_split(
        identity,
        target_kind == TargetKind::Tree,
        tail,
        TailVersions::NamedOrCommit,
    ) {
        Ok((version, path)) => ForgeRoute::Same(ForgeMatch {
            intent_kind: IntentKind::SameRepositoryGitlab,
            target_kind,
            version,
            path,
        }),
        Err(resolution) => ForgeRoute::Unsupported(resolution),
    }
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
    let segments: Vec<&str> = suffix.split('/').collect();
    let literal_ascii = |text: &str| !text.is_empty() && text.is_ascii() && !text.contains('%');
    let (Some(owner), Some(project), Some(&"src"), Some(selector)) = (
        segments.first(),
        segments.get(1),
        segments.get(2),
        segments.get(3),
    ) else {
        return ForgeRoute::Foreign;
    };
    if !literal_ascii(owner)
        || !literal_ascii(project)
        || !owner.eq_ignore_ascii_case(&identity.owner)
        || !project.eq_ignore_ascii_case(&identity.repository)
    {
        return ForgeRoute::Foreign;
    }
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
            match decoded_tail(directory_hint, raw_tail) {
                Ok(decoded) => contained_path(&decoded).map(|path| {
                    let version = if identity.candidate_oid.as_ref() == Some(&commit_oid) {
                        ForgeVersion::Candidate
                    } else {
                        ForgeVersion::Commit(commit_oid)
                    };
                    (version, path)
                }),
                Err(resolution) => Err(resolution),
            }
        }
        "tag" => Err(Resolution::UnsupportedVersion(VersionScope::UnknownPath)),
        _ => return ForgeRoute::Foreign,
    };
    match split {
        Ok((version, path)) => ForgeRoute::Same(ForgeMatch {
            intent_kind: IntentKind::SameRepositoryGitea,
            target_kind,
            version,
            path,
        }),
        Err(resolution) => ForgeRoute::Unsupported(resolution),
    }
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
        let version = if identity.candidate_oid.as_ref() == Some(&commit_oid) {
            ForgeVersion::Candidate
        } else {
            ForgeVersion::Commit(commit_oid)
        };
        return Ok((version, path));
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
