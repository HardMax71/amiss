use amiss_git::{GitResources, Repository};
use amiss_wire::controls::TargetKind;
use amiss_wire::model::{ForgeDialect, RepoPath};
use amiss_wire::report::{IntentKind, ResolutionCode};

use crate::Error;
use crate::discovery::SnapshotDiscovery;
use crate::resources::ScanResources;

use super::{
    ForgeContext, Intent, Resolution, TargetCache, decode_segment, defect_code, lookup, null_row,
    unsupported_intent,
};

#[expect(
    clippy::too_many_arguments,
    reason = "the resolver context is the contract's"
)]
pub(super) fn resolve(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    cache: &mut TargetCache,
    snapshot: &SnapshotDiscovery,
    context: &ForgeContext,
    suffix: &str,
    query: Option<String>,
    fragment: Option<String>,
) -> Result<(Intent, Resolution), Error> {
    match context.dialect {
        ForgeDialect::Github => github(
            repo, git, scan, cache, snapshot, context, suffix, query, fragment,
        ),
        ForgeDialect::Gitlab => gitlab(
            repo, git, scan, cache, snapshot, context, suffix, query, fragment,
        ),
        ForgeDialect::Gitea => gitea(
            repo, git, scan, cache, snapshot, context, suffix, query, fragment,
        ),
    }
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
        null_row(ResolutionCode::ForeignRepository),
    )
}

/// Foreign unless proven trusted: exact accepted `blob`/`tree` forms, literal
/// ASCII owner and repository folded only `A`-`Z`, each later segment decoded
/// exactly once, the trusted refs matched by whole segments, and the
/// remaining path validated before the candidate-or-default decision.
#[expect(
    clippy::too_many_arguments,
    reason = "the resolver context is the contract's"
)]
fn github(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    cache: &mut TargetCache,
    snapshot: &SnapshotDiscovery,
    identity: &ForgeContext,
    suffix: &str,
    query: Option<String>,
    fragment: Option<String>,
) -> Result<(Intent, Resolution), Error> {
    let foreign = foreign_row;
    let segments: Vec<&str> = suffix.split('/').collect();
    let (Some(owner), Some(repository), Some(form)) =
        (segments.first(), segments.get(1), segments.get(2))
    else {
        return Ok(foreign(query, fragment));
    };
    let literal_ascii = |text: &str| !text.is_empty() && text.is_ascii() && !text.contains('%');
    if !literal_ascii(owner)
        || !literal_ascii(repository)
        || owner.to_ascii_lowercase() != identity.owner
        || repository.to_ascii_lowercase() != identity.repository
    {
        return Ok(foreign(query, fragment));
    }
    let target_kind = match *form {
        "blob" => TargetKind::Blob,
        "tree" => TargetKind::Tree,
        _ => return Ok(foreign(query, fragment)),
    };

    let tolerate_terminal_slash = target_kind == TargetKind::Tree;
    let (matched_candidate, joined) = match trusted_split(
        identity,
        tolerate_terminal_slash,
        segments.get(3..).unwrap_or_default(),
    ) {
        Ok(split) => split,
        Err(code) => {
            return Ok((unsupported_intent(query, fragment), null_row(code)));
        }
    };

    let intent = Intent {
        kind: IntentKind::SameRepositoryGithub,
        repository_path: Some(joined.clone()),
        target_kind: Some(target_kind),
        external_scheme: None,
        query: query.clone(),
        fragment: fragment.clone(),
    };
    if !matched_candidate {
        let mut row = null_row(ResolutionCode::UnsupportedVersionScope);
        row.path = Some(joined);
        return Ok((intent, row));
    }
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
        Some(identity.dialect),
    )?;
    Ok((intent, row))
}

/// GitLab's canonical form: every segment before the reserved `-` separator
/// names the project (nested group segments, then the name), the form
/// follows the separator, and the ref/path tail splits exactly like
/// GitHub's. No owner segment or name may be a bare `-`, so the first `-`
/// at index two or later is the separator or the URL is nobody's; anything
/// without one, including the legacy pre-separator form and `/-/raw/`, is
/// foreign.
#[expect(
    clippy::too_many_arguments,
    reason = "the resolver context is the contract's"
)]
fn gitlab(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    cache: &mut TargetCache,
    snapshot: &SnapshotDiscovery,
    identity: &ForgeContext,
    suffix: &str,
    query: Option<String>,
    fragment: Option<String>,
) -> Result<(Intent, Resolution), Error> {
    let segments: Vec<&str> = suffix.split('/').collect();
    let literal_ascii = |text: &str| !text.is_empty() && text.is_ascii() && !text.contains('%');
    let Some(separator) = segments.iter().position(|segment| *segment == "-") else {
        return Ok(foreign_row(query, fragment));
    };
    if separator < 2 {
        return Ok(foreign_row(query, fragment));
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
        return Ok(foreign_row(query, fragment));
    }
    let target_kind = match segments.get(separator.saturating_add(1)) {
        Some(&"blob") => TargetKind::Blob,
        Some(&"tree") => TargetKind::Tree,
        Some(_) | None => return Ok(foreign_row(query, fragment)),
    };

    let tail = segments
        .get(separator.saturating_add(2)..)
        .unwrap_or_default();
    let (matched_candidate, joined) =
        match trusted_split(identity, target_kind == TargetKind::Tree, tail) {
            Ok(split) => split,
            Err(code) => {
                return Ok((unsupported_intent(query, fragment), null_row(code)));
            }
        };

    let intent = Intent {
        kind: IntentKind::SameRepositoryGitlab,
        repository_path: Some(joined.clone()),
        target_kind: Some(target_kind),
        external_scheme: None,
        query: query.clone(),
        fragment: fragment.clone(),
    };
    if !matched_candidate {
        let mut row = null_row(ResolutionCode::UnsupportedVersionScope);
        row.path = Some(joined);
        return Ok((intent, row));
    }
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
        Some(ForgeDialect::Gitlab),
    )?;
    Ok((intent, row))
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
#[expect(
    clippy::too_many_arguments,
    reason = "the resolver context is the contract's"
)]
fn gitea(
    repo: &Repository,
    git: &mut GitResources,
    scan: &mut ScanResources,
    cache: &mut TargetCache,
    snapshot: &SnapshotDiscovery,
    identity: &ForgeContext,
    suffix: &str,
    query: Option<String>,
    fragment: Option<String>,
) -> Result<(Intent, Resolution), Error> {
    let segments: Vec<&str> = suffix.split('/').collect();
    let literal_ascii = |text: &str| !text.is_empty() && text.is_ascii() && !text.contains('%');
    let (Some(owner), Some(project), Some(&"src"), Some(selector)) = (
        segments.first(),
        segments.get(1),
        segments.get(2),
        segments.get(3),
    ) else {
        return Ok(foreign_row(query, fragment));
    };
    if !literal_ascii(owner)
        || !literal_ascii(project)
        || owner.to_ascii_lowercase() != identity.owner
        || project.to_ascii_lowercase() != identity.repository
    {
        return Ok(foreign_row(query, fragment));
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
                return Ok(foreign_row(query, fragment));
            }
            match decoded_tail(directory_hint, raw_tail) {
                Ok(decoded) => contained_path(&decoded)
                    .map(|path| (identity.candidate_oid.as_deref() == Some(pinned), path)),
                Err(code) => Err(code),
            }
        }
        "tag" => Err(ResolutionCode::UnsupportedVersionScope),
        _ => return Ok(foreign_row(query, fragment)),
    };
    let (matched_candidate, joined) = match split {
        Ok(split) => split,
        Err(code) => {
            return Ok((unsupported_intent(query, fragment), null_row(code)));
        }
    };

    let intent = Intent {
        kind: IntentKind::SameRepositoryGitea,
        repository_path: Some(joined.clone()),
        target_kind: Some(target_kind),
        external_scheme: None,
        query: query.clone(),
        fragment: fragment.clone(),
    };
    if !matched_candidate {
        let mut row = null_row(ResolutionCode::UnsupportedVersionScope);
        row.path = Some(joined);
        return Ok((intent, row));
    }
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
        Some(ForgeDialect::Gitea),
    )?;
    Ok((intent, row))
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
) -> Result<(bool, RepoPath), ResolutionCode> {
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
                return Err(ResolutionCode::UnsupportedVersionScope);
            }
        }
        (Some(after), None) => (true, after),
        (None, Some(after)) => (false, after),
        (None, None) => return Err(ResolutionCode::UnsupportedVersionScope),
    };
    Ok((matched_candidate, contained_path(&remaining)?))
}

/// One decode per segment, empties refused, a lone terminal empty segment
/// dropped where the dialect's form tolerates a directory-hint slash.
fn decoded_tail(
    tolerate_terminal_slash: bool,
    raw_tail: &[&str],
) -> Result<Vec<Vec<u8>>, ResolutionCode> {
    let mut tail: Vec<&str> = raw_tail.to_vec();
    if tolerate_terminal_slash && tail.len() > 1 && tail.last() == Some(&"") {
        tail.pop();
    }
    let mut decoded: Vec<Vec<u8>> = Vec::new();
    for segment in &tail {
        if segment.is_empty() {
            return Err(ResolutionCode::InvalidReference);
        }
        decoded.push(decode_segment(segment).map_err(defect_code)?);
    }
    Ok(decoded)
}

/// The remaining segments as a contained repository path: nonempty, no dot
/// segments, and inside the frozen byte grammar.
fn contained_path(remaining: &[Vec<u8>]) -> Result<RepoPath, ResolutionCode> {
    if remaining.is_empty() {
        return Err(ResolutionCode::InvalidReference);
    }
    if remaining
        .iter()
        .any(|segment| segment == b"." || segment == b"..")
    {
        return Err(ResolutionCode::PathTraversal);
    }
    RepoPath::from_bytes(remaining.join(&b'/')).ok_or(ResolutionCode::InvalidReference)
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
