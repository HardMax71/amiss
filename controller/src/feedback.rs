mod tests;

use amiss_wire::human::{atom, atom_bytes, decode_hex};
use amiss_wire::json::{self, Value};

const DISPLAYED_ITEMS: usize = 10;

/// Every repository-derived value passes the human-atom law before it
/// reaches provider markdown.
#[must_use]
pub fn with_feedback(text: String, report: Option<&[u8]>) -> String {
    let lines = feedback_lines(report);
    if lines.is_empty() {
        text
    } else {
        format!("{text}\n{}", lines.join("\n"))
    }
}

fn feedback_lines(report: Option<&[u8]>) -> Vec<String> {
    let Some(bytes) = report else {
        return Vec::new();
    };
    let Ok(envelope) = json::parse(bytes) else {
        return Vec::new();
    };
    let Some(feedback) =
        member(&envelope, "payload").and_then(|payload| member(payload, "feedback"))
    else {
        return Vec::new();
    };
    if member(feedback, "status").and_then(as_text) != Some("available") {
        return Vec::new();
    }
    let items = match member(feedback, "items") {
        Some(Value::Array(rows)) => rows.as_slice(),
        Some(
            Value::Null | Value::Bool(_) | Value::Integer(_) | Value::String(_) | Value::Object(_),
        )
        | None => &[],
    };
    let fixes = items.iter().filter(|item| action_is(item, "fix")).count();
    let checks = items.iter().filter(|item| action_is(item, "check")).count();
    let existing = member(feedback, "existing_count")
        .and_then(as_integer)
        .unwrap_or(0);
    let mut lines = vec![format!(
        "findings: fix {fixes}, check {checks}, existing {existing}"
    )];
    lines.extend(items.iter().take(DISPLAYED_ITEMS).map(item_line));
    let overflow = items.len().saturating_sub(DISPLAYED_ITEMS);
    if overflow == 1 {
        lines.push("- 1 more item in the report".to_owned());
    } else if overflow > 1 {
        lines.push(format!("- {overflow} more items in the report"));
    }
    lines
}

fn item_line(item: &Value) -> String {
    let mut action = member(item, "action")
        .and_then(as_text)
        .unwrap_or_default()
        .to_owned();
    action.retain(|symbol| symbol.is_ascii_alphanumeric() || symbol == '-');
    if let Some(first) = action.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    let places = member(item, "location_count")
        .and_then(as_integer)
        .unwrap_or(0);
    format!(
        "- {action} target {} affected places {places}",
        target_atom(item)
    )
}

fn target_atom(item: &Value) -> String {
    match member(item, "target") {
        Some(Value::String(path)) => atom(path),
        Some(Value::Object(members)) => {
            if let [(key, Value::String(hex))] = members.as_slice()
                && key == "bytes_hex"
            {
                atom_bytes(&decode_hex(hex))
            } else {
                "-".to_owned()
            }
        }
        Some(Value::Null | Value::Bool(_) | Value::Integer(_) | Value::Array(_)) | None => {
            "-".to_owned()
        }
    }
}

fn action_is(item: &Value, action: &str) -> bool {
    member(item, "action").and_then(as_text) == Some(action)
}

fn member<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    if let Value::Object(members) = value {
        members
            .iter()
            .find(|(key, _value)| key == name)
            .map(|(_key, value)| value)
    } else {
        None
    }
}

fn as_text(value: &Value) -> Option<&str> {
    if let Value::String(text) = value {
        Some(text.as_str())
    } else {
        None
    }
}

fn as_integer(value: &Value) -> Option<i64> {
    if let Value::Integer(number) = value {
        Some(*number)
    } else {
        None
    }
}
