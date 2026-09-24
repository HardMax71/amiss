#![cfg(test)]

use amiss_wire::controls::AnalysisPhase;
use amiss_wire::model::RepoPath;
use amiss_wire::model::RepoPathText;
use amiss_wire::repo_path_text;
use amiss_wire::report::model::{
    AnalysisError, ByteSpan, FindingFix, ReportEnvelope, ReportStatus, SourceSpan,
};
use amiss_wire::report::{Disposition, FindingKind, model::AnalysisErrorCode};

fn projection_payload() -> amiss_wire::report::model::ReportPayload {
    let mut report: ReportEnvelope =
        serde_json::from_slice(amiss_fixtures::SCANNER_REPORT).unwrap();
    let payload = &mut report.payload;
    payload.result.complete = false;
    payload.result.exit_code = 2;
    payload.result.status = ReportStatus::Incomplete;
    payload.errors = vec![AnalysisError {
        code: AnalysisErrorCode::GitObjectUnreadable,
        description: "could not read".to_owned(),
        configured_limit: None,
        observed_lower_bound: None,
        path: None,
        path_bytes: None,
        phase: AnalysisPhase::Git,
        resource: None,
    }];
    let mut first = payload.findings[0].clone();
    first.kind = FindingKind::ExplicitTargetMissing;
    first.description = "missing \"target\"\n".to_owned();
    first.effective_disposition = Disposition::Fail;
    first.location.path = Some(RepoPath::from(&repo_path_text!("docs/a b.md")));
    first.location.span = Some(SourceSpan {
        start_byte: 4,
        end_byte: 7,
        start_line: 2,
        start_column: 3,
        end_line: 2,
        end_column: 8,
    });
    first.fix = Some(FindingFix {
        path: repo_path_text!("docs/a b.md"),
        description: "repair".to_owned(),
        replacement: String::new(),
        span: ByteSpan {
            start_byte: 4,
            end_byte: 7,
        },
    });
    let mut byte_path = first.clone();
    byte_path.effective_disposition = Disposition::Warn;
    byte_path.location.path = Some(RepoPath::from_bytes(b"\xff.md".to_vec()).unwrap());
    byte_path.location.span = None;
    byte_path.fix = None;
    let mut without_span = first.clone();
    without_span.effective_disposition = Disposition::Record;
    without_span.location.path = Some(RepoPath::from(&repo_path_text!("docs/b.md")));
    without_span.location.span = None;
    without_span.fix = None;
    payload.findings = vec![first, byte_path, without_span];
    report.payload
}

#[test]
fn typed_sarif_preserves_optional_fields_and_canonical_order() {
    let payload = projection_payload();
    let fingerprint = payload.findings[0].finding_key.to_string();
    let log = super::log(&payload, RepoPath::as_str);
    let bytes = serde_json::to_vec(&log).unwrap();
    assert_eq!(bytes, serde_json_canonicalizer::to_vec(&log).unwrap());
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let run = &value["runs"][0];
    let invocation = &run["invocations"][0];
    assert_eq!(invocation["executionSuccessful"], false);
    assert_eq!(invocation["exitCode"], 2);
    assert_eq!(
        invocation["toolExecutionNotifications"][0]["descriptor"]["id"],
        "GIT_OBJECT_UNREADABLE"
    );
    let first = &run["results"][0];
    assert_eq!(first["level"], "error");
    assert_eq!(first["ruleIndex"], 0);
    assert_eq!(first["message"]["text"], "missing \"target\"\n");
    assert_eq!(
        first["partialFingerprints"]["amissFindingKey/v1"],
        fingerprint
    );
    let location = &first["locations"][0]["physicalLocation"];
    assert_eq!(location["artifactLocation"]["uri"], "docs/a%20b.md");
    assert_eq!(
        location["region"],
        serde_json::json!({
            "endColumn": 8, "endLine": 2, "startColumn": 3, "startLine": 2
        })
    );
    let replacement = &first["fixes"][0]["artifactChanges"][0]["replacements"][0];
    assert_eq!(
        replacement["deletedRegion"],
        serde_json::json!({"byteLength": 3, "byteOffset": 4})
    );
    assert_eq!(replacement["insertedContent"]["text"], "");
    let byte_path = &run["results"][1];
    assert_eq!(byte_path["level"], "warning");
    assert_eq!(byte_path["ruleIndex"], 0);
    for omitted in ["locations", "fixes"] {
        assert!(byte_path.get(omitted).is_none(), "{omitted}");
    }
    let without_span = &run["results"][2];
    assert_eq!(without_span["level"], "note");
    assert_eq!(
        without_span["locations"],
        serde_json::json!([{
            "physicalLocation": {
                "artifactLocation": {"uri": "docs/b.md"}
            }
        }])
    );
    assert!(without_span.get("fixes").is_none());
}

#[test]
fn sarif_paths_escape_uri_delimiters_and_utf8_in_locations_and_fixes() {
    for (path, expected) in [
        ("docs/a-._~/file.md", "docs/a-._~/file.md"),
        ("docs/a?#%é😀.md", "docs/a%3F%23%25%C3%A9%F0%9F%98%80.md"),
        ("docs/%20\u{1b}.md", "docs/%2520%1B.md"),
    ] {
        let mut payload = projection_payload();
        let path = RepoPathText::try_from(path.to_owned()).unwrap();
        let finding = &mut payload.findings[0];
        finding.location.path = Some(RepoPath::from(&path.clone()));
        finding.fix.as_mut().unwrap().path = path;
        let log = super::log(&payload, RepoPath::as_str);
        let value = serde_json::to_value(log).unwrap();
        let finding = &value["runs"][0]["results"][0];
        assert_eq!(
            finding["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            expected
        );
        assert_eq!(
            finding["fixes"][0]["artifactChanges"][0]["artifactLocation"]["uri"],
            expected
        );
    }
}
