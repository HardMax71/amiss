mod tests;

use amiss_controller::{
    ForgeFact, ForgeNegative, ForgePresence as Presence, ForgeProducer,
    ForgeRefFamily as RefFamily, ForgeTail, ForgeVisibility as Visibility, ProviderError,
    forge_evidence, forge_repository_evidence, ref_span, spelled_segments,
};
use amiss_wire::json::Value;
use serde::Deserialize;

use super::GitLabClient;
use super::transport::Budget;

pub(super) const PRODUCER_NAME: &str = "amiss-controller-gitlab";

/// The read-only verification surface, apart from refresh and publication
/// on purpose: a verifier holding this can state facts and nothing else.
pub(super) trait GitLabVerification: Send + Sync {
    fn budget(&self) -> Result<Budget, ProviderError>;

    fn project_visibility(
        &self,
        project: &str,
        budget: Budget,
    ) -> Result<(Visibility, Budget), ProviderError>;

    /// Ref names in the family sharing the prefix; `None` when the project
    /// stopped answering for them or the listing could not be proven
    /// complete, so no ref fact exists.
    fn matching_refs(
        &self,
        project: &str,
        family: RefFamily,
        prefix: &str,
        budget: Budget,
    ) -> Result<(Option<Vec<String>>, Budget), ProviderError>;

    fn file_presence(
        &self,
        project: &str,
        reference: &str,
        path: &str,
        budget: Budget,
    ) -> Result<(Presence, Budget), ProviderError>;

    fn tree_presence(
        &self,
        project: &str,
        reference: &str,
        path: &str,
        budget: Budget,
    ) -> Result<(Presence, Budget), ProviderError>;

    fn commit_presence(
        &self,
        project: &str,
        revision: &str,
        budget: Budget,
    ) -> Result<(Presence, Budget), ProviderError>;
}

/// Verifies the plan's introduced destinations shaped for this host through
/// the forge API and returns the evidence file. Facts only, never verdicts:
/// an answer the API refused or could not give omits the field it would
/// have filled, and a standing unavailability ends the walk early with the
/// rows already learned, since partial evidence beats none and a skipped
/// destination stays unproven downstream.
pub(super) fn verify_external<R: GitLabVerification>(
    rest: &R,
    plan: &Value,
    host: &str,
    producer_version: &str,
    checked_at: &str,
) -> Result<Value, ProviderError> {
    forge_evidence(
        plan,
        ForgeProducer {
            dialect: "gitlab",
            host,
            name: PRODUCER_NAME,
            version: producer_version,
            checked_at,
        },
        || rest.budget(),
        |budget, target| {
            let project = format!("{}/{}", target.owner, target.name);
            let (visibility, spent) = rest.project_visibility(&project, *budget)?;
            *budget = spent;
            forge_repository_evidence(visibility, || {
                resolve_tail(rest, target.repository, &project, budget)
            })
        },
    )
}

/// Resolves the opaque tail against the readable project: a whole-segment
/// ref match under heads then tags, a commit id as the fallback the URL
/// grammar allows, then the path under whatever resolved. The tail still
/// wears the URL's percent-escapes, so each segment is decoded once after
/// splitting, and a spelling whose escaped slash rewrites segmentation is
/// only ever confirmed, never refuted. `None` means no resolution was
/// established, never that one failed.
fn resolve_tail<R: GitLabVerification>(
    rest: &R,
    repository: &Value,
    project: &str,
    budget: &mut Budget,
) -> Result<Option<ForgeTail>, ProviderError> {
    let Some(form) = repository
        .text("form")
        .filter(|form| matches!(*form, "blob" | "tree" | "raw"))
    else {
        return Ok(None);
    };
    let Some(tail) = repository.text("tail") else {
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
        let (names, spent) = rest.matching_refs(project, family, first, *budget)?;
        *budget = spent;
        let Some(names) = names else {
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
    let (reference, span) = if let Some(resolved) = resolved {
        resolved
    } else {
        let (presence, spent) = rest.commit_presence(project, first, *budget)?;
        *budget = spent;
        match presence {
            Presence::Present => (first.to_owned(), 1),
            Presence::Absent if rewritten => return Ok(None),
            Presence::Absent => return Ok(Some(ForgeTail::RevisionMissing)),
            Presence::Unknown => return Ok(None),
        }
    };
    let path = segments.get(span..).unwrap_or_default();
    if path.is_empty() {
        return Ok(Some(ForgeTail::Resolved));
    }
    // The API takes the whole path as one parameter, so the decoded
    // segments travel joined; a tree tail names a directory, which the
    // files route would deny regardless of presence, and each form asks
    // the route that can see it.
    let path = path.join("/");
    let (presence, spent) = if form == "tree" {
        rest.tree_presence(project, &reference, &path, *budget)?
    } else {
        rest.file_presence(project, &reference, &path, *budget)?
    };
    *budget = spent;
    Ok(match presence {
        Presence::Present => Some(ForgeTail::Resolved),
        Presence::Absent if rewritten => None,
        Presence::Absent => Some(ForgeTail::PathMissing),
        Presence::Unknown => None,
    })
}

#[derive(Deserialize)]
struct NamedRef {
    name: String,
}

fn presence<T>(fact: &ForgeFact<T>) -> Presence {
    match fact {
        Ok(_) => Presence::Present,
        Err(ForgeNegative::Missing) => Presence::Absent,
        Err(ForgeNegative::Denied) => Presence::Unknown,
    }
}

impl GitLabVerification for GitLabClient {
    fn budget(&self) -> Result<Budget, ProviderError> {
        self.transport.budget()
    }

    fn project_visibility(
        &self,
        project: &str,
        budget: Budget,
    ) -> Result<(Visibility, Budget), ProviderError> {
        let url = self.transport.endpoint(["projects", project])?;
        let (fact, budget) = self
            .transport
            .get_fact::<serde::de::IgnoredAny>(url, budget)?;
        Ok((
            match fact {
                Ok(_) => Visibility::Readable,
                Err(ForgeNegative::Missing) => Visibility::Missing,
                Err(ForgeNegative::Denied) => Visibility::Denied,
            },
            budget,
        ))
    }

    fn matching_refs(
        &self,
        project: &str,
        family: RefFamily,
        prefix: &str,
        budget: Budget,
    ) -> Result<(Option<Vec<String>>, Budget), ProviderError> {
        let route = match family {
            RefFamily::Heads => "branches",
            RefFamily::Tags => "tags",
        };
        let mut budget = budget;
        let mut names = Vec::new();
        for page in 1..=super::MAX_PAGES {
            let mut url = self
                .transport
                .endpoint(["projects", project, "repository", route])?;
            url.query_pairs_mut()
                .append_pair("search", &format!("^{prefix}"))
                .append_pair("per_page", &super::PAGE_SIZE.to_string())
                .append_pair("page", &page.to_string());
            let (fact, spent) = self.transport.get_fact::<Vec<NamedRef>>(url, budget)?;
            budget = spent;
            let batch = match fact {
                Ok(refs) => refs,
                Err(ForgeNegative::Missing | ForgeNegative::Denied) => {
                    return Ok((None, budget));
                }
            };
            if batch.len() > super::PAGE_SIZE {
                return Err(ProviderError::InvalidResponse);
            }
            let complete = batch.len() < super::PAGE_SIZE;
            names.extend(
                batch
                    .into_iter()
                    .map(|reference| reference.name)
                    .filter(|name| name.starts_with(prefix)),
            );
            if complete {
                return Ok((Some(names), budget));
            }
        }
        // Ten full pages leave the listing unproven complete, and a truncated
        // candidate set could become a false refutation downstream: no fact.
        Ok((None, budget))
    }

    fn file_presence(
        &self,
        project: &str,
        reference: &str,
        path: &str,
        budget: Budget,
    ) -> Result<(Presence, Budget), ProviderError> {
        let mut url =
            self.transport
                .endpoint(["projects", project, "repository", "files", path])?;
        url.query_pairs_mut().append_pair("ref", reference);
        let (fact, budget) = self.transport.head_fact(url, budget)?;
        Ok((presence(&fact), budget))
    }

    fn tree_presence(
        &self,
        project: &str,
        reference: &str,
        path: &str,
        budget: Budget,
    ) -> Result<(Presence, Budget), ProviderError> {
        let mut url = self
            .transport
            .endpoint(["projects", project, "repository", "tree"])?;
        url.query_pairs_mut()
            .append_pair("path", path)
            .append_pair("ref", reference)
            .append_pair("per_page", "1");
        let (fact, budget) = self
            .transport
            .get_fact::<Vec<serde::de::IgnoredAny>>(url, budget)?;
        Ok((
            match &fact {
                // An empty page is either an empty directory or a path the
                // route ignores, and GitLab does not say which: no fact.
                Ok(rows) if rows.is_empty() => Presence::Unknown,
                Ok(_) | Err(ForgeNegative::Missing | ForgeNegative::Denied) => presence(&fact),
            },
            budget,
        ))
    }

    fn commit_presence(
        &self,
        project: &str,
        revision: &str,
        budget: Budget,
    ) -> Result<(Presence, Budget), ProviderError> {
        let url =
            self.transport
                .endpoint(["projects", project, "repository", "commits", revision])?;
        let (fact, budget) = self
            .transport
            .get_fact::<serde::de::IgnoredAny>(url, budget)?;
        Ok((presence(&fact), budget))
    }
}
