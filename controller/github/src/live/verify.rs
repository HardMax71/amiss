mod tests;

use amiss_controller::{
    ForgeProducer, ForgeTail, ProviderError, forge_evidence, forge_repository_evidence, ref_span,
    spelled_segments,
};
use amiss_wire::json::Value;
use amiss_wire::model::ForgeDialect;

use super::rest::{GitHubVerification, Presence, RefFamily};

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
    forge_evidence(
        plan,
        ForgeProducer {
            dialect: ForgeDialect::Github,
            host,
            name: PRODUCER_NAME,
            version: producer_version,
            checked_at,
        },
        || rest.deadline(),
        |deadline, repository| {
            let visibility =
                rest.repository_visibility(&repository.owner, &repository.name, *deadline)?;
            forge_repository_evidence(visibility, || resolve_tail(rest, repository, *deadline))
        },
    )
}

/// Resolves the opaque tail against the readable repository: a whole-segment
/// ref match under heads then tags, a commit id as the fallback the URL
/// grammar allows, then the path under whatever resolved. The tail still
/// wears the URL's percent-escapes, so each segment is decoded once after
/// splitting, and a spelling whose escaped slash rewrites segmentation is
/// only ever confirmed, never refuted. `None` means no resolution was
/// established, never that one failed.
fn resolve_tail<R: GitHubVerification>(
    rest: &R,
    repository: &amiss_wire::external::ExternalRepository,
    deadline: super::rest::OperationDeadline,
) -> Result<Option<ForgeTail>, ProviderError> {
    if !matches!(repository.form.as_deref(), Some("blob" | "tree" | "raw")) {
        return Ok(None);
    }
    let Some(tail) = repository.tail.as_deref() else {
        return Ok(None);
    };
    let Some(segments) = spelled_segments(tail) else {
        return Ok(None);
    };
    let Some(first) = segments
        .first()
        .map(String::as_str)
        .filter(|segment| !segment.is_empty())
    else {
        return Ok(None);
    };
    let rewritten = segments.iter().any(|segment| segment.contains('/'));
    let mut matches = Vec::new();
    for family in [RefFamily::Heads, RefFamily::Tags] {
        let Some(names) =
            rest.matching_refs(&repository.owner, &repository.name, family, first, deadline)?
        else {
            return Ok(None);
        };
        // Within one family a second whole-segment match cannot exist, since
        // git refuses a ref nesting under another; across families it can.
        matches.extend(
            names.into_iter().find_map(|candidate| {
                ref_span(&segments, &candidate).map(|span| (candidate, span))
            }),
        );
    }
    // A branch and a differing tag both matching leave the revision split
    // ambiguous, and the forge's tie-break is its own; that is no fact.
    let resolved = match matches.as_slice() {
        [only] => Some(only.clone()),
        [head, tag] if head == tag => Some(head.clone()),
        [] => None,
        [_, ..] => return Ok(None),
    };
    // The commit route also resolves what no ref names: symbolic HEAD and
    // abbreviated ids. Only its positive absence may claim revision-missing,
    // since a false refutation is the worst answer this producer can give.
    let (reference, span) = match resolved {
        Some(resolved) => resolved,
        None => match rest.commit_presence(&repository.owner, &repository.name, first, deadline)? {
            Presence::Present => (first.to_owned(), 1),
            Presence::Absent if rewritten => return Ok(None),
            Presence::Absent => return Ok(Some(ForgeTail::RevisionMissing)),
            Presence::Unknown => return Ok(None),
        },
    };
    let path = segments.get(span..).unwrap_or_default();
    if path.is_empty() {
        return Ok(Some(ForgeTail::Resolved));
    }
    Ok(
        match rest.content_presence(
            &repository.owner,
            &repository.name,
            &reference,
            path,
            deadline,
        )? {
            Presence::Present => Some(ForgeTail::Resolved),
            Presence::Absent if rewritten => None,
            Presence::Absent => Some(ForgeTail::PathMissing),
            Presence::Unknown => None,
        },
    )
}
