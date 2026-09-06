use std::fs;
use std::io::Read as _;
use std::path::Path;

use amiss_wire::json::{self, Value};
use amiss_wire::report::MACHINE_JSON_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadError {
    Unreadable,
    TooLarge,
}

pub(crate) struct StrictJson {
    pub bytes: Vec<u8>,
    pub value: Value,
}

pub(crate) fn bounded_bytes(path: &Path, limit: u64) -> Result<Vec<u8>, ReadError> {
    let file = fs::File::open(path).map_err(|_error| ReadError::Unreadable)?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_error| ReadError::Unreadable)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
        Err(ReadError::TooLarge)
    } else {
        Ok(bytes)
    }
}

/// The writer caps an envelope at `MACHINE_JSON_BYTES`, so a larger input
/// cannot be one of the scanner's artifacts.
pub(crate) fn report_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let shown = path.display();
    bounded_bytes(path, MACHINE_JSON_BYTES).map_err(|error| match error {
        ReadError::Unreadable => format!("{shown} is unreadable"),
        ReadError::TooLarge => format!("{shown} is larger than a scanner report can be"),
    })
}

pub(crate) fn strict_json(path: &Path) -> Result<StrictJson, String> {
    let bytes = report_bytes(path)?;
    let value = json::parse(&bytes)
        .map_err(|_error| format!("{} is not the scanner's strict JSON", path.display()))?;
    Ok(StrictJson { bytes, value })
}
