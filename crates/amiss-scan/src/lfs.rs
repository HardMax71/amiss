const MAX_POINTER_BYTES: usize = 1_023;
const MAX_SIZE_VALUE: u64 = 9_223_372_036_854_775_807;
const VERSIONS: [&str; 2] = [
    "version https://git-lfs.github.com/spec/v1",
    "version https://hawser.github.com/spec/v1",
];

/// Bytes-only LFS pointer recognition: no attributes, configuration, or
/// filters are consulted. A recognized pointer is 1 to 1,023 bytes, BOM-free
/// UTF-8, key/value lines under one consistent ending (all LF, or the exact
/// all-CRLF transform), the version line first, the later keys strictly
/// increasing, exactly one well-formed `oid` and one `size`. Anything else,
/// including a missing final ending, is ordinary content; an ordinary file
/// matching the grammar is deliberately a pointer.
#[must_use]
pub fn is_pointer(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.len() > MAX_POINTER_BYTES || bytes.starts_with(&[0xef, 0xbb, 0xbf])
    {
        return false;
    }
    let Ok(text) = str::from_utf8(bytes) else {
        return false;
    };
    let crlf = text.contains("\r\n");
    if crlf && text.replace("\r\n", "").contains(['\r', '\n']) {
        return false;
    }
    if !crlf && text.contains('\r') {
        return false;
    }
    let ending = if crlf { "\r\n" } else { "\n" };
    let Some(body) = text.strip_suffix(ending) else {
        return false;
    };

    let mut lines = body.split(ending);
    let Some(first) = lines.next() else {
        return false;
    };
    if !VERSIONS.contains(&first) {
        return false;
    }

    let mut previous_key: Option<&str> = None;
    let mut oid_lines = 0_u32;
    let mut size_lines = 0_u32;
    for line in lines {
        let Some((key, value)) = line.split_once(' ') else {
            return false;
        };
        if value.starts_with(' ') {
            return false;
        }
        if key.is_empty()
            || !key.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.' || byte == b'-'
            })
        {
            return false;
        }
        if previous_key.is_some_and(|previous| previous >= key) {
            return false;
        }
        previous_key = Some(key);
        match key {
            "version" => return false,
            "oid" => {
                oid_lines = oid_lines.saturating_add(1);
                let Some(hex) = value.strip_prefix("sha256:") else {
                    return false;
                };
                if hex.len() != 64
                    || !hex
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                {
                    return false;
                }
            }
            "size" => {
                size_lines = size_lines.saturating_add(1);
                if value != "0" && (value.starts_with('0') || value.is_empty()) {
                    return false;
                }
                if !value.bytes().all(|byte| byte.is_ascii_digit()) {
                    return false;
                }
                match value.parse::<u64>() {
                    Ok(size) if size <= MAX_SIZE_VALUE => {}
                    Ok(_) | Err(_) => return false,
                }
            }
            _ => {}
        }
    }
    oid_lines == 1 && size_lines == 1
}
