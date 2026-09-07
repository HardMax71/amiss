use serde::Serialize;

use crate::de::{Error, ErrorKind, fail};

/// Writes a canonical JSON artifact within the caller's encoded-byte ceiling.
/// A sink checks the same ceiling without retaining the output bytes.
///
/// # Errors
///
/// Fails on serialization or output errors, or when the encoded artifact exceeds the ceiling.
/// The destination may already contain partial or oversized output when an error is returned.
pub fn write_json<T: Serialize>(
    document: &T,
    writer: impl std::io::Write,
    limit: u64,
) -> Result<(), Error> {
    let mut writer = countio::Counter::new(writer);
    serde_json_canonicalizer::to_writer(document, &mut writer)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    if u64::try_from(writer.writer_bytes()).unwrap_or(u64::MAX) > limit {
        return fail("$", ErrorKind::LimitExceeded);
    }
    Ok(())
}
