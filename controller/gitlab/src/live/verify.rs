mod tests;

use amiss_controller::ProviderError;
use amiss_wire::external::{bound_plan, evidence_file, forge_evidence_row};
use amiss_wire::json::Value;
use serde::Deserialize;

use super::GitLabClient;
use super::transport::{Budget, Fact};

pub(super) const PRODUCER_NAME: &str = "amiss-controller-gitlab";

/// What the API said about a foreign project itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Visibility {
    Readable,
    Missing,
    Denied,
}

/// Whether a route's subject exists; Unknown when the answer names neither
/// presence nor absence, as the tree route's empty page does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Presence {
    Present,
    Absent,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RefFamily {
    Heads,
    Tags,
}

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
    let introduced = plan
        .member("payload")
        .and_then(|payload| payload.member("introduced"));
    // A value that is not a digest-whole plan is the caller's defect, not
    // the provider's, and no call is spent on it.
    let (Some(Value::Array(introduced)), true) = (introduced, bound_plan(plan)) else {
        return Err(ProviderError::InvalidResponse);
    };
    let mut budget = rest.budget()?;
    let mut rows = Vec::new();
    for row in introduced {
        let (Some(destination), Some(repository)) =
            (row.text("destination"), row.member("repository"))
        else {
            continue;
        };
        if repository.text("dialect") != Some("gitlab") || repository.text("host") != Some(host) {
            continue;
        }
        let (Some(owner), Some(name)) = (repository.text("owner"), repository.text("name")) else {
            continue;
        };
        let project = format!("{owner}/{name}");
        let (visibility, spent) = match rest.project_visibility(&project, budget) {
            Ok(answer) => answer,
            Err(ProviderError::Unavailable) => break,
            Err(defect) => return Err(defect),
        };
        budget = spent;
        let (fact, tail) = match visibility {
            Visibility::Missing => ("missing", None),
            Visibility::Denied => ("denied", None),
            Visibility::Readable => match resolve_tail(rest, repository, &project, &mut budget) {
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

/// Resolves the opaque tail against the readable project: a whole-segment
/// ref match under heads then tags, a commit id as the fallback the URL
/// grammar allows, then the path under whatever resolved. `None` means no
/// resolution was established, never that one failed.
fn resolve_tail<R: GitLabVerification>(
    rest: &R,
    repository: &Value,
    project: &str,
    budget: &mut Budget,
) -> Result<Option<&'static str>, ProviderError> {
    let Some(form) = repository
        .text("form")
        .filter(|form| matches!(*form, "blob" | "tree" | "raw"))
    else {
        return Ok(None);
    };
    let Some(tail) = repository.text("tail") else {
        return Ok(None);
    };
    let tail = tail.strip_suffix('/').unwrap_or(tail);
    let Some(first) = tail.split('/').next().filter(|segment| !segment.is_empty()) else {
        return Ok(None);
    };
    let mut matches = Vec::new();
    for family in [RefFamily::Heads, RefFamily::Tags] {
        let (names, spent) = rest.matching_refs(project, family, first, *budget)?;
        *budget = spent;
        let Some(names) = names else {
            return Ok(None);
        };
        // Within one family a second whole-segment match cannot exist, since
        // git refuses a ref nesting under another; across families it can.
        matches.extend(names.into_iter().find(|candidate| {
            tail == candidate
                || tail
                    .strip_prefix(candidate.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        }));
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
    let reference = if let Some(reference) = resolved {
        reference
    } else {
        let (presence, spent) = rest.commit_presence(project, first, *budget)?;
        *budget = spent;
        match presence {
            Presence::Present => first.to_owned(),
            Presence::Absent => return Ok(Some("revision-missing")),
            Presence::Unknown => return Ok(None),
        }
    };
    let path = tail
        .get(reference.len()..)
        .unwrap_or_default()
        .trim_start_matches('/');
    if path.is_empty() {
        return Ok(Some("resolved"));
    }
    // A tree tail names a directory, which the files route would deny
    // regardless of presence; each form asks the route that can see it.
    let (presence, spent) = if form == "tree" {
        rest.tree_presence(project, &reference, path, *budget)?
    } else {
        rest.file_presence(project, &reference, path, *budget)?
    };
    *budget = spent;
    Ok(match presence {
        Presence::Present => Some("resolved"),
        Presence::Absent => Some("path-missing"),
        Presence::Unknown => None,
    })
}

#[derive(Deserialize)]
struct NamedRef {
    name: String,
}

fn presence<T>(fact: &Fact<T>) -> Presence {
    match fact {
        Fact::Found(_) => Presence::Present,
        Fact::Missing => Presence::Absent,
        Fact::Denied => Presence::Unknown,
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
                Fact::Found(_) => Visibility::Readable,
                Fact::Missing => Visibility::Missing,
                Fact::Denied => Visibility::Denied,
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
                Fact::Found(refs) => refs,
                Fact::Missing | Fact::Denied => return Ok((None, budget)),
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
                Fact::Found(rows) if rows.is_empty() => Presence::Unknown,
                Fact::Found(_) | Fact::Missing | Fact::Denied => presence(&fact),
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
