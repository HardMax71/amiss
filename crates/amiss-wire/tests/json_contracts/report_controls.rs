use amiss_wire::{
    controls::{canonical_execution_constraint, canonical_trusted_time},
    digest::hb,
    report::{
        PAYLOAD_SCHEMA, ReportDefect,
        model::{
            Controls, ControlsUnavailableReason, ExecutionConstraintProvenance, ReportEnvelope,
            SandboxAssurance, SandboxEnforcementSource, SandboxMechanism, SandboxVerification,
            SandboxVerificationSchema, SandboxVerifier, SemanticEvidenceProducer,
            SemanticEvidenceProvenance, TrustedTimeProvenance, TrustedTimeTrustSource,
            UnavailableControls, UnavailableStatus, VerifiedControlStatus,
            VerifiedExecutionConstraint, VerifiedTrustedTime,
        },
        validate_envelope,
    },
    requests::RequestTrust,
    semantic::SemanticProducerKind,
};

#[expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "published fixtures and known report controls"
)]
fn reports() -> Vec<ReportEnvelope> {
    let mut report: ReportEnvelope = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-report.canonical.json"
    ))
    .unwrap();
    let mut reports = vec![report.clone()];
    let Controls::Resolved(controls) = &mut report.payload.controls else {
        panic!("the report fixture has resolved controls");
    };
    let descriptor = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-execution-constraint.json"
    ))
    .unwrap();
    let (_, descriptor_digest) = canonical_execution_constraint(&descriptor).unwrap();
    let statement = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-trusted-time-statement.json"
    ))
    .unwrap();
    let (_, statement_digest) = canonical_trusted_time(&statement).unwrap();
    controls.sandbox.assurance = SandboxAssurance::ProviderVerified;
    controls.sandbox.enforcement_source = SandboxEnforcementSource::ExternalRequiredCheck;
    controls.sandbox.verification = Some(SandboxVerification {
        evaluation_identity_digest: statement.candidate_identity_digest,
        execution_constraint_digest: descriptor_digest,
        mechanism: SandboxMechanism::OciRootlessSandbox,
        platform: descriptor.selected_platform,
        provider: statement.provider.parse().unwrap(),
        provider_run_attempt: statement.provider_run_attempt,
        provider_run_id: statement.provider_run_id.clone(),
        sandbox_descriptor_digest: controls.sandbox.descriptor_digest,
        schema: SandboxVerificationSchema::Current,
        verifier: SandboxVerifier::ExternalRequiredCheck,
    });
    controls.execution_constraint =
        ExecutionConstraintProvenance::Verified(Box::new(VerifiedExecutionConstraint {
            descriptor,
            descriptor_digest,
            status: VerifiedControlStatus::Verified,
            trust_source: RequestTrust::ExternalRequiredCheck,
        }));
    controls.trusted_time_source = TrustedTimeProvenance::Verified(Box::new(VerifiedTrustedTime {
        statement,
        statement_digest,
        status: VerifiedControlStatus::Verified,
        trust_source: TrustedTimeTrustSource::ExternalRequiredCheck,
    }));
    controls.semantic_evidence = Some(vec![SemanticEvidenceProvenance {
        payload_digest: hb("test", b"evidence"),
        producer: SemanticEvidenceProducer {
            identity: "producer".parse().unwrap(),
            input_digest: hb("test", b"input"),
            kind: SemanticProducerKind::RecordSet,
            version: "1".to_owned(),
        },
    }]);
    reports.push(report.clone());
    report.payload.controls = Controls::Unavailable(UnavailableControls {
        reasons: vec![ControlsUnavailableReason::NotParsed],
        request_digest: None,
        status: UnavailableStatus::Unavailable,
    });
    reports.push(report);
    reports
}

#[test]
fn report_controls_reject_unknown_members_after_payload_digest_verification() {
    let cases = reports();
    assert_eq!(cases.len(), 3);
    for mut report in cases {
        let payload =
            String::from_utf8(serde_json_canonicalizer::to_vec(&report.payload).unwrap()).unwrap();
        report.payload_digest = hb(PAYLOAD_SCHEMA, payload.as_bytes());
        let wire = String::from_utf8(serde_json_canonicalizer::to_vec(&report).unwrap()).unwrap();
        assert_eq!(
            validate_envelope(wire.as_bytes()).unwrap().0,
            report.payload
        );
        let controls =
            String::from_utf8(serde_json_canonicalizer::to_vec(&report.payload.controls).unwrap())
                .unwrap();
        for (offset, _) in controls.match_indices('{') {
            let mut invalid = controls.clone();
            invalid.insert_str(offset + 1, "\"__unexpected\":true,");
            let altered_payload = payload.replace(&controls, &invalid);
            assert_ne!(payload, altered_payload);
            let altered = wire.replace(&payload, &altered_payload).replace(
                &report.payload_digest.to_string(),
                &hb(PAYLOAD_SCHEMA, altered_payload.as_bytes()).to_string(),
            );
            assert_eq!(
                validate_envelope(altered.as_bytes()).map(drop),
                Err(ReportDefect::NotAReport),
                "{invalid}"
            );
        }
    }
}

#[test]
fn absent_control_statuses_cannot_hide_verified_bodies() {
    let mut cases = reports();
    let report = cases.remove(1);
    let Controls::Resolved(controls) = report.payload.controls else {
        panic!("the verified fixture resolves its controls");
    };
    let constraint = serde_json::to_string(&controls.execution_constraint).unwrap();
    let time = serde_json::to_string(&controls.trusted_time_source).unwrap();
    let status = serde_json::to_string(&VerifiedControlStatus::Verified).unwrap();
    let none = serde_json::to_string(&amiss_wire::report::model::NoControlStatus::None).unwrap();
    assert!(
        serde_json::from_str::<ExecutionConstraintProvenance>(&constraint.replace(&status, &none))
            .is_err()
    );
    assert!(serde_json::from_str::<TrustedTimeProvenance>(&time.replace(&status, &none)).is_err());
}
