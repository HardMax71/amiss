use amiss_wire::report::IntentKind;
use amiss_wire::resolution::{InvalidReference, UnsupportedSemantics};

use crate::route::bundler_request;

use super::Intent;

use amiss_wire::model::RepoPath;
use amiss_wire::resolution::Resolution;

/// The recognition opening: `https://`, the declared host byte-exact, then
/// the path separator. Anything less exact is not this repository's forge.
pub(super) fn same_repo_suffix<'a>(path_part: &'a str, host: &str) -> Option<&'a str> {
    path_part
        .strip_prefix("https://")?
        .strip_prefix(host)?
        .strip_prefix('/')
}

/// A destination no tree answers whatever the document above it: a protocol
/// relative network path, and the inline request syntax a bundler owns.
pub(super) fn unreadable(path_part: &str) -> Option<Resolution<RepoPath>> {
    if path_part.starts_with("//") {
        return Some(Resolution::UnsupportedSemantics(
            UnsupportedSemantics::NetworkPath,
        ));
    }
    bundler_request(path_part).then_some(Resolution::Invalid {
        reason: InvalidReference::Syntax,
    })
}

pub(super) fn unsupported_intent(query: Option<String>, fragment: Option<String>) -> Intent {
    Intent {
        kind: IntentKind::Unsupported,
        commit_oid: None,
        repository_path: None,
        target_kind: None,
        external_scheme: None,
        query,
        fragment,
    }
}

/// RFC 3986 order: the first `#` opens the fragment through end; within the
/// prefix the first `?` opens the query. `a?x?y#z?u` has query `x?y` and
/// fragment `z?u`. A field is absent exactly when its delimiter is.
pub(super) fn split_components(semantic: &str) -> (&str, Option<String>, Option<String>) {
    let (before, fragment) = match semantic.split_once('#') {
        Some((before, after)) => (before, Some(after.to_owned())),
        None => (semantic, None),
    };
    let (path, query) = match before.split_once('?') {
        Some((path, after)) => (path, Some(after.to_owned())),
        None => (before, None),
    };
    (path, query, fragment)
}
