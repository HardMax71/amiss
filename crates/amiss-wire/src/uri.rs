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
    let bytes = text.as_bytes();
    let mut at = 0_usize;
    let mut invalid_byte = None;
    while let Some(&byte) = bytes.get(at) {
        let (decoded, consumed) = if byte == b'%' {
            let high = bytes.get(at.saturating_add(1)).copied();
            let low = bytes.get(at.saturating_add(2)).copied();
            let (Some(high), Some(low)) = (high, low) else {
                return Err(InvalidReference::PercentEncoding);
            };
            let (Some(high), Some(low)) = (hex_value(high), hex_value(low)) else {
                return Err(InvalidReference::PercentEncoding);
            };
            (high.wrapping_shl(4) | low, 3)
        } else {
            (byte, 1)
        };
        invalid_byte = invalid_byte.or_else(|| invalid(decoded));
        out.push(decoded);
        at = at.saturating_add(consumed);
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

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte.wrapping_sub(b'0')),
        b'a'..=b'f' => Some(byte.wrapping_sub(b'a').wrapping_add(10)),
        b'A'..=b'F' => Some(byte.wrapping_sub(b'A').wrapping_add(10)),
        _ => None,
    }
}

fn bytes_valid(text: &str) -> bool {
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
