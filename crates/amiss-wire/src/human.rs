pub const ATOM_SCALAR_BOUND: usize = 200;

/// `human-atom-v1`: every repository-derived scalar is rendered as a
/// double-quoted ASCII JSON-style string. At most the first two hundred
/// Unicode scalar values are kept, with a literal `...` appended inside the
/// quotes when any were omitted. Quote and backslash escape as `\"` and
/// `\\`, printable ASCII stays literal, and every other scalar becomes a
/// lowercase `\uXXXX` escape, non-BMP scalars as a UTF-16 surrogate pair, so
/// CR, LF, tab, ESC, bidi controls, and ANSI bytes are never active terminal
/// syntax.
#[must_use]
pub fn atom(text: &str) -> String {
    let mut out = String::with_capacity(text.len().saturating_add(2));
    out.push('"');
    let mut omitted = false;
    for (index, scalar) in text.chars().enumerate() {
        if index >= ATOM_SCALAR_BOUND {
            omitted = true;
            break;
        }
        match scalar {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(scalar),
            _ => {
                let mut units = [0_u16; 2];
                for unit in scalar.encode_utf16(&mut units) {
                    out.push_str("\\u");
                    for shift in [12_u32, 8, 4, 0] {
                        let nibble = (u32::from(*unit) >> shift) & 0xf;
                        out.push(char::from_digit(nibble, 16).unwrap_or('0'));
                    }
                }
            }
        }
    }
    if omitted {
        out.push_str("...");
    }
    out.push('"');
    out
}

/// `human-atom-bytes-v1`: a repository-derived value that is raw bytes, not
/// text, rendered under the same law as [`atom`]. At most the first two
/// hundred bytes are kept with a literal `...` appended inside the quotes
/// when any were omitted; printable ASCII stays literal with quote and
/// backslash escaped, and every other byte becomes the lowercase `\u00xx`
/// escape of its value, so no byte is ever active terminal syntax and no
/// Unicode scalar is invented for bytes that never encoded one.
#[must_use]
pub fn atom_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().saturating_add(2));
    out.push('"');
    let mut omitted = false;
    for (index, byte) in bytes.iter().enumerate() {
        if index >= ATOM_SCALAR_BOUND {
            omitted = true;
            break;
        }
        match byte {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b' '..=b'~' => out.push(char::from(*byte)),
            _ => {
                out.push_str("\\u00");
                for shift in [4_u32, 0] {
                    let nibble = (u32::from(*byte) >> shift) & 0xf;
                    out.push(char::from_digit(nibble, 16).unwrap_or('0'));
                }
            }
        }
    }
    if omitted {
        out.push_str("...");
    }
    out.push('"');
    out
}
