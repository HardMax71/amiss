use amiss_wire::controls::TargetKind;
use amiss_wire::model::RepoPath;
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::InvalidReference;

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

pub(super) fn scheme_of(path_part: &str) -> Option<&str> {
    let mut bytes = path_part.bytes();
    let first = bytes.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    let mut length = 1_usize;
    for byte in bytes {
        match byte {
            b':' => {
                return path_part.get(..length);
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'.' | b'-' => {
                length = length.saturating_add(1);
            }
            _ => return None,
        }
    }
    None
}

/// One percent decode, never repeated: `%25` becomes a literal `%` and stays
/// one. The caller's byte grammar is applied while malformed escapes retain
/// precedence over decoded-byte defects anywhere in the component.
pub(super) fn decode_bytes(
    text: &str,
    out: &mut Vec<u8>,
    invalid: impl Fn(u8) -> Option<InvalidReference>,
) -> Result<(), Resolution> {
    let bytes = text.as_bytes();
    let mut at = 0_usize;
    let mut invalid_byte = None;
    while let Some(&byte) = bytes.get(at) {
        let (decoded, consumed) = if byte == b'%' {
            let high = bytes.get(at.saturating_add(1)).copied();
            let low = bytes.get(at.saturating_add(2)).copied();
            let (Some(high), Some(low)) = (high, low) else {
                return Err(Resolution::Invalid(InvalidReference::PercentEncoding));
            };
            let (Some(high), Some(low)) = (hex_value(high), hex_value(low)) else {
                return Err(Resolution::Invalid(InvalidReference::PercentEncoding));
            };
            (high.wrapping_shl(4) | low, 3)
        } else {
            (byte, 1)
        };
        invalid_byte = invalid_byte.or_else(|| invalid(decoded));
        out.push(decoded);
        at = at.saturating_add(consumed);
    }
    invalid_byte.map_or(Ok(()), |reason| Err(Resolution::Invalid(reason)))
}

pub(super) const fn invalid_path_byte(byte: u8) -> Option<InvalidReference> {
    match byte {
        b'/' => Some(InvalidReference::EncodedSlash),
        b'\\' => Some(InvalidReference::BackslashSeparator),
        0..=0x1f | 0x7f => Some(InvalidReference::DecodedPathControl),
        _ => None,
    }
}

/// Decodes a fragment: only invalid escapes, invalid UTF-8, and control bytes
/// invalidate it; separators are ordinary fragment characters.
pub(super) fn decode_fragment(fragment: &str) -> Option<String> {
    let mut out = Vec::with_capacity(fragment.len());
    decode_bytes(fragment, &mut out, |byte| {
        matches!(byte, 0..=0x1f | 0x7f).then_some(InvalidReference::DecodedPathControl)
    })
    .ok()?;
    String::from_utf8(out).ok()
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte.wrapping_sub(b'0')),
        b'a'..=b'f' => Some(byte.wrapping_sub(b'a').wrapping_add(10)),
        b'A'..=b'F' => Some(byte.wrapping_sub(b'A').wrapping_add(10)),
        _ => None,
    }
}

/// The ASCII RFC 3986 generic-syntax charset with two-hex-digit escapes:
/// unreserved, gen-delims, and sub-delims only, so a space, angle bracket,
/// quote, or non-ASCII byte is an invalid URI rather than data.
pub(super) fn uri_bytes_valid(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut at = 0_usize;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%' {
            let pair = (
                bytes.get(at.saturating_add(1)).copied().and_then(hex_value),
                bytes.get(at.saturating_add(2)).copied().and_then(hex_value),
            );
            if !matches!(pair, (Some(_), Some(_))) {
                return false;
            }
            at = at.saturating_add(3);
            continue;
        }
        let allowed = byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'.'
                    | b'_'
                    | b'~'
                    | b':'
                    | b'/'
                    | b'?'
                    | b'['
                    | b']'
                    | b'@'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b','
                    | b';'
                    | b'='
            );
        if !allowed {
            return false;
        }
        at = at.saturating_add(1);
    }
    true
}

pub(super) fn absolute_uri_valid(path: &str, scheme: &str, query: Option<&str>) -> bool {
    if !uri_bytes_valid(path) || query.is_some_and(|value| !uri_bytes_valid(value)) {
        return false;
    }
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return true;
    }
    let after_scheme = path
        .get(scheme.len().saturating_add(1)..)
        .unwrap_or_default();
    let Some(rest) = after_scheme.strip_prefix("//") else {
        return false;
    };
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let authority = rest.get(..authority_end).unwrap_or_default();
    !authority.is_empty() && authority_valid(authority)
}

pub(super) fn authority_valid(authority: &str) -> bool {
    if !authority.is_ascii() {
        return false;
    }
    if let Some(host) = authority.strip_prefix('[') {
        let Some((inside, port)) = host.split_once(']') else {
            return false;
        };
        return !inside.is_empty()
            && (port.is_empty()
                || port
                    .strip_prefix(':')
                    .is_some_and(|p| p.bytes().all(|b| b.is_ascii_digit())));
    }
    !authority.contains(['[', ']'])
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
        decode_bytes(segment, &mut resolved, invalid_path_byte)?;
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
