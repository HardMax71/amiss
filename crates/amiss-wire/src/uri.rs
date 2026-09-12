use crate::resolution::InvalidReference;

/// Returns the leading RFC 3986 scheme without normalizing its spelling.
#[must_use]
pub fn scheme(path: &str) -> Option<&str> {
    let mut bytes = path.bytes();
    let first = bytes.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    let mut length = 1_usize;
    for byte in bytes {
        match byte {
            b':' => return path.get(..length),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'.' | b'-' => {
                length = length.saturating_add(1);
            }
            _ => return None,
        }
    }
    None
}

/// Decodes one component exactly once while retaining malformed-escape precedence.
///
/// # Errors
///
/// An escape is malformed or the caller rejects a decoded byte.
pub fn decode_component(
    text: &str,
    out: &mut Vec<u8>,
    invalid: impl Fn(u8) -> Option<InvalidReference>,
) -> Result<(), InvalidReference> {
    let malformed = first_malformed_escape(text);
    let prefix = match malformed {
        Some(at) => text.get(..at).ok_or(InvalidReference::PercentEncoding)?,
        None => text,
    };
    let mut invalid_byte = None;
    for decoded in percent_encoding::percent_decode_str(prefix) {
        invalid_byte = invalid_byte.or_else(|| invalid(decoded));
        out.push(decoded);
    }
    if malformed.is_some() {
        return Err(InvalidReference::PercentEncoding);
    }
    invalid_byte.map_or(Ok(()), Err)
}

/// Decodes a fragment only when its escapes, UTF-8, and bytes are valid.
#[must_use]
pub fn decode_fragment(fragment: &str) -> Option<String> {
    let mut out = Vec::with_capacity(fragment.len());
    decode_component(fragment, &mut out, |byte| {
        matches!(byte, 0..=0x1f | 0x7f).then_some(InvalidReference::DecodedPathControl)
    })
    .ok()?;
    String::from_utf8(out).ok()
}

/// Applies the shared absolute-URI grammar after the caller has split components.
#[must_use]
pub fn absolute_valid(path: &str, scheme: &str, query: Option<&str>) -> bool {
    if !bytes_valid(path) || query.is_some_and(|value| !bytes_valid(value)) {
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

/// Accepts exactly the HTTP destination grammar shared by evidence producers and consumers.
#[must_use]
pub fn http_destination_valid(destination: &str) -> bool {
    let (before_fragment, fragment) = match destination.split_once('#') {
        Some((before, fragment)) => (before, Some(fragment)),
        None => (destination, None),
    };
    let (path, query) = match before_fragment.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (before_fragment, None),
    };
    let Some(scheme) = scheme(path) else {
        return false;
    };
    (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
        && absolute_valid(path, scheme, query)
        && fragment.is_none_or(|value| decode_fragment(value).is_some())
}

/// Accepts one exact absolute-path URI without a query or fragment.
#[must_use]
pub fn site_route_valid(route: &str) -> bool {
    route.starts_with('/')
        && !route.starts_with("//")
        && !route.contains(['?', '#'])
        && bytes_valid(route)
}

fn first_malformed_escape(text: &str) -> Option<usize> {
    text.match_indices('%').find_map(|(at, _)| {
        let valid = text
            .as_bytes()
            .get(at.saturating_add(1)..at.saturating_add(3))
            .is_some_and(|pair| pair.iter().all(u8::is_ascii_hexdigit));
        (!valid).then_some(at)
    })
}

fn bytes_valid(text: &str) -> bool {
    first_malformed_escape(text).is_none()
        && text.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
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
                        | b'%'
                )
        })
}

fn authority_valid(authority: &str) -> bool {
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
                    .is_some_and(|value| value.bytes().all(|byte| byte.is_ascii_digit())));
    }
    !authority.contains(['[', ']'])
}
