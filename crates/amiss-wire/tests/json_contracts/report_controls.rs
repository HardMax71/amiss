use amiss_wire::{
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
use sha2::Digest as _;

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
    let descriptor: amiss_wire::controls::ExecutionConstraintDescriptor = serde_json::from_slice(
        include_bytes!("../../../../spec/examples/scanner-execution-constraint.json"),
    )
    .unwrap();
    let descriptor_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix("amiss/scanner-execution-constraint")
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(&descriptor).unwrap())
            .finalize()
            .0,
    );
    report.payload.engine.action_provenance =
        ActionProvenance::ForgeAction(Box::new(forge_action(&descriptor)));
    let statement: amiss_wire::controls::TrustedTimeStatement = serde_json::from_slice(
        include_bytes!("../../../../spec/examples/scanner-trusted-time-statement.json"),
    )
    .unwrap();
    let statement_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix("amiss/scanner-trusted-time-statement")
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(&statement).unwrap())
            .finalize()
            .0,
    );
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
        payload_digest: amiss_wire::model::Digest::from([23; 32]),
        producer: SemanticEvidenceProducer {
            identity: "producer".parse().unwrap(),
            input_digest: amiss_wire::model::Digest::from([21; 32]),
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

#[expect(
    clippy::unwrap_used,
    reason = "published execution provenance fixtures"
)]
fn forge_action(
    descriptor: &amiss_wire::controls::ExecutionConstraintDescriptor,
) -> ForgeActionProvenance {
    let release_manifest: amiss_wire::manifest::ReleaseManifest = serde_json::from_slice(
        include_bytes!("../../../../spec/examples/scanner-release-manifest.json"),
    )
    .unwrap();
    let release_manifest_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix("amiss/scanner-release-manifest")
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(&release_manifest).unwrap())
            .finalize()
            .0,
    );
    let artifact = release_manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.platform == descriptor.selected_platform)
        .unwrap();
    ForgeActionProvenance {
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
    }
}

#[test]
fn complete_report_payloads_reject_unknown_members_with_matching_digests() {
    let mut cases = reports();
    cases.extend(super::report_rows::reports().unwrap());
    cases.push(
        serde_json::from_slice(include_bytes!(
            "../../../../spec/examples/scanner-report.frozen-1.json"
        ))
        .unwrap(),
    );
    cases.extend(super::report_projections::reports());
    cases.extend(super::report_details::reports());
    cases.extend(super::report_findings::reports());
    assert_eq!(cases.len(), 18);
    for mut report in cases {
        let payload =
            String::from_utf8(serde_json_canonicalizer::to_vec(&report.payload).unwrap()).unwrap();
        report.payload_digest = amiss_wire::model::Digest::from(
            sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA)
                .chain_update([0_u8])
                .chain_update(payload.as_bytes())
                .finalize()
                .0,
        );
        let wire = String::from_utf8(serde_json_canonicalizer::to_vec(&report).unwrap()).unwrap();
        assert_eq!(
            validate_envelope(wire.as_bytes()).unwrap().0,
            report.payload
        );
        for (offset, _) in payload.match_indices('{') {
            let mut invalid = payload.clone();
            invalid.insert_str(offset + 1, "\"__unexpected\":true,");
            let altered = wire.replace(&payload, &invalid).replace(
                &report.payload_digest.to_string(),
                &amiss_wire::model::Digest::from(
                    sha2::Sha256::new_with_prefix(PAYLOAD_SCHEMA)
                        .chain_update([0_u8])
                        .chain_update(invalid.as_bytes())
                        .finalize()
                        .0,
                )
                .to_string(),
            );
            assert_eq!(
                validate_envelope(altered.as_bytes()).map(drop),
                Err(ReportDefect::NotAReport),
                "{invalid:.160} (object offset {offset})"
            );
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
