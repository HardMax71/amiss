mod tests;

use amiss_controller::ProviderError;
use amiss_wire::external::{bound_plan, evidence_file, forge_evidence_row};
use amiss_wire::json::Value;

use super::rest::{GiteaVerification, Presence, RefFamily};

pub(super) const PRODUCER_NAME: &str = "amiss-controller-gitea";

/// Verifies the plan's introduced destinations shaped for this host through
/// the forge API and returns the evidence file. Facts only, never verdicts:
/// an answer the API refused omits the field it would have filled, and a
/// standing unavailability ends the walk early with the rows already
/// learned, since a skipped destination stays unproven downstream.
pub(super) fn verify_external<R: GiteaVerification>(
    rest: &R,
    plan: &Value,
    host: &str,
    producer_version: &str,
    checked_at: &str,
) -> Result<Value, ProviderError> {
    let introduced = plan
        .member("payload")
        .and_then(|payload| payload.member("introduced"));
    // A value that is not a digest-whole plan is the caller's defect, not
    // the provider's, and no call is spent on it.
    let (Some(Value::Array(introduced)), true) = (introduced, bound_plan(plan)) else {
        return Err(ProviderError::InvalidResponse);
    };
    let deadline = rest.deadline()?;
    let mut rows = Vec::new();
    for row in introduced {
        let (Some(destination), Some(repository)) =
            (row.text("destination"), row.member("repository"))
        else {
            continue;
        };
        if repository.text("dialect") != Some("gitea") || repository.text("host") != Some(host) {
            continue;
        }
        let (Some(owner), Some(name)) = (repository.text("owner"), repository.text("name")) else {
            continue;
        };
        let visibility = match rest.repository_visibility(owner, name, deadline) {
            Ok(visibility) => visibility,
            Err(ProviderError::Unavailable) => break,
            Err(defect) => return Err(defect),
        };
        let (fact, tail) = match visibility {
            super::rest::Visibility::Missing => ("missing", None),
            super::rest::Visibility::Denied => ("denied", None),
            super::rest::Visibility::Readable => {
                match resolve_tail(rest, repository, owner, name, deadline) {
                    Ok(resolution) => ("readable", resolution),
                    Err(ProviderError::Unavailable) => {
                        rows.push(forge_evidence_row(
                            destination,
                            "readable",
                            None,
                            checked_at,
                        ));
                        break;
                    }
                    Err(defect) => return Err(defect),
                }
            }
        };
        rows.push(forge_evidence_row(destination, fact, tail, checked_at));
    }
    evidence_file(plan, PRODUCER_NAME, producer_version, rows).ok_or(ProviderError::InvalidResponse)
}

/// Resolves the opaque tail against the readable repository. The gitea
/// grammar spells its own selector, `branch`, `tag`, or `commit`, so the
/// named family is authoritative and no cross-family ambiguity exists.
/// `None` means no resolution was established, never that one failed.
fn resolve_tail<R: GiteaVerification>(
    rest: &R,
    repository: &Value,
    owner: &str,
    name: &str,
    deadline: super::rest::OperationDeadline,
) -> Result<Option<&'static str>, ProviderError> {
    if !matches!(repository.text("form"), Some("src" | "raw" | "media")) {
        return Ok(None);
    }
    let Some(tail) = repository.text("tail") else {
        return Ok(None);
    };
    let tail = tail.strip_suffix('/').unwrap_or(tail);
    let (selector, remainder) = tail.split_once('/').unwrap_or((tail, ""));
    let family = match selector {
        "branch" => RefFamily::Heads,
        "tag" => RefFamily::Tags,
        "commit" => {
            return commit_resolution(rest, owner, name, remainder, deadline);
        }
        // Untyped legacy selectors are outside the grammar: no fact.
        _ => return Ok(None),
    };
    let Some(first) = remainder
        .split('/')
        .next()
        .filter(|segment| !segment.is_empty())
    else {
        return Ok(None);
    };
    let Some(names) = rest.matching_refs(owner, name, family, first, deadline)? else {
        return Ok(None);
    };
    let resolved = names.into_iter().find(|candidate| {
        remainder == candidate
            || remainder
                .strip_prefix(candidate.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    });
    // The selector named the family, so an absent ref there is the fact.
    let Some(reference) = resolved else {
        return Ok(Some("revision-missing"));
    };
    let path = remainder
        .get(reference.len()..)
        .unwrap_or_default()
        .trim_start_matches('/');
    finish(rest, owner, name, &reference, path, deadline)
}

fn commit_resolution<R: GiteaVerification>(
    rest: &R,
    owner: &str,
    name: &str,
    remainder: &str,
    deadline: super::rest::OperationDeadline,
) -> Result<Option<&'static str>, ProviderError> {
    let (revision, path) = remainder.split_once('/').unwrap_or((remainder, ""));
    if revision.is_empty() {
        return Ok(None);
    }
    match rest.commit_presence(owner, name, revision, deadline)? {
        Presence::Present => finish(rest, owner, name, revision, path, deadline),
        Presence::Absent => Ok(Some("revision-missing")),
        Presence::Unknown => Ok(None),
    }
}

fn finish<R: GiteaVerification>(
    rest: &R,
    owner: &str,
    name: &str,
    reference: &str,
    path: &str,
    deadline: super::rest::OperationDeadline,
) -> Result<Option<&'static str>, ProviderError> {
    if path.is_empty() {
        return Ok(Some("resolved"));
    }
    Ok(
        match rest.content_presence(owner, name, reference, path, deadline)? {
            Presence::Present => Some("resolved"),
            Presence::Absent => Some("path-missing"),
            Presence::Unknown => None,
        },
    )
}
