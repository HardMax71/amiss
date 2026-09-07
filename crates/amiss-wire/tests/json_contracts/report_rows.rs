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
fn report_row_objects_reject_positional_forms() {
    let [report, incomplete] = reports().unwrap();
    let Feedback::Available(feedback) = &report.payload.feedback else {
        panic!("the row fixture has available feedback");
    };
    let annotation = feedback.items[0].annotation.as_ref().unwrap();
    let comparison = &report.payload.observations[0];
    let occurrence = comparison.candidate.as_ref().unwrap();
    let address = &occurrence.observation_id_input.structural_address;
    assert_ne!(annotation.span, occurrence.source_span);
    let fragments = [
        (
            serde_json::to_string(&report.payload.feedback).unwrap(),
            serde_json::to_string(&(feedback.existing_count, &feedback.items, feedback.status))
                .unwrap(),
        ),
        (
            serde_json::to_string(address).unwrap(),
            serde_json::to_string(&(
                address.address_kind,
                address.construct_index,
                address.duplicate_index,
                &address.node_path,
                address.schema,
            ))
            .unwrap(),
        ),
        (
            serde_json::to_string(&report.payload.documents[0].path).unwrap(),
            serde_json::to_string(&[hex::encode(b"docs/b\xff.md")]).unwrap(),
        ),
        (
            serde_json::to_string(&comparison.alternatives).unwrap(),
            serde_json::to_string(&(
                &comparison.alternatives.base,
                &comparison.alternatives.candidate,
            ))
            .unwrap(),
        ),
    ]
    .into_iter()
    .chain(
        [annotation.span, occurrence.source_span]
            .into_iter()
            .map(|span| {
                (
                    serde_json::to_string(&span).unwrap(),
                    serde_json::to_string(&(
                        span.end_byte,
                        span.end_column,
                        span.end_line,
                        span.start_byte,
                        span.start_column,
                        span.start_line,
                    ))
                    .unwrap(),
                )
            }),
    );
    let encoded = serde_json::to_string(&report).unwrap();
    for (object, sequence) in fragments {
        let altered = encoded.replace(&object, &sequence);
        assert_ne!(altered, encoded, "{object}");
        assert!(serde_json::from_str::<ReportEnvelope>(&altered).is_err());
    }
    let unavailable = serde_json::to_string(&[UnavailableStatus::Unavailable]).unwrap();
    let encoded = serde_json::to_string(&incomplete).unwrap();
    let object = serde_json::to_string(&incomplete.payload.feedback).unwrap();
    let altered = encoded.replace(&object, &unavailable);
    assert_ne!(altered, encoded);
    assert!(serde_json::from_str::<ReportEnvelope>(&altered).is_err());
}

#[test]
fn nullable_row_fields_remain_required() {
    let [mut report, _incomplete] = reports().unwrap();
    report.payload.documents[0].base = None;
    report.payload.documents[0].candidate = None;
    report.payload.observations[0].base = None;
    report.payload.observations[0].candidate = None;
    let Feedback::Available(feedback) = &mut report.payload.feedback else {
        panic!("the row fixture has available feedback");
    };
    feedback.items[0].annotation = None;
    feedback.items[0].target = None;
    let fragments = [
        (
            serde_json::to_string(&report.payload.documents[0]).unwrap(),
            ["base", "candidate"],
        ),
        (
            serde_json::to_string(&report.payload.observations[0]).unwrap(),
            ["base", "candidate"],
        ),
        (
            serde_json::to_string(&feedback.items[0]).unwrap(),
            ["annotation", "target"],
        ),
    ];
    let encoded = serde_json::to_string(&report).unwrap();
    assert_eq!(
        serde_json::from_str::<ReportEnvelope>(&encoded).unwrap(),
        report
    );
    for (fragment, fields) in fragments {
        for field in fields {
            let member = format!("\"{field}\":null");
            for invalid in [
                fragment.replace(&member, &format!("\"{field}\":{{}}")),
                fragment
                    .replace(&format!("{member},"), "")
                    .replace(&format!(",{member}"), ""),
            ] {
                assert_ne!(invalid, fragment, "{field}");
                let altered = encoded.replace(&fragment, &invalid);
                assert_ne!(altered, encoded, "{field}");
                assert!(
                    serde_json::from_str::<ReportEnvelope>(&altered).is_err(),
                    "{field}: {invalid}"
                );
            }
        }
    }
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
