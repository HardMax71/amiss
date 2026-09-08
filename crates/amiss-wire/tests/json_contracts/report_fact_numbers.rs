use amiss_wire::{
    controls::{
        BlobLineSelection, ProjectionKind, ProjectionSink, ProjectionSource, TreePathSelection,
    },
    digest::sha256,
    model::RepoPathText,
    report::model::{
        ClaimKind, ClaimObserved, ControlStateSource, FindingAggregation, FindingFactEvidence,
        MissingResolution, ProjectionDifference, ProjectionObserved, RepoPath, ReportEnvelope,
        Resolution, RowsProjectionDifference,
    },
};

use super::report_numbers::assert_safe_numbers;

#[test]
fn finding_counts_are_bounded_in_each_evidence_variant() -> Result<(), Box<dyn std::error::Error>> {
    let digest = sha256(b"evidence");
    assert_safe_numbers(|number| ControlStateSource {
        digest,
        multiplicity: number,
    })?;
    let report: ReportEnvelope = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-report.canonical.json"
    ))?;
    let aggregation = report.payload.findings[0].aggregation;
    assert_safe_numbers(|number| FindingAggregation {
        locations_omitted: number,
        member_count: number,
        ..aggregation
    })?;
    let path = RepoPathText::new("docs/page.md".to_owned()).unwrap();
    assert_safe_numbers(|number| FindingFactEvidence::<RepoPath>::Claim {
        claim_kind: ClaimKind::Value,
        expected_digest: digest,
        line: number,
        name: "example".to_owned(),
        observed: ClaimObserved::LineDiffers,
        observed_digest: Some(digest),
        sources: Vec::new(),
        target_path: path.clone(),
    })?;
    assert_safe_numbers(|number| FindingFactEvidence::<RepoPath>::Reference {
        occurrence_multiplicity: number,
        resolution: Resolution::Missing(MissingResolution::LabelNotDeclared {}),
    })?;
    Ok(())
}

#[test]
fn projection_counts_and_sources_enforce_safe_integers() -> Result<(), Box<dyn std::error::Error>> {
    assert_safe_numbers(|number| RowsProjectionDifference {
        expected_records: number,
        extra_omitted: number,
        extra_preview: Vec::new(),
        extra_records: number,
        missing_omitted: number,
        missing_preview: Vec::new(),
        missing_records: number,
        observed_records: number,
        ordering_only: false,
    })?;
    assert_safe_numbers(
        |number| ProjectionDifference::<RowsProjectionDifference>::Count {
            expected_count: number,
            observed_count: Some(number),
        },
    )?;
    let path = RepoPathText::new("docs/page.md".to_owned()).unwrap();
    assert_safe_numbers(|number| BlobLineSelection {
        path: path.clone(),
        first_line: number,
        last_line: number,
    })?;
    assert_safe_numbers(|number| TreePathSelection {
        root: path.clone(),
        suffix: None,
        maximum_depth: number,
    })?;
    let count = ProjectionDifference::<RowsProjectionDifference>::Count {
        expected_count: 0,
        observed_count: None,
    };
    let text = serde_json::to_string(&count)?;
    assert_eq!(serde_json::from_str::<ProjectionDifference>(&text)?, count);
    let member = ",\"observed_count\":null";
    assert_eq!(text.matches(member).count(), 1);
    assert!(serde_json::from_str::<ProjectionDifference>(&text.replacen(member, "", 1)).is_err());
    Ok(())
}

#[test]
fn projection_sizes_keep_required_nullable_fields() -> Result<(), Box<dyn std::error::Error>> {
    let path = RepoPathText::new("docs/page.md".to_owned()).unwrap();
    let projection = |number| FindingFactEvidence::<RepoPath>::Projection {
        difference: None,
        expected_bytes: number,
        expected_digest: None,
        name: "example".to_owned(),
        observed: ProjectionObserved::ContentDiffers,
        observed_bytes: number,
        observed_digest: None,
        projection: ProjectionKind::CodeTextV1,
        sink: ProjectionSink::PreviousCode,
        source: ProjectionSource::BlobLines(BlobLineSelection {
            path: path.clone(),
            first_line: 1,
            last_line: 1,
        }),
        sources: Vec::new(),
    };
    assert_safe_numbers(|number| projection(Some(number)))?;
    let absent = projection(None);
    let text = serde_json::to_string(&absent)?;
    assert_eq!(serde_json::from_str::<FindingFactEvidence>(&text)?, absent);
    for member in ["\"expected_bytes\":null,", "\"observed_bytes\":null,"] {
        assert_eq!(text.matches(member).count(), 1);
        assert!(
            serde_json::from_str::<FindingFactEvidence>(&text.replacen(member, "", 1)).is_err()
        );
    }
    Ok(())
}
