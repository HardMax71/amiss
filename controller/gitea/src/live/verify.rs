mod tests;

use amiss_controller::{ProviderError, ref_span, spelled_segments};
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
/// The tail still wears the URL's percent-escapes, so each segment is
/// decoded once after splitting, and a spelling whose escaped slash
/// rewrites segmentation is only ever confirmed, never refuted. `None`
/// means no resolution was established, never that one failed.
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
    let Some(segments) = spelled_segments(tail) else {
        return Ok(None);
    };
    let Some((selector, remainder)) = segments.split_first() else {
        return Ok(None);
    };
    let rewritten = segments.iter().any(|segment| segment.contains('/'));
    let family = match selector.as_str() {
        "branch" => RefFamily::Heads,
        "tag" => RefFamily::Tags,
        "commit" => {
            return commit_resolution(rest, owner, name, remainder, rewritten, deadline);
        }
        // Untyped legacy selectors are outside the grammar: no fact.
        _ => return Ok(None),
    };
    let Some(first) = remainder
        .first()
        .map(String::as_str)
        .filter(|segment| !segment.is_empty())
    else {
        return Ok(None);
    };
    let Some(names) = rest.matching_refs(owner, name, family, first, deadline)? else {
        return Ok(None);
    };
    let resolved = names
        .into_iter()
        .find_map(|candidate| ref_span(remainder, &candidate).map(|span| (candidate, span)));
    // The selector named the family, so an absent ref there is the fact,
    // unless the spelling left the revision boundary to the forge.
    let Some((reference, span)) = resolved else {
        return Ok(if rewritten {
            None
        } else {
            Some("revision-missing")
        });
    };
    let path = remainder.get(span..).unwrap_or_default();
    finish(rest, owner, name, &reference, path, rewritten, deadline)
}

fn commit_resolution<R: GiteaVerification>(
    rest: &R,
    owner: &str,
    name: &str,
    remainder: &[String],
    rewritten: bool,
    deadline: super::rest::OperationDeadline,
) -> Result<Option<&'static str>, ProviderError> {
    let Some(revision) = remainder
        .first()
        .map(String::as_str)
        .filter(|segment| !segment.is_empty())
    else {
        return Ok(None);
    };
    let path = remainder.get(1..).unwrap_or_default();
    match rest.commit_presence(owner, name, revision, deadline)? {
        Presence::Present => finish(rest, owner, name, revision, path, rewritten, deadline),
        Presence::Absent if rewritten => Ok(None),
        Presence::Absent => Ok(Some("revision-missing")),
        Presence::Unknown => Ok(None),
    }
}

fn finish<R: GiteaVerification>(
    rest: &R,
    owner: &str,
    name: &str,
    reference: &str,
    path: &[String],
    rewritten: bool,
    deadline: super::rest::OperationDeadline,
) -> Result<Option<&'static str>, ProviderError> {
    if path.is_empty() {
        return Ok(Some("resolved"));
    }
    Ok(
        match rest.content_presence(owner, name, reference, path, deadline)? {
            Presence::Present => Some("resolved"),
            Presence::Absent if rewritten => None,
            Presence::Absent => Some("path-missing"),
            Presence::Unknown => None,
        },
    )
}
