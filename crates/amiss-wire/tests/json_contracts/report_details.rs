use amiss_wire::{
    controls::{FACT_DOMAIN, FINDING_KEY_DOMAIN, ProjectionKind},
    digest::hb,
    report::{
        FindingKind,
        model::{
            ControlState, ControlStateInput, ControlStateSchema, ControlStateSource,
            ExceptionDiagnostic, FindingFactEvidence, FindingKeyScope, ProjectionDifference,
            ReportEnvelope,
        },
    },
};

#[expect(
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "published report and independent canonical evidence vectors"
)]
pub(super) fn reports() -> Vec<ReportEnvelope> {
    let template = super::report_projections::reports().remove(2);
    let mut reports = Vec::new();
    for (kind, wire) in [
        (
            ProjectionKind::DecimalCountV1,
            r#"{"expected_count":1,"kind":"count","observed_count":null}"#,
        ),
        (
            ProjectionKind::SortedRowsV1,
            r#"{"expected_records":1,"extra_omitted":0,"extra_preview":[],"extra_records":0,"kind":"rows","missing_omitted":0,"missing_preview":[],"missing_records":0,"observed_records":1,"ordering_only":false}"#,
        ),
    ] {
        let detail: ProjectionDifference = serde_json::from_str(wire).unwrap();
        assert_eq!(
            serde_json_canonicalizer::to_vec(&detail).unwrap(),
            wire.as_bytes()
        );
        let mut report = template.clone();
        let fact = report.payload.findings[0].candidate_fact.as_mut().unwrap();
        let FindingFactEvidence::Projection {
            projection,
            difference,
            ..
        } = &mut fact.evidence
        else {
            panic!("the source fixture has projection evidence");
        };
        *projection = kind;
        *difference = Some(detail);
        reports.push(report);
    }
    let digest = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    let oid = "0000000000000000000000000000000000000000";
    for (kind, wire) in [
        (
            FindingKind::DebtExpired,
            r#"{"accepted_fact_digest":"$digest","adoption_tree":{"object_format":"sha1","tree_oid":"$oid"},"created_at":"2026-01-01T00:00:00Z","current_fact_digest":"$digest","debt_id":"debt","debt_snapshot_digest":"$digest","expires_at":"2026-01-02T00:00:00Z","kind":"debt","owner":"team:docs","reason":"reason"}"#,
        ),
        (
            FindingKind::WaiverInvalid,
            r#"{"authorized_fact_digest":"$digest","candidate_tree":{"object_format":"sha1","tree_oid":"$oid"},"created_at":"2026-01-01T00:00:00Z","current_fact_digest":null,"expires_at":"2026-01-02T00:00:00Z","finding_key":"$digest","issuer":"service:amiss","kind":"waiver","not_before":"2026-01-01T00:00:00Z","owner":"team:docs","reason":"reason","residual_disposition":"warn","waiver_bundle_digest":"$digest","waiver_id":"waiver"}"#,
        ),
    ] {
        let wire = wire.replace("$digest", digest).replace("$oid", oid);
        let detail: ExceptionDiagnostic = serde_json::from_str(&wire).unwrap();
        assert_eq!(
            serde_json_canonicalizer::to_vec(&detail).unwrap(),
            wire.as_bytes()
        );
        let mut report = template.clone();
        let finding = &mut report.payload.findings[0];
        finding.kind = kind;
        kind.meaning().clone_into(&mut finding.description);
        finding.key_input.finding_kind = kind;
        finding.key_input.scope = FindingKeyScope::Control {
            control_path: None,
            rule_id: kind.as_ref().to_owned(),
        };
        finding.finding_key = hb(
            FINDING_KEY_DOMAIN,
            &serde_json_canonicalizer::to_vec(&finding.key_input).unwrap(),
        );
        let fact = finding.candidate_fact.as_mut().unwrap();
        fact.finding_kind = kind;
        fact.key_input = finding.key_input.clone();
        fact.evidence = FindingFactEvidence::Control {
            base_control_digest: None,
            base_control_state: None,
            candidate_control_digest: None,
            candidate_control_state: Some(ControlStateInput {
                path: None,
                rule_id: kind.as_ref().to_owned(),
                schema: ControlStateSchema::Current,
                sources: vec![ControlStateSource {
                    digest: hb("test", b"control"),
                    multiplicity: 1,
                }],
                state: ControlState::Present,
            }),
            control_path: None,
            exception: Some(Box::new(detail)),
            rule_id: kind.as_ref().to_owned(),
        };
        reports.push(report);
    }
    for report in &mut reports {
        let finding = &mut report.payload.findings[0];
        finding.candidate_fact_digest = Some(hb(
            FACT_DOMAIN,
            &serde_json_canonicalizer::to_vec(finding.candidate_fact.as_ref().unwrap()).unwrap(),
        ));
    }
    reports
}

#[test]
fn evidence_details_preserve_nullable_fields_and_require_tree_objects() {
    let mut rejected = 0;
    for report in reports() {
        let encoded = serde_json::to_string(&report).unwrap();
        for (field, value) in [
            ("observed_count", "0".to_owned()),
            (
                "current_fact_digest",
                serde_json::to_string(&hb("test", b"fact")).unwrap(),
            ),
        ] {
            let member = format!("\"{field}\":null");
            if encoded.contains(&member) {
                let present = encoded.replace(&member, &format!("\"{field}\":{value}"));
                assert!(serde_json::from_str::<ReportEnvelope>(&present).is_ok());
                for invalid in [
                    encoded.replace(&format!(",{member}"), ""),
                    encoded.replace(&member, &format!("\"{field}\":{{}}")),
                ] {
                    assert_ne!(invalid, encoded);
                    assert!(serde_json::from_str::<ReportEnvelope>(&invalid).is_err());
                    rejected += 1;
                }
            }
        }
        let fact = report.payload.findings[0].candidate_fact.as_ref().unwrap();
        if let FindingFactEvidence::Control {
            exception: Some(detail),
            ..
        } = &fact.evidence
        {
            let tree = match detail.as_ref() {
                ExceptionDiagnostic::Debt { adoption_tree, .. } => adoption_tree,
                ExceptionDiagnostic::Waiver { candidate_tree, .. } => candidate_tree,
            };
            let object = serde_json::to_string(tree).unwrap();
            let sequence = serde_json::to_string(&(tree.object_format, &tree.tree_oid)).unwrap();
            let invalid = encoded.replace(&object, &sequence);
            assert_ne!(invalid, encoded);
            assert!(serde_json::from_str::<ReportEnvelope>(&invalid).is_err());
            rejected += 1;
        }
    }
    assert_eq!(rejected, 6);
}
