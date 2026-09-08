#![cfg(test)]

use amiss_fixtures::captured_report;
use amiss_fixtures::feedback_report;
use amiss_wire::report::model::{
    Feedback, FeedbackAction, FeedbackItem, RepoPath, RepoPathBytes, ReportEnvelope,
    UnavailableFeedback, UnavailableStatus,
};
use amiss_wire::report::{Disposition, FindingKind};

use super::{feedback_lines, with_feedback};
use crate::{ArtifactReference, ExternalTally};

fn item(action: FeedbackAction, target: Option<RepoPath>, places: u64) -> FeedbackItem {
    FeedbackItem {
        action,
        annotation: None,
        effective_disposition: Disposition::Fail,
        finding_kinds: vec![FindingKind::ExplicitTargetMissing],
        location_count: std::num::NonZeroU64::new(places).unwrap(),
        target,
    }
}

#[test]
fn feedback_projects_counts_labels_and_atom_targets() {
    let captured = captured_report(
        feedback_report(
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
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        feedback_lines(Some(&captured), false),
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
    let captured = captured_report(
        feedback_report(
            0,
            vec![item(
                FeedbackAction::Fix,
                Some(RepoPath::Text(
                    "docs/\u{1b}[31m::error::x.md".parse().unwrap(),
                )),
                1,
            )],
        )
        .unwrap(),
    )
    .unwrap();
    let lines = feedback_lines(Some(&captured), false);
    let joined = lines.join("\n");
    assert!(!joined.contains('\u{1b}'), "raw ESC leaked: {joined:?}");
    assert!(
        joined.contains("\\u001b"),
        "the atom law spells the escape: {joined:?}"
    );
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
    let captured = captured_report(feedback_report(0, items).unwrap()).unwrap();
    let lines = feedback_lines(Some(&captured), false);
    assert_eq!(lines.len(), 12, "counts line, ten items, one overflow");
    assert_eq!(
        lines.last().map(String::as_str),
        Some("- 1 more item not displayed")
    );
    assert_eq!(
        feedback_lines(Some(&captured), true)
            .last()
            .map(String::as_str),
        Some("- 1 more item in the retained report")
    );
}

#[test]
fn unavailable_or_absent_feedback_adds_nothing() {
    assert_eq!(feedback_lines(None, false), Vec::<String>::new());
    let mut report: ReportEnvelope =
        serde_json::from_slice(amiss_fixtures::SCANNER_REPORT).unwrap();
    report.payload.feedback = Feedback::Unavailable(UnavailableFeedback {
        status: UnavailableStatus::Unavailable,
    });
    report.payload_digest = amiss_wire::digest::hb(
        amiss_wire::report::PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&report.payload).unwrap(),
    );
    let captured = captured_report(serde_json::to_vec(&report).unwrap()).unwrap();
    assert!(feedback_lines(Some(&captured), false).is_empty());
}

#[test]
fn feedback_keeps_the_original_report_bytes_for_artifact_binding() {
    let captured =
        captured_report(feedback_report(0, vec![item(FeedbackAction::Check, None, 1)]).unwrap())
            .unwrap();
    let report = &captured.envelope;
    let mut pretty = serde_json::to_vec_pretty(&report).unwrap();
    pretty.push(b'\n');
    assert_ne!(captured.bytes, pretty);
    let pretty = captured_report(pretty).unwrap();
    let artifact = ArtifactReference {
        id: "a".repeat(64),
        locator: "https://amiss.example/artifacts/fixture/report".to_owned(),
        expires_at_unix_millis: 1_800_000_000_000,
        report_digest: amiss_wire::digest::sha256(&pretty.bytes),
        semantic_digest: None,
        assessment_digest: None,
        external_tally: None,
        external_incomplete: false,
    };
    let summary = with_feedback("summary", Some(&pretty), Some(&artifact)).unwrap();
    assert!(summary.contains(&format!("report: {}", artifact.report_digest)));
    assert!(summary.contains("findings: fix 0, check 1, existing 0"));
    assert_eq!(
        with_feedback("summary", Some(&captured), Some(&artifact)),
        None
    );
}

#[test]
fn malformed_byte_targets_refuse_the_summary_even_outside_the_display_window() {
    for invalid in [
        String::new(),
        "gg".to_owned(),
        "f".to_owned(),
        "fG".to_owned(),
        "FF".to_owned(),
        "ff00".to_owned(),
        "ff5c".to_owned(),
        "2fff".to_owned(),
        "ff2f".to_owned(),
        "ff2f2fff".to_owned(),
        "2e2fff".to_owned(),
        "ff2f2e2e".to_owned(),
        "646f63732f612e6d64".to_owned(),
        "ff".repeat(4097),
    ] {
        for index in [0, 10] {
            let mut items = vec![item(FeedbackAction::Fix, None, 1); 11];
            items[index].target = Some(RepoPath::Bytes(RepoPathBytes {
                bytes_hex: invalid.clone(),
            }));
            let captured = captured_report(feedback_report(0, items).unwrap()).unwrap();
            assert!(
                feedback_lines(Some(&captured), false).is_empty(),
                "accepted byte target {invalid:?} at item {index}"
            );
            assert_eq!(
                with_feedback("summary", Some(&captured), None),
                Some(format!(
                    "summary\nreport: {}",
                    amiss_wire::digest::sha256(&captured.bytes)
                ))
            );
        }
    }
    let target = Some(RepoPath::Bytes(RepoPathBytes {
        bytes_hex: "ff".repeat(4096),
    }));
    let captured =
        captured_report(feedback_report(0, vec![item(FeedbackAction::Fix, target, 1)]).unwrap())
            .unwrap();
    assert_eq!(
        feedback_lines(Some(&captured), false),
        vec![
            "findings: fix 1, check 0, existing 0".to_owned(),
            format!(
                "- Fix target \"{}...\" affected places 1",
                "\\u00ff".repeat(200)
            ),
        ]
    );
}

#[test]
fn large_exact_counts_are_preserved() {
    let max_safe = 9_007_199_254_740_991;
    let captured = captured_report(
        feedback_report(max_safe, vec![item(FeedbackAction::Check, None, max_safe)]).unwrap(),
    )
    .unwrap();
    let expected = vec![
        "findings: fix 0, check 1, existing 9007199254740991".to_owned(),
        "- Check target - affected places 9007199254740991".to_owned(),
    ];
    assert_eq!(feedback_lines(Some(&captured), false), expected);
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
    let captured = captured_report(
        feedback_report(
            0,
            vec![item(
                FeedbackAction::Fix,
                Some(RepoPath::Text("docs/new.md".parse().unwrap())),
                1,
            )],
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        with_feedback("summary", Some(&captured), None),
        Some(format!(
            "summary\nreport: {}\nfindings: fix 1, check 0, existing 0\n\
             - Fix target \"docs/new.md\" affected places 1",
            amiss_wire::digest::sha256(&captured.bytes)
        ))
    );

    let id = "a".repeat(64);
    let artifact = ArtifactReference {
        id: id.clone(),
        locator: format!("https://amiss.example/artifacts/{id}/report"),
        expires_at_unix_millis: 1_800_000_000_000,
        report_digest: amiss_wire::digest::sha256(&captured.bytes),
        semantic_digest: Some(amiss_wire::digest::sha256(b"semantic input")),
        assessment_digest: None,
        external_tally: None,
        external_incomplete: false,
    };
    let projected = with_feedback("summary", Some(&captured), Some(&artifact)).unwrap();
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
    let projected = with_feedback("summary", Some(&captured), Some(&assessed)).unwrap();
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
    let projected = with_feedback("summary", Some(&captured), Some(&incomplete)).unwrap();
    assert!(
        projected.contains("external-assessment: incomplete"),
        "{projected}"
    );

    let mut mismatched = artifact;
    mismatched.report_digest = amiss_wire::digest::sha256(b"different");
    assert_eq!(
        with_feedback("summary", Some(&captured), Some(&mismatched)),
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
