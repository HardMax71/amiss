mod tests;

use amiss_wire::external::ExternalPlanEnvelope;

/// The probeable introduced destinations: https, not shaped as a forge
/// repository since the API verifiers own those, capped at the run budget.
/// Returns the selection and how many probeable rows fell past the cap.
pub(crate) fn targets(plan: &ExternalPlanEnvelope, cap: usize) -> (Vec<&str>, usize) {
    let probeable: Vec<&str> = plan
        .payload
        .introduced
        .iter()
        .filter(|row| row.scheme == "https" && row.repository.is_none())
        .map(|row| row.destination.as_str())
        .collect();
    let skipped = probeable.len().saturating_sub(cap);
    let mut selected = probeable;
    selected.truncate(cap);
    (selected, skipped)
}
