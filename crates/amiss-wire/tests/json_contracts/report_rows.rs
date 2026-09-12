use amiss_wire::report::{
    AnalysisErrorCode,
    model::{
        AnalysisError, AnalysisPhase, Feedback, RepoPath, RepoPathBytes, ReportEnvelope,
        ReportStatus, UnavailableFeedback, UnavailableStatus,
    },
};

#[expect(
    clippy::panic,
    clippy::indexing_slicing,
    reason = "the published report has a document and available feedback items"
)]
pub(super) fn reports() -> serde_json::Result<[ReportEnvelope; 2]> {
    let mut report: ReportEnvelope = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-report.canonical.json"
    ))?;
    let path = RepoPath::Bytes(RepoPathBytes {
        bytes_hex: hex::encode(b"docs/b\xff.md"),
    });
    report.payload.documents[0].path = path.clone();
    let Feedback::Available(feedback) = &mut report.payload.feedback else {
        panic!("the report fixture has available feedback");
    };
    feedback.items[0].target = Some(path.clone());
    let available = report.clone();
    report.payload.errors.push(AnalysisError {
        code: AnalysisErrorCode::InvalidUtf8,
        configured_limit: None,
        description: AnalysisErrorCode::InvalidUtf8.meaning().to_owned(),
        observed_lower_bound: None,
        path: Some(path),
        path_bytes_hex: None,
        phase: AnalysisPhase::Configuration,
        resource: None,
    });
    report.payload.result.complete = false;
    report.payload.result.error_count = 1;
    report.payload.result.exit_code = 2;
    report.payload.result.status = ReportStatus::Incomplete;
    report.payload.summary.counts_complete = false;
    report.payload.summary.findings.analysis_errors = 1;
    report.payload.feedback = Feedback::Unavailable(UnavailableFeedback {
        status: UnavailableStatus::Unavailable,
    });
    Ok([available, report])
}

#[test]
fn unavailable_feedback_cannot_hide_available_items() {
    let [report, _incomplete] = reports().unwrap();
    let feedback = serde_json::to_string(&report.payload.feedback).unwrap();
    let available =
        serde_json::to_string(&amiss_wire::report::model::AvailableFeedbackStatus::Available)
            .unwrap();
    let unavailable = serde_json::to_string(&UnavailableStatus::Unavailable).unwrap();
    let invalid = feedback.replace(&available, &unavailable);
    assert_ne!(feedback, invalid);
    assert!(serde_json::from_str::<Feedback>(&invalid).is_err());
}
