use amiss_wire::controls::TargetKind;
use amiss_wire::model::{ForgeDialect, RepoPath};
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::{ExternalReference, InvalidReference, VersionScope};

use crate::Error;

use super::{
    ForgeContext, Intent, Resolution, Resolver, decode_segment, lookup, unsupported_intent,
};

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
            let resolution = matched
                .candidate
                .then(|| {
                    lookup(
                        resolver,
                        &matched.path,
                        matched.target_kind,
                        query.as_deref(),
                        fragment.as_deref(),
                        Some(context.dialect),
                    )
                })
                .transpose()?
                .unwrap_or_else(|| {
                    Resolution::UnsupportedVersion(VersionScope::KnownPath {
                        path: matched.path.clone(),
                    })
                });
            Ok((
                Intent {
                    kind: matched.intent_kind,
                    repository_path: Some(matched.path),
                    target_kind: Some(matched.target_kind),
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
    candidate: bool,
    path: RepoPath,
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
        || owner.to_ascii_lowercase() != identity.owner
        || repository.to_ascii_lowercase() != identity.repository
    {
        return ForgeRoute::Foreign;
    }
    let target_kind = match *form {
        "blob" => TargetKind::Blob,
        "tree" => TargetKind::Tree,
        _ => return ForgeRoute::Foreign,
    };

    let tolerate_terminal_slash = target_kind == TargetKind::Tree;
    match trusted_split(
        identity,
        tolerate_terminal_slash,
        segments.get(3..).unwrap_or_default(),
    ) {
        Ok((candidate, path)) => ForgeRoute::Same(ForgeMatch {
            intent_kind: IntentKind::SameRepositoryGithub,
            target_kind,
            candidate,
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
    let identity_segments: Vec<&str> = identity.owner.split('/').collect();
    let owner_matches = owner_segments.len() == identity_segments.len()
        && owner_segments
            .iter()
            .zip(&identity_segments)
            .all(|(url, own)| literal_ascii(url) && url.to_ascii_lowercase() == **own);
    let project = segments.get(name_at).copied().unwrap_or_default();
    if !owner_matches
        || !literal_ascii(project)
        || project.to_ascii_lowercase() != identity.repository
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
    match trusted_split(identity, target_kind == TargetKind::Tree, tail) {
        Ok((candidate, path)) => ForgeRoute::Same(ForgeMatch {
            intent_kind: IntentKind::SameRepositoryGitlab,
            target_kind,
            candidate,
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
        || owner.to_ascii_lowercase() != identity.owner
        || project.to_ascii_lowercase() != identity.repository
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
            trusted_split(identity, directory_hint, branch_tail)
        }
        "commit" => {
            let pinned = segments.get(4).copied().unwrap_or_default();
            if !oid_shaped(pinned) {
                return ForgeRoute::Foreign;
            }
            match decoded_tail(directory_hint, raw_tail) {
                Ok(decoded) => contained_path(&decoded)
                    .map(|path| (identity.candidate_oid.as_deref() == Some(pinned), path)),
                Err(resolution) => Err(resolution),
            }
        }
        "tag" => Err(Resolution::UnsupportedVersion(VersionScope::UnknownPath)),
        _ => return ForgeRoute::Foreign,
    };
    match split {
        Ok((candidate, path)) => ForgeRoute::Same(ForgeMatch {
            intent_kind: IntentKind::SameRepositoryGitea,
            target_kind,
            candidate,
            path,
        }),
        Err(resolution) => ForgeRoute::Unsupported(resolution),
    }
}

/// A full lowercase object id in either frozen format; anything else after
/// `src/commit/` is not a spelling the forge emits.
fn oid_shaped(segment: &str) -> bool {
    matches!(segment.len(), 40 | 64)
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Decodes the suffix after the form segment, removes a lone terminal empty
/// segment where the dialect's form tolerates one, matches the two trusted
/// refs by whole segments, and validates the remaining path before deciding
/// candidate or default.
fn trusted_split(
    identity: &ForgeContext,
    tolerate_terminal_slash: bool,
    raw_tail: &[&str],
) -> Result<(bool, RepoPath), Resolution> {
    let decoded = decoded_tail(tolerate_terminal_slash, raw_tail)?;

    let candidate = ref_segments(&identity.candidate_ref);
    let default = ref_segments(&identity.default_ref);
    let candidate_split = split_after(&decoded, &candidate);
    let default_split = split_after(&decoded, &default);
    let (matched_candidate, remaining) = match (candidate_split, default_split) {
        (Some(after_candidate), Some(_after_default)) => {
            if candidate == default {
                (true, after_candidate)
            } else {
                return Err(Resolution::UnsupportedVersion(VersionScope::UnknownPath));
            }
        }
        (Some(after), None) => (true, after),
        (None, Some(after)) => (false, after),
        (None, None) => {
            return Err(Resolution::UnsupportedVersion(VersionScope::UnknownPath));
        }
    };
    Ok((matched_candidate, contained_path(&remaining)?))
}

/// One decode per segment, empties refused, a lone terminal empty segment
/// dropped where the dialect's form tolerates a directory-hint slash.
fn decoded_tail(
    tolerate_terminal_slash: bool,
    raw_tail: &[&str],
) -> Result<Vec<Vec<u8>>, Resolution> {
    let mut tail: Vec<&str> = raw_tail.to_vec();
    if tolerate_terminal_slash && tail.len() > 1 && tail.last() == Some(&"") {
        tail.pop();
    }
    let mut decoded: Vec<Vec<u8>> = Vec::new();
    for segment in &tail {
        if segment.is_empty() {
            return Err(Resolution::Invalid(InvalidReference::Syntax));
        }
        decoded.push(decode_segment(segment)?);
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

fn ref_segments(full_ref: &str) -> Vec<Vec<u8>> {
    full_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(full_ref)
        .split('/')
        .map(|segment| segment.as_bytes().to_vec())
        .collect()
}

fn split_after(decoded: &[Vec<u8>], reference: &[Vec<u8>]) -> Option<Vec<Vec<u8>>> {
    if decoded.len() < reference.len() {
        return None;
    }
    let (head, tail) = decoded.split_at(reference.len());
    (head == reference).then(|| tail.to_vec())
}
