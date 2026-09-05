#![cfg(test)]

use amiss_wire::model::RepoPathText;
use amiss_wire::report::model::{RepoPath, RepoPathBytes, ReportEnvelope};
use amiss_wire::report::{Disposition, FindingKind};

use super::issues;

#[test]
fn a_global_finding_yields_a_valid_placeholder_location() {
    let mut report: ReportEnvelope =
        serde_json::from_slice(amiss_fixtures::SCANNER_REPORT).unwrap();
    report.payload.findings.truncate(1);
    let finding = &mut report.payload.findings[0];
    finding.kind = FindingKind::PolicyWeakened;
    finding.description = "a control sentence".to_owned();
    finding.effective_disposition = Disposition::Fail;
    finding.location.path = None;
    finding.location.span = None;
    let fingerprint = finding.finding_key.to_string();

    let projected = issues(&report.payload);
    let bytes = serde_json::to_vec(&projected).unwrap();
    assert_eq!(bytes, serde_json_canonicalizer::to_vec(&projected).unwrap());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        serde_json::json!([{
            "check_name": "policy-weakened",
            "description": "a control sentence",
            "fingerprint": fingerprint,
            "location": {"lines": {"begin": 1}, "path": "(global)"},
            "severity": "major"
        }])
    );
}

#[test]
fn paths_and_dispositions_keep_their_projection_without_owned_json_rows() {
    let mut report: ReportEnvelope =
        serde_json::from_slice(amiss_fixtures::SCANNER_REPORT).unwrap();
    report.payload.findings.truncate(1);
    for (path, line, disposition, expected_path, expected_line, severity) in [
        (
            RepoPath::Text(RepoPathText::new("docs/a\"b\n.md".to_owned()).unwrap()),
            7,
            Disposition::Warn,
            "docs/a\"b\n.md",
            7,
            "minor",
        ),
        (
            RepoPath::Bytes(RepoPathBytes {
                bytes_hex: "646f63732fff2e6d64".to_owned(),
            }),
            0,
            Disposition::Record,
            "646f63732fff2e6d64",
            1,
            "info",
        ),
    ] {
        let finding = &mut report.payload.findings[0];
        finding.location.path = Some(path);
        finding.location.span.as_mut().unwrap().start_line = line;
        finding.effective_disposition = disposition;
        finding.description = "a \"missing\" target\n".to_owned();
        let projected = issues(&report.payload);
        let bytes = serde_json::to_vec(&projected).unwrap();
        assert_eq!(bytes, serde_json_canonicalizer::to_vec(&projected).unwrap());
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value[0]["location"]["path"], expected_path);
        assert_eq!(value[0]["location"]["lines"]["begin"], expected_line);
        assert_eq!(value[0]["severity"], severity);
        assert_eq!(value[0]["description"], "a \"missing\" target\n");
    }
}
