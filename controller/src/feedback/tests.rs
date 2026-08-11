#![cfg(test)]

use super::{feedback_lines, with_feedback};

fn report(feedback: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "payload": { "feedback": feedback },
        "schema": "amiss/scanner-report-envelope"
    }))
    .unwrap_or_default()
}

fn item(action: &str, target: &serde_json::Value, places: u64) -> serde_json::Value {
    serde_json::json!({
        "action": action,
        "annotation": null,
        "effective_disposition": "fail",
        "finding_kinds": ["explicit-target-missing"],
        "location_count": places,
        "target": target
    })
}

#[test]
fn feedback_projects_counts_labels_and_atom_targets() {
    let bytes = report(&serde_json::json!({
        "existing_count": 2,
        "items": [
            item("fix", &serde_json::json!("docs/new.md"), 1),
            item("check", &serde_json::json!({ "bytes_hex": "ff" }), 2),
            item("existing", &serde_json::Value::Null, 3),
        ],
        "status": "available"
    }));
    assert_eq!(
        feedback_lines(Some(&bytes)),
        vec![
            "findings: fix 1, check 1, existing 2".to_owned(),
            "- Fix target \"docs/new.md\" affected places 1".to_owned(),
            "- Check target \"\\u00ff\" affected places 2".to_owned(),
            "- Existing target - affected places 3".to_owned(),
        ]
    );
}

#[test]
fn a_hostile_target_cannot_carry_control_bytes_into_provider_markdown() {
    let bytes = report(&serde_json::json!({
        "existing_count": 0,
        "items": [item("fix", &serde_json::json!("docs/\u{1b}[31m::error::x.md"), 1)],
        "status": "available"
    }));
    let lines = feedback_lines(Some(&bytes));
    let joined = lines.join("\n");
    assert!(!joined.contains('\u{1b}'), "raw ESC leaked: {joined:?}");
    assert!(
        joined.contains("\\u001b"),
        "the atom law spells the escape: {joined:?}"
    );

    let forged = report(&serde_json::json!({
        "existing_count": 0,
        "items": [item("fix\n\n- [x] done", &serde_json::json!("docs/new.md"), 1)],
        "status": "available"
    }));
    let lines = feedback_lines(Some(&forged));
    assert_eq!(
        lines.get(1).map(String::as_str),
        Some("- Fix-xdone target \"docs/new.md\" affected places 1"),
        "a forged action word is constrained to word characters: {lines:?}"
    );
}

#[test]
fn eleven_items_show_ten_and_one_overflow_line() {
    let items: Vec<serde_json::Value> = (0..11)
        .map(|index| {
            item(
                "fix",
                &serde_json::json!(format!("docs/absent-{index}.md")),
                1,
            )
        })
        .collect();
    let bytes = report(&serde_json::json!({
        "existing_count": 0,
        "items": items,
        "status": "available"
    }));
    let lines = feedback_lines(Some(&bytes));
    assert_eq!(lines.len(), 12, "counts line, ten items, one overflow");
    assert_eq!(
        lines.last().map(String::as_str),
        Some("- 1 more item in the report")
    );
}

#[test]
fn unreadable_or_absent_feedback_adds_nothing() {
    assert_eq!(feedback_lines(None), Vec::<String>::new());
    assert_eq!(feedback_lines(Some(b"not json")), Vec::<String>::new());
    assert_eq!(
        feedback_lines(Some(br#"{"schema":"amiss/report"}"#)),
        Vec::<String>::new()
    );
    let unavailable = report(&serde_json::json!({ "status": "unavailable" }));
    assert_eq!(feedback_lines(Some(&unavailable)), Vec::<String>::new());
}

#[test]
fn with_feedback_appends_below_the_text_or_leaves_it_alone() {
    assert_eq!(with_feedback("report: x".to_owned(), None), "report: x");
    let bytes = report(&serde_json::json!({
        "existing_count": 0,
        "items": [item("fix", &serde_json::json!("docs/new.md"), 1)],
        "status": "available"
    }));
    assert_eq!(
        with_feedback("report: x".to_owned(), Some(&bytes)),
        "report: x\nfindings: fix 1, check 0, existing 0\n\
         - Fix target \"docs/new.md\" affected places 1"
    );
}
