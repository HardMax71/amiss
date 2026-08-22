use std::fs;
use std::io::Read as _;
use std::path::Path;

use amiss_wire::json::{self, Value};
use amiss_wire::report::MACHINE_JSON_BYTES;

/// The writer caps an envelope at `MACHINE_JSON_BYTES`, so a larger input
/// cannot be one of the scanner's artifacts.
pub(crate) fn strict_value(path: &Path) -> Result<Value, String> {
    let shown = path.display();
    let file = fs::File::open(path).map_err(|_error| format!("{shown} is unreadable"))?;
    let mut bytes = Vec::new();
    file.take(MACHINE_JSON_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_error| format!("{shown} is unreadable"))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MACHINE_JSON_BYTES {
        return Err(format!("{shown} is larger than a scanner report can be"));
    }
    json::parse(&bytes).map_err(|_error| format!("{shown} is not the scanner's strict JSON"))
}
