mod tests;

use amiss_wire::json::Value;
use serde::Deserialize;

/// Selects unshaped HTTPS destinations and counts those beyond the run budget.
pub(crate) fn targets(plan: &Value, cap: usize) -> (Vec<&str>, usize) {
    let Ok(plan) = amiss_wire::codec::borrow_value::<Plan<'_>>("$", plan) else {
        return (Vec::new(), 0);
    };
    let mut probeable = plan
        .payload
        .introduced
        .into_iter()
        .filter(|row| row.scheme == "https" && !row.repository)
        .map(|row| row.destination);
    let selected = probeable.by_ref().take(cap).collect();
    let skipped = probeable.count();
    (selected, skipped)
}

#[derive(Deserialize)]
struct Plan<'a> {
    #[serde(borrow)]
    payload: Payload<'a>,
}

#[derive(Deserialize)]
struct Payload<'a> {
    #[serde(borrow)]
    introduced: Vec<ProbeCandidate<'a>>,
}

#[derive(Deserialize)]
struct ProbeCandidate<'a> {
    scheme: &'a str,
    destination: &'a str,
    #[serde(default, deserialize_with = "present")]
    repository: bool,
}

fn present<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    serde::de::IgnoredAny::deserialize(deserializer)?;
    Ok(true)
}
