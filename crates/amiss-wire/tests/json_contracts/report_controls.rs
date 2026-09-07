use amiss_wire::{
    controls::{canonical_execution_constraint, canonical_trusted_time},
    digest::hb,
    manifest::canonical_release_manifest,
    report::{
        PAYLOAD_SCHEMA, ReportDefect,
        model::{
            ActionProvenance, Controls, ControlsUnavailableReason, ExecutionConstraintProvenance,
            ForgeActionKind, ForgeActionProvenance, LocalActionKind, ReportEnvelope,
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
    let release_manifest = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-release-manifest.json"
    ))
    .unwrap();
    let (_, release_manifest_digest) = canonical_release_manifest(&release_manifest).unwrap();
    let artifact = release_manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.platform == descriptor.selected_platform)
        .unwrap();
    report.payload.engine.action_provenance =
        ActionProvenance::ForgeAction(Box::new(ForgeActionProvenance {
            action_commit_oid: descriptor.action_commit_oid.clone(),
            action_object_format: descriptor.action_object_format,
            action_repository: descriptor.action_repository.clone(),
            action_tree_oid: descriptor.action_tree_oid.clone(),
            dependency_lock_digest: release_manifest.dependency_lock_digest,
            kind: ForgeActionKind::ForgeAction,
            manifest_path: descriptor.manifest_path.clone(),
            selected_artifact_name: artifact.artifact_name.to_string(),
            release_manifest,
            release_manifest_digest,
            selected_platform: descriptor.selected_platform,
        }));
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
fn report_metadata_rejects_unknown_members_after_digest_verification() {
    let mut cases = reports();
    cases.extend(super::report_rows::reports().unwrap());
    cases.push(
        serde_json::from_slice(include_bytes!(
            "../../../../spec/examples/scanner-report.frozen-1.json"
        ))
        .unwrap(),
    );
    assert_eq!(cases.len(), 6);
    for mut report in cases {
        let payload =
            String::from_utf8(serde_json_canonicalizer::to_vec(&report.payload).unwrap()).unwrap();
        report.payload_digest = hb(PAYLOAD_SCHEMA, payload.as_bytes());
        let wire = String::from_utf8(serde_json_canonicalizer::to_vec(&report).unwrap()).unwrap();
        assert_eq!(
            validate_envelope(wire.as_bytes()).unwrap().0,
            report.payload
        );
        for (fragment, expected) in [
            (
                serde_json_canonicalizer::to_vec(&report.payload.controls).unwrap(),
                ReportDefect::NotAReport,
            ),
            (
                serde_json_canonicalizer::to_vec(&report.payload.result).unwrap(),
                ReportDefect::InvalidResult,
            ),
            (
                serde_json_canonicalizer::to_vec(&report.payload.engine).unwrap(),
                ReportDefect::NotAReport,
            ),
            (
                serde_json_canonicalizer::to_vec(&report.payload.summary).unwrap(),
                ReportDefect::NotAReport,
            ),
            (
                serde_json_canonicalizer::to_vec(&report.payload.documents).unwrap(),
                ReportDefect::NotAReport,
            ),
            (
                serde_json_canonicalizer::to_vec(&report.payload.feedback).unwrap(),
                ReportDefect::NotAReport,
            ),
            (
                serde_json_canonicalizer::to_vec(&report.payload.errors).unwrap(),
                ReportDefect::NotAReport,
            ),
            (
                serde_json_canonicalizer::to_vec(&report.payload.observations).unwrap(),
                ReportDefect::NotAReport,
            ),
        ]
        .into_iter()
        .chain(report.payload.findings.iter().flat_map(|finding| {
            std::iter::once(&finding.key_input)
                .chain(
                    finding
                        .base_fact
                        .iter()
                        .chain(&finding.candidate_fact)
                        .map(|fact| &fact.key_input),
                )
                .map(|key| {
                    (
                        serde_json_canonicalizer::to_vec(key).unwrap(),
                        ReportDefect::NotAReport,
                    )
                })
        })) {
            let fragment = String::from_utf8(fragment).unwrap();
            for (offset, _) in fragment.match_indices('{') {
                let mut invalid = fragment.clone();
                invalid.insert_str(offset + 1, "\"__unexpected\":true,");
                let altered_payload = payload.replace(&fragment, &invalid);
                assert_ne!(payload, altered_payload);
                let altered = wire.replace(&payload, &altered_payload).replace(
                    &report.payload_digest.to_string(),
                    &hb(PAYLOAD_SCHEMA, altered_payload.as_bytes()).to_string(),
                );
                assert_eq!(
                    validate_envelope(altered.as_bytes()).map(drop),
                    Err(expected),
                    "{invalid:.160} (object offset {offset})"
                );
            }
        }
    }
}

#[test]
fn local_action_status_cannot_hide_a_forge_action_body() {
    let report = reports().remove(1);
    let action = serde_json::to_string(&report.payload.engine.action_provenance).unwrap();
    let forge = serde_json::to_string(&ForgeActionKind::ForgeAction).unwrap();
    let local = serde_json::to_string(&LocalActionKind::Local).unwrap();
    let invalid = action.replace(&forge, &local);
    assert_ne!(action, invalid);
    assert!(serde_json::from_str::<ActionProvenance>(&invalid).is_err());
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
