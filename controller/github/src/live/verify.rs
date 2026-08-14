mod tests;

use amiss_controller::ProviderError;
use amiss_wire::external::{bound_plan, evidence_file, forge_evidence_row};
use amiss_wire::json::Value;

use super::rest::{GitHubVerification, Presence, RefFamily, Visibility};

pub(super) const PRODUCER_NAME: &str = "amiss-controller-github";

/// Verifies the plan's introduced destinations shaped for this host through
/// the forge API and returns the evidence file. Facts only, never verdicts:
/// an answer the API refused or could not give omits the field it would
/// have filled, and a standing unavailability ends the walk early with the
/// rows already learned, since partial evidence beats none and a skipped
/// destination stays unproven downstream.
pub(super) fn verify_external<R: GitHubVerification>(
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
        if repository.text("dialect") != Some("github") || repository.text("host") != Some(host) {
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
            Visibility::Missing => ("missing", None),
            Visibility::Denied => ("denied", None),
            Visibility::Readable => match resolve_tail(rest, repository, owner, name, deadline) {
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
            },
        };
        rows.push(forge_evidence_row(destination, fact, tail, checked_at));
    }
    evidence_file(plan, PRODUCER_NAME, producer_version, rows).ok_or(ProviderError::InvalidResponse)
}

/// Resolves the opaque tail against the readable repository: a whole-segment
/// ref match under heads then tags, a commit id as the fallback the URL
/// grammar allows, then the path under whatever resolved. `None` means no
/// resolution was established, never that one failed.
fn resolve_tail<R: GitHubVerification>(
    rest: &R,
    repository: &Value,
    owner: &str,
    name: &str,
    deadline: super::rest::OperationDeadline,
) -> Result<Option<&'static str>, ProviderError> {
    if !matches!(repository.text("form"), Some("blob" | "tree" | "raw")) {
        return Ok(None);
    }
    let Some(tail) = repository.text("tail") else {
        return Ok(None);
    };
    let tail = tail.strip_suffix('/').unwrap_or(tail);
    let Some(first) = tail.split('/').next().filter(|segment| !segment.is_empty()) else {
        return Ok(None);
    };
    let mut resolved = None;
    for family in [RefFamily::Heads, RefFamily::Tags] {
        let Some(names) = rest.matching_refs(owner, name, family, first, deadline)? else {
            return Ok(None);
        };
        resolved = names.into_iter().find(|candidate| {
            tail == candidate
                || tail
                    .strip_prefix(candidate.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        });
        if resolved.is_some() {
            break;
        }
    }
    // The commit route also resolves what no ref names: symbolic HEAD and
    // abbreviated ids. Only its positive absence may claim revision-missing,
    // since a false refutation is the worst answer this producer can give.
    let reference = match resolved {
        Some(reference) => reference,
        None => match rest.commit_presence(owner, name, first, deadline)? {
            Presence::Present => first.to_owned(),
            Presence::Absent => return Ok(Some("revision-missing")),
            Presence::Unknown => return Ok(None),
        },
    };
    let path = tail
        .get(reference.len()..)
        .unwrap_or_default()
        .trim_start_matches('/');
    if path.is_empty() {
        return Ok(Some("resolved"));
    }
    Ok(
        match rest.content_presence(owner, name, &reference, path, deadline)? {
            Presence::Present => Some("resolved"),
            Presence::Absent => Some("path-missing"),
            Presence::Unknown => None,
        },
    )
}
