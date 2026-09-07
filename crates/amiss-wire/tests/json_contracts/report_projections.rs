use amiss_wire::{
    controls::{FACT_DOMAIN, ProjectionKind, parse_projection_source},
    digest::hb,
    report::model::{FindingFactEvidence, ReportEnvelope},
};

#[expect(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "published report and canonical source fixtures"
)]
pub(super) fn reports() -> Vec<ReportEnvelope> {
    let template: ReportEnvelope = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-report.canonical.json"
    ))
    .unwrap();
    [
        (
            ProjectionKind::CodeTextV1,
            r#"{"first_line":1,"kind":"blob-lines","last_line":2,"path":"a.md"}"#,
        ),
        (
            ProjectionKind::CodeTextV1,
            r#"{"end_marker":"end","kind":"named-region","path":"a.md","start_marker":"start"}"#,
        ),
        (
            ProjectionKind::SortedRowsV1,
            r#"{"kind":"record-set","set":"records"}"#,
        ),
        (
            ProjectionKind::CodeTextV1,
            r#"{"key":"name","kind":"record-value","set":"records"}"#,
        ),
        (
            ProjectionKind::SortedRowsV1,
            r#"{"kind":"tree-paths","maximum_depth":1,"root":"docs"}"#,
        ),
        (
            ProjectionKind::SortedRowsV1,
            r#"{"kind":"tree-paths","maximum_depth":1,"root":"docs","suffix":".md"}"#,
        ),
    ]
    .into_iter()
    .map(|(kind, wire)| {
        let producer = parse_projection_source(wire.as_bytes(), kind).unwrap();
        assert_eq!(
            serde_json_canonicalizer::to_vec(&producer).unwrap(),
            wire.as_bytes()
        );
        let mut report = template.clone();
        let finding = &mut report.payload.findings[0];
        let fact = finding.candidate_fact.as_mut().unwrap();
        let FindingFactEvidence::Projection {
            projection, source, ..
        } = &mut fact.evidence
        else {
            panic!("the fixture has projection evidence");
        };
        *projection = kind;
        *source = producer;
        finding.candidate_fact_digest = Some(hb(
            FACT_DOMAIN,
            &serde_json_canonicalizer::to_vec(fact).unwrap(),
        ));
        report
    })
    .collect()
}

#[test]
fn report_projection_sources_reject_hidden_selection_fields_and_null_suffixes() {
    let mut rejected = 0;
    for report in reports() {
        let encoded = serde_json::to_string(&report).unwrap();
        for invalid in [
            encoded.replace("\"kind\":\"record-value\"", "\"kind\":\"record-set\""),
            encoded.replace("\"suffix\":\".md\"", "\"suffix\":null"),
        ] {
            if invalid != encoded {
                assert!(serde_json::from_str::<ReportEnvelope>(&invalid).is_err());
                rejected += 1;
            }
        }
    }
    assert_eq!(rejected, 2);
}
