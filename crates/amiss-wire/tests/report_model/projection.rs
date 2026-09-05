use amiss_wire::report::model::ProjectionObserved;
use strum::IntoEnumIterator;

#[test]
fn projection_reason_names_are_one_closed_serde_vocabulary() -> Result<(), serde_json::Error> {
    let expected = [
        "content-differs",
        "sink-absent",
        "sink-ambiguous",
        "sink-document-unavailable",
        "sink-not-adjacent",
        "source-absent",
        "source-end-marker-absent",
        "source-end-marker-ambiguous",
        "source-lfs-pointer",
        "source-lines-out-of-range",
        "source-not-a-blob",
        "source-record-absent",
        "source-record-set-absent",
        "source-record-set-incomplete",
        "source-record-unproven",
        "source-region-not-utf8",
        "source-region-order-invalid",
        "source-start-marker-absent",
        "source-start-marker-ambiguous",
        "source-tree-path-not-a-row",
        "source-tree-path-not-utf8",
        "source-tree-root-absent",
        "source-tree-root-not-a-tree",
    ];
    let reasons: Vec<_> = ProjectionObserved::iter().collect();
    assert_eq!(reasons.len(), expected.len());
    for (reason, expected) in reasons.into_iter().zip(expected) {
        assert_eq!(reason.as_ref(), expected);
        let encoded = serde_json::to_string(&reason)?;
        assert_eq!(encoded, format!("\"{expected}\""));
        assert_eq!(
            serde_json::from_str::<ProjectionObserved>(&encoded)?,
            reason
        );
    }
    for invalid in [r#""source_absent""#, r#""future-reason""#, "null", "0"] {
        assert!(serde_json::from_str::<ProjectionObserved>(invalid).is_err());
    }
    Ok(())
}
