mod tests;

use amiss_controller::{
    ForgeProducer, ForgeTail, ProviderError, forge_evidence, forge_repository_evidence, ref_span,
    spelled_segments,
};
use amiss_wire::model::ForgeDialect;

use super::rest::{GiteaVerification, Presence, RefFamily};

pub(super) const PRODUCER_NAME: &str = "amiss-controller-gitea";

/// Verifies the plan's introduced destinations shaped for this host through
/// the forge API and returns the evidence file. Facts only, never verdicts:
/// an answer the API refused omits the field it would have filled, and a
/// standing unavailability ends the walk early with the rows already
/// learned, since a skipped destination stays unproven downstream.
pub(super) fn verify_external<R: GiteaVerification>(
    rest: &R,
    plan: &[u8],
    host: &str,
    producer_version: &str,
    checked_at: &str,
) -> Result<Vec<u8>, ProviderError> {
    forge_evidence(
        plan,
        ForgeProducer {
            dialect: ForgeDialect::Gitea,
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

/// Resolves the opaque tail against the readable repository. The gitea
/// grammar spells its own selector, `branch`, `tag`, or `commit`, so the
/// named family is authoritative and no cross-family ambiguity exists.
/// The tail still wears the URL's percent-escapes, so each segment is
/// decoded once after splitting, and a spelling whose escaped slash
/// rewrites segmentation is only ever confirmed, never refuted. `None`
/// means no resolution was established, never that one failed.
fn resolve_tail<R: GiteaVerification>(
    rest: &R,
    repository: &amiss_wire::external::ExternalRepository,
    deadline: super::rest::OperationDeadline,
) -> Result<Option<ForgeTail>, ProviderError> {
    if !matches!(repository.form.as_deref(), Some("src" | "raw" | "media")) {
        return Ok(None);
    }
    let Some(tail) = repository.tail.as_deref() else {
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
            return commit_resolution(rest, repository, remainder, rewritten, deadline);
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
    let Some(names) =
        rest.matching_refs(&repository.owner, &repository.name, family, first, deadline)?
    else {
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
            Some(ForgeTail::RevisionMissing)
        });
    };
    let path = remainder.get(span..).unwrap_or_default();
    finish(rest, repository, &reference, path, rewritten, deadline)
}

fn commit_resolution<R: GiteaVerification>(
    rest: &R,
    repository: &amiss_wire::external::ExternalRepository,
    remainder: &[String],
    rewritten: bool,
    deadline: super::rest::OperationDeadline,
) -> Result<Option<ForgeTail>, ProviderError> {
    let Some(revision) = remainder
        .first()
        .map(String::as_str)
        .filter(|segment| !segment.is_empty())
    else {
        return Ok(None);
    };
    let path = remainder.get(1..).unwrap_or_default();
    match rest.commit_presence(&repository.owner, &repository.name, revision, deadline)? {
        Presence::Present => finish(rest, repository, revision, path, rewritten, deadline),
        Presence::Absent if rewritten => Ok(None),
        Presence::Absent => Ok(Some(ForgeTail::RevisionMissing)),
        Presence::Unknown => Ok(None),
    }
}

fn finish<R: GiteaVerification>(
    rest: &R,
    repository: &amiss_wire::external::ExternalRepository,
    reference: &str,
    path: &[String],
    rewritten: bool,
    deadline: super::rest::OperationDeadline,
) -> Result<Option<ForgeTail>, ProviderError> {
    if path.is_empty() {
        return Ok(Some(ForgeTail::Resolved));
    }
    Ok(
        match rest.content_presence(
            &repository.owner,
            &repository.name,
            reference,
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
