mod tests;

use amiss_wire::json::Value;

/// The probeable introduced destinations: https, not shaped as a forge
/// repository since the API verifiers own those, capped at the run budget.
/// Returns the selection and how many probeable rows fell past the cap.
pub(crate) fn targets(plan: &Value, cap: usize) -> (Vec<&str>, usize) {
    let introduced = plan
        .member("payload")
        .and_then(|payload| payload.member("introduced"));
    let Some(Value::Array(introduced)) = introduced else {
        return (Vec::new(), 0);
    };
    let probeable: Vec<&str> = introduced
        .iter()
        .filter(|row| row.text("scheme") == Some("https") && row.member("repository").is_none())
        .filter_map(|row| row.text("destination"))
        .collect();
    let skipped = probeable.len().saturating_sub(cap);
    let mut selected = probeable;
    selected.truncate(cap);
    (selected, skipped)
}
