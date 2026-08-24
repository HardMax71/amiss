use amiss_wire::controls::TargetKind;
use amiss_wire::model::RepoPath;
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::InvalidReference;
use amiss_wire::uri::decode_component;

use super::{Intent, Resolution};

/// The recognition opening: `https://`, the declared host byte-exact, then
/// the path separator. Anything less exact is not this repository's forge.
pub(super) fn same_repo_suffix<'a>(path_part: &'a str, host: &str) -> Option<&'a str> {
    path_part
        .strip_prefix("https://")?
        .strip_prefix(host)?
        .strip_prefix('/')
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

pub(super) const fn invalid_path_byte(byte: u8) -> Option<InvalidReference> {
    match byte {
        b'/' => Some(InvalidReference::EncodedSlash),
        b'\\' => Some(InvalidReference::BackslashSeparator),
        0..=0x1f | 0x7f => Some(InvalidReference::DecodedPathControl),
        _ => None,
    }
}

pub(super) fn normalized_native_path(
    document_path: &RepoPath,
    is_image: bool,
    path_part: &str,
) -> Result<(RepoPath, TargetKind), Resolution> {
    if path_part.contains('\\') {
        return Err(Resolution::Invalid(InvalidReference::BackslashSeparator));
    }
    let trailing_slash = path_part.len() > 1 && path_part.ends_with('/');
    let path = path_part.strip_suffix('/').unwrap_or(path_part);
    if path.split('/').any(str::is_empty) || (trailing_slash && is_image) {
        return Err(Resolution::Invalid(InvalidReference::Syntax));
    }
    let target_kind = if trailing_slash {
        TargetKind::Tree
    } else if is_image {
        TargetKind::Blob
    } else {
        TargetKind::Either
    };

    let raw_document = document_path.as_bytes();
    let parent = raw_document
        .iter()
        .rposition(|byte| *byte == b'/')
        .and_then(|split| raw_document.get(..split))
        .unwrap_or_default();
    let mut resolved =
        Vec::with_capacity(parent.len().saturating_add(path.len()).saturating_add(1));
    resolved.extend_from_slice(parent);
    for segment in path.split('/') {
        let prior = resolved.len();
        if prior > 0 {
            resolved.push(b'/');
        }
        let decoded = resolved.len();
        decode_component(segment, &mut resolved, invalid_path_byte).map_err(Resolution::Invalid)?;
        match resolved.get(decoded..).unwrap_or_default() {
            b"." => resolved.truncate(prior),
            b".." => {
                resolved.truncate(prior);
                if resolved.is_empty() {
                    return Err(Resolution::Invalid(InvalidReference::PathTraversal));
                }
                match resolved.iter().rposition(|byte| *byte == b'/') {
                    Some(separator) => resolved.truncate(separator),
                    None => resolved.clear(),
                }
            }
            _ => {}
        }
    }
    let Some(joined) = RepoPath::from_bytes(resolved) else {
        return Err(Resolution::Invalid(InvalidReference::Syntax));
    };
    Ok((joined, target_kind))
}
