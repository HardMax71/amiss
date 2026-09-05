#![cfg(test)]

use amiss_wire::report::model::{
    AvailableFeedback, AvailableFeedbackStatus, FeedbackAction, FeedbackItem, RepoPath,
    RepoPathBytes,
};
use amiss_wire::report::{Disposition, FindingKind};

use super::{FeedbackPayload, ReportFeedback, feedback_lines, with_feedback};
use crate::{ArtifactReference, ExternalTally};

fn report(existing_count: u64, items: Vec<FeedbackItem>) -> Vec<u8> {
    serde_json::to_vec(&ReportFeedback {
        payload: FeedbackPayload {
            feedback: AvailableFeedback {
                existing_count,
                items,
                status: AvailableFeedbackStatus::Available,
            },
        },
    })
    .unwrap()
}

fn item(action: FeedbackAction, target: Option<RepoPath>, places: u64) -> FeedbackItem {
    FeedbackItem {
        action,
        annotation: None,
        effective_disposition: Disposition::Fail,
        finding_kinds: vec![FindingKind::ExplicitTargetMissing],
        location_count: places,
        target,
    }
}

#[test]
fn feedback_projects_counts_labels_and_atom_targets() {
    let bytes = report(
        2,
        vec![
            item(
                FeedbackAction::Fix,
                Some(RepoPath::Text("docs/new.md".parse().unwrap())),
                1,
            ),
            item(
                FeedbackAction::Check,
                Some(RepoPath::Bytes(RepoPathBytes {
                    bytes_hex: "ff".to_owned(),
                })),
                2,
            ),
            item(FeedbackAction::Existing, None, 3),
            item(
                FeedbackAction::Fix,
                Some(RepoPath::Text("docs/second.md".parse().unwrap())),
                4,
            ),
        ],
    );
    assert_eq!(
        feedback_lines(Some(&bytes), false),
        vec![
            "findings: fix 2, check 1, existing 2".to_owned(),
            "- Fix target \"docs/new.md\" affected places 1".to_owned(),
            "- Check target \"\\u00ff\" affected places 2".to_owned(),
            "- Existing target - affected places 3".to_owned(),
            "- Fix target \"docs/second.md\" affected places 4".to_owned(),
        ]
    );
}

#[test]
fn a_hostile_target_cannot_carry_control_bytes_into_provider_markdown() {
    let bytes = report(
        0,
        vec![item(
            FeedbackAction::Fix,
            Some(RepoPath::Text(
                "docs/\u{1b}[31m::error::x.md".parse().unwrap(),
            )),
            1,
        )],
    );
    let lines = feedback_lines(Some(&bytes), false);
    let joined = lines.join("\n");
    assert!(!joined.contains('\u{1b}'), "raw ESC leaked: {joined:?}");
    assert!(
        joined.contains("\\u001b"),
        "the atom law spells the escape: {joined:?}"
    );

    let mut forged: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    forged["payload"]["feedback"]["items"][0]["action"] = serde_json::json!("fix\n\n- [x] done");
    assert!(feedback_lines(Some(&serde_json::to_vec(&forged).unwrap()), false).is_empty());
}

#[test]
fn eleven_items_show_ten_and_one_overflow_line() {
    let items = (0..11)
        .map(|index| {
            item(
                FeedbackAction::Fix,
                Some(RepoPath::Text(
                    format!("docs/absent-{index}.md").parse().unwrap(),
                )),
                1,
            )
        })
        .collect();
    let bytes = report(0, items);
    let lines = feedback_lines(Some(&bytes), false);
    assert_eq!(lines.len(), 12, "counts line, ten items, one overflow");
    assert_eq!(
        lines.last().map(String::as_str),
        Some("- 1 more item not displayed")
    );
    assert_eq!(
        feedback_lines(Some(&bytes), true)
            .last()
            .map(String::as_str),
        Some("- 1 more item in the retained report")
    );
}

#[test]
fn unreadable_or_absent_feedback_adds_nothing() {
    assert_eq!(feedback_lines(None, false), Vec::<String>::new());
    assert_eq!(
        feedback_lines(Some(b"not json"), false),
        Vec::<String>::new()
    );
    assert_eq!(
        feedback_lines(Some(br#"{"schema":"amiss/report"}"#), false),
        Vec::<String>::new()
    );
    let unavailable = br#"{"payload":{"feedback":{"status":"unavailable"}}}"#;
    assert_eq!(
        feedback_lines(Some(unavailable), false),
        Vec::<String>::new()
    );
}

#[test]
fn malformed_feedback_cannot_turn_into_plausible_counts_or_labels() {
    let bytes = report(0, vec![item(FeedbackAction::Fix, None, 1)]);
    for (path, invalid) in [
        ("/payload/feedback/status", serde_json::json!("future")),
        ("/payload/feedback/items", serde_json::Value::Null),
        ("/payload/feedback/existing_count", serde_json::json!(-1)),
        (
            "/payload/feedback/items/0/action",
            serde_json::json!("fixme"),
        ),
        (
            "/payload/feedback/items/0/location_count",
            serde_json::json!(-1),
        ),
        ("/payload/feedback/items/0/target", serde_json::json!(42)),
    ] {
        let mut changed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        *changed.pointer_mut(path).unwrap() = invalid;
        assert!(
            feedback_lines(Some(&serde_json::to_vec(&changed).unwrap()), false).is_empty(),
            "{path}"
        );
    }
    let mut missing: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    missing["payload"]["feedback"]["items"][0]
        .as_object_mut()
        .unwrap()
        .remove("target");
    assert!(feedback_lines(Some(&serde_json::to_vec(&missing).unwrap()), false).is_empty());
}

#[test]
fn the_strict_profile_also_covers_fields_the_projection_ignores() {
    let bytes = report(0, vec![item(FeedbackAction::Fix, None, 1)]);
    let text = std::str::from_utf8(&bytes).unwrap();
    let prefix = text.strip_suffix('}').unwrap();
    for invalid in [
        r#"{"duplicate":0,"duplicate":1}"#,
        "9007199254740992",
        "0.5",
    ] {
        let invalid = format!("{prefix},\"future\":{invalid}}}");
        assert!(
            feedback_lines(Some(invalid.as_bytes()), false).is_empty(),
            "{invalid}"
        );
    }
    assert!(feedback_lines(Some(format!("{text} null").as_bytes()), false).is_empty());
    let mut invalid_utf8 = bytes;
    invalid_utf8.push(0xff);
    assert!(feedback_lines(Some(&invalid_utf8), false).is_empty());
}

#[test]
fn additive_fields_and_large_exact_counts_preserve_the_projection() {
    let max_safe = 9_007_199_254_740_991;
    let bytes = report(max_safe, vec![item(FeedbackAction::Check, None, max_safe)]);
    let expected = vec![
        "findings: fix 0, check 1, existing 9007199254740991".to_owned(),
        "- Check target - affected places 9007199254740991".to_owned(),
    ];
    assert_eq!(feedback_lines(Some(&bytes), false), expected);
    let mut extended: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for path in [
        "",
        "/payload",
        "/payload/feedback",
        "/payload/feedback/items/0",
    ] {
        extended.pointer_mut(path).unwrap()["future"] =
            serde_json::json!({"data": [1, true, null]});
    }
    assert_eq!(
        feedback_lines(Some(&serde_json::to_vec(&extended).unwrap()), false),
        expected
    );
    let nested = format!("{}0{}", "[".repeat(256), "]".repeat(256));
    let extended = std::str::from_utf8(&bytes).unwrap().replace(
        "\"status\":\"available\"",
        &format!("\"status\":\"available\",\"future\":{nested}"),
    );
    assert!(amiss_wire::json::parse(extended.as_bytes()).is_ok());
    assert_eq!(feedback_lines(Some(extended.as_bytes()), false), expected);
}

#[test]
fn with_feedback_appends_below_the_text_or_leaves_it_alone() {
    assert_eq!(
        with_feedback("summary", None, None),
        Some(format!(
            "summary\nreport: {}",
            amiss_wire::digest::sha256(&[])
        ))
    );
    let bytes = report(
        0,
        vec![item(
            FeedbackAction::Fix,
            Some(RepoPath::Text("docs/new.md".parse().unwrap())),
            1,
        )],
    );
    assert_eq!(
        with_feedback("summary", Some(&bytes), None),
        Some(format!(
            "summary\nreport: {}\nfindings: fix 1, check 0, existing 0\n\
             - Fix target \"docs/new.md\" affected places 1",
            amiss_wire::digest::sha256(&bytes)
        ))
    );

    let id = "a".repeat(64);
    let artifact = ArtifactReference {
        id: id.clone(),
        locator: format!("https://amiss.example/artifacts/{id}/report"),
        expires_at_unix_millis: 1_800_000_000_000,
        report_digest: amiss_wire::digest::sha256(&bytes),
        semantic_digest: Some(amiss_wire::digest::sha256(b"semantic input")),
        assessment_digest: None,
        external_tally: None,
        external_incomplete: false,
    };
    let projected = with_feedback("summary", Some(&bytes), Some(&artifact)).unwrap();
    assert!(projected.contains(&format!("artifact: {}", artifact.locator)));
    assert!(projected.contains("artifact-auth: bearer"));
    assert!(projected.contains("artifact-expires-unix-millis: 1800000000000"));
    assert!(projected.contains(&format!(
        "semantic-input: {}",
        amiss_wire::digest::sha256(b"semantic input")
    )));
    assert!(projected.contains(&format!(
        "semantic-input-artifact: https://amiss.example/artifacts/{id}/semantic"
    )));

    let mut assessed = artifact.clone();
    assessed.assessment_digest = Some(amiss_wire::digest::sha256(b"assessment"));
    assessed.external_tally = Some(ExternalTally {
        refuted: 1,
        unproven: 2,
        reachable: 3,
    });
    let projected = with_feedback("summary", Some(&bytes), Some(&assessed)).unwrap();
    assert!(
        projected.contains("external-assessment: refuted 1 unproven 2 reachable 3"),
        "{projected}"
    );
    assert!(
        projected.contains(&format!(
            "assessment-artifact: https://amiss.example/artifacts/{id}/assessment"
        )),
        "{projected}"
    );

    let mut incomplete = artifact.clone();
    incomplete.external_incomplete = true;
    let projected = with_feedback("summary", Some(&bytes), Some(&incomplete)).unwrap();
    assert!(
        projected.contains("external-assessment: incomplete"),
        "{projected}"
    );

    let mut mismatched = artifact;
    mismatched.report_digest = amiss_wire::digest::sha256(b"different");
    assert_eq!(
        with_feedback("summary", Some(&bytes), Some(&mismatched)),
        None
    );
}

#[test]
fn provider_feedback_accepts_only_the_previous_additive_projection() {
    let expected = "binding: exact\nsemantic-input: sha256:aaaa\n\
                    semantic-input-artifact: https://amiss.example/a/semantic\n\
                    assessment: sha256:bbbb\n\
                    assessment-artifact: https://amiss.example/a/assessment\n\
                    external-assessment: incomplete\nfindings: none";
    let retention_only = "binding: exact\nassessment: sha256:bbbb\n\
                          assessment-artifact: https://amiss.example/a/assessment\n\
                          external-assessment: incomplete\nfindings: none";
    let previous = "binding: exact\nassessment: sha256:bbbb\nfindings: none";

    assert!(super::compatible_provider_feedback(expected, expected));
    assert!(super::compatible_provider_feedback(
        retention_only,
        expected
    ));
    assert!(super::compatible_provider_feedback(previous, expected));
    assert!(!super::compatible_provider_feedback(
        "binding: changed\nassessment: sha256:bbbb\nfindings: none",
        expected
    ));
}
