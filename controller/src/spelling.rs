mod tests;

use percent_encoding::percent_decode_str;

/// The tail of a forge URL in its own spelling: split on the slashes the
/// URL wrote, then each segment's percent-escapes decoded exactly once, so
/// an escaped slash stays inside its segment and an escaped percent stays
/// literal. `None` when an escape names bytes outside UTF-8, which no
/// forge object answers to; guessing a spelling there could refute a live
/// URL.
#[must_use]
pub fn spelled_segments(tail: &str) -> Option<Vec<String>> {
    let tail = tail.strip_suffix('/').unwrap_or(tail);
    tail.split('/')
        .map(|segment| {
            percent_decode_str(segment)
                .decode_utf8()
                .ok()
                .map(std::borrow::Cow::into_owned)
        })
        .collect()
}

/// How many whole leading segments the candidate ref covers. A ref may
/// span segments the URL separated or sit inside one segment whose escaped
/// slash the decode revealed, but its end always falls on a segment
/// boundary, since only the forge knows how it reads a boundary the URL
/// did not write.
#[must_use]
pub fn ref_span(segments: &[String], candidate: &str) -> Option<usize> {
    let mut remainder = candidate;
    for (index, segment) in segments.iter().enumerate() {
        remainder = remainder.strip_prefix(segment.as_str())?;
        if remainder.is_empty() {
            return index.checked_add(1);
        }
        remainder = remainder.strip_prefix('/')?;
    }
    None
}
