use amiss_wire::digest::hb;

/// Frames ordered string fields into one provider-neutral route identity.
#[must_use]
pub fn framed_route_id(domain: &str, prefix: &str, fields: &[&str]) -> Option<String> {
    let valid_prefix = !prefix.is_empty()
        && prefix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if domain.is_empty() || !valid_prefix || fields.is_empty() || fields.contains(&"") {
        return None;
    }
    let mut frame = Vec::new();
    frame.extend_from_slice(&u64::try_from(fields.len()).ok()?.to_be_bytes());
    for field in fields {
        frame.extend_from_slice(&u64::try_from(field.len()).ok()?.to_be_bytes());
        frame.extend_from_slice(field.as_bytes());
    }
    Some(format!("{prefix}:{}", hb(domain, &frame)))
}
