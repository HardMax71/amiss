use std::sync::LazyLock;

use amiss_bootstrap::supervise::{AcceptanceDefect, Expectations, accept};
use amiss_wire::digest::hb;
use amiss_wire::report::{PAYLOAD_SCHEMA, model};

use amiss_fixtures::corrupt;

use super::{Deviation, golden};

static GOLDEN: LazyLock<(model::ReportEnvelope, Expectations)> = LazyLock::new(|| {
    let (wire, expectations) = golden(Deviation::default());
    (serde_json::from_slice(&wire).unwrap(), expectations)
});

static POSITIONAL: LazyLock<Vec<Vec<u8>>> = LazyLock::new(|| {
    let (report, _) = &*GOLDEN;
    let model::Controls::Resolved(controls) = &report.payload.controls else {
        panic!("the fixture has resolved controls");
    };
    let model::ExecutionConstraintProvenance::Verified(constraint) = &controls.execution_constraint
    else {
        panic!("the fixture has a verified constraint");
    };
    let model::TrustedTimeProvenance::Verified(trusted) = &controls.trusted_time_source else {
        panic!("the fixture has verified time");
    };
    let descriptor = &constraint.descriptor;
    let statement = &trusted.statement;
    let sandbox = &controls.sandbox;
    let profile = &sandbox.descriptor;
    [
        (
            "controls",
            serde_json_canonicalizer::to_string(controls).unwrap(),
            serde_json_canonicalizer::to_string(&(
                &controls.base_repository_policy_digest,
                &controls.candidate_repository_policy_digest,
                &controls.debt_snapshot,
                &controls.execution_constraint,
                &controls.organization_floor,
                &controls.profile,
                &controls.sandbox,
                &controls.semantic_evidence,
                &controls.trusted_time_source,
                &controls.waiver_bundle,
            ))
            .unwrap(),
        ),
        (
            "organization_floor",
            serde_json_canonicalizer::to_string(&controls.organization_floor).unwrap(),
            serde_json_canonicalizer::to_string(&(
                controls.organization_floor.digest,
                controls.organization_floor.status,
                controls.organization_floor.trust_source,
            ))
            .unwrap(),
        ),
        (
            "debt_snapshot",
            serde_json_canonicalizer::to_string(&controls.debt_snapshot).unwrap(),
            serde_json_canonicalizer::to_string(&(
                controls.debt_snapshot.digest,
                controls.debt_snapshot.status,
                controls.debt_snapshot.trust_source,
            ))
            .unwrap(),
        ),
        (
            "waiver_bundle",
            serde_json_canonicalizer::to_string(&controls.waiver_bundle).unwrap(),
            serde_json_canonicalizer::to_string(&(
                controls.waiver_bundle.digest,
                controls.waiver_bundle.status,
                controls.waiver_bundle.trust_source,
            ))
            .unwrap(),
        ),
        (
            "execution_constraint",
            serde_json_canonicalizer::to_string(constraint).unwrap(),
            serde_json_canonicalizer::to_string(&(
                &constraint.descriptor,
                constraint.descriptor_digest,
                constraint.status,
                constraint.trust_source,
            ))
            .unwrap(),
        ),
        (
            "descriptor",
            serde_json_canonicalizer::to_string(descriptor).unwrap(),
            serde_json_canonicalizer::to_string(&(
                &descriptor.action_commit_oid,
                descriptor.action_object_format,
                &descriptor.action_repository,
                &descriptor.action_tree_oid,
                descriptor.bootstrap_contract,
                descriptor.bootstrap_digest,
                &descriptor.manifest_path,
                descriptor.release_manifest_digest,
                &descriptor.required_status_name,
                descriptor.schema,
                descriptor.selected_platform,
            ))
            .unwrap(),
        ),
        (
            "trusted_time_source",
            serde_json_canonicalizer::to_string(trusted).unwrap(),
            serde_json_canonicalizer::to_string(&(
                &trusted.statement,
                trusted.statement_digest,
                trusted.status,
                trusted.trust_source,
            ))
            .unwrap(),
        ),
        (
            "statement",
            serde_json_canonicalizer::to_string(statement).unwrap(),
            serde_json_canonicalizer::to_string(&(
                statement.candidate_identity_digest,
                statement.controller,
                &statement.evaluation_instant,
                &statement.provider,
                statement.provider_run_attempt,
                &statement.provider_run_id,
                &statement.ref_name,
                &statement.repository,
                statement.schema,
                &statement.valid_until,
            ))
            .unwrap(),
        ),
        (
            "sandbox",
            serde_json_canonicalizer::to_string(sandbox).unwrap(),
            serde_json_canonicalizer::to_string(&(
                sandbox.assurance,
                &sandbox.descriptor,
                sandbox.descriptor_digest,
                sandbox.enforcement_source,
                &sandbox.verification,
            ))
            .unwrap(),
        ),
        (
            "descriptor",
            serde_json_canonicalizer::to_string(profile).unwrap(),
            serde_json_canonicalizer::to_string(&(
                profile.child_processes,
                profile.credentials,
                profile.environment,
                profile.isolation,
                profile.network,
                &profile.physical_memory,
                profile.profile,
                profile.repository_processes,
                profile.schema,
                profile.secrets,
                profile.shared_cache,
                &profile.temporary_storage,
                &profile.watchdog,
                profile.workspace,
            ))
            .unwrap(),
        ),
        (
            "physical_memory",
            serde_json_canonicalizer::to_string(&profile.physical_memory).unwrap(),
            serde_json_canonicalizer::to_string(&[profile.physical_memory.maximum_bytes]).unwrap(),
        ),
        (
            "temporary_storage",
            serde_json_canonicalizer::to_string(&profile.temporary_storage).unwrap(),
            serde_json_canonicalizer::to_string(&(
                profile.temporary_storage.kind,
                profile.temporary_storage.maximum_bytes,
            ))
            .unwrap(),
        ),
        (
            "watchdog",
            serde_json_canonicalizer::to_string(&profile.watchdog).unwrap(),
            serde_json_canonicalizer::to_string(&[profile.watchdog.maximum_milliseconds]).unwrap(),
        ),
    ]
    .map(|(field, object, positional)| {
        corrupt(
            report,
            &format!(r#""{field}":{object}"#),
            &format!(r#""{field}":{positional}"#),
        )
        .unwrap()
    })
    .into()
});

#[test]
fn sealed_controls_require_objects_not_positional_arrays() {
    let (_, expectations) = &*GOLDEN;
    assert_eq!(POSITIONAL.len(), 13);
    for (index, wire) in POSITIONAL.iter().enumerate() {
        assert_eq!(
            accept(wire, expectations),
            Err(AcceptanceDefect::Shape),
            "positional case {index}"
        );
    }
    assert_eq!(
        hb("amiss/test-sealed-control-positional", &POSITIONAL.concat()).to_string(),
        "sha256:fc073c2e75dc50ca7db64556988bed72ec29bbaa431b6e60828087e85c80196c"
    );
}

#[test]
fn unknown_control_members_are_refused_with_correct_payload_digests() {
    let (report, expectations) = &*GOLDEN;
    let model::Controls::Resolved(controls) = &report.payload.controls else {
        panic!("the fixture has resolved controls");
    };
    let model::ExecutionConstraintProvenance::Verified(constraint) = &controls.execution_constraint
    else {
        panic!("the fixture has a verified constraint");
    };
    let model::TrustedTimeProvenance::Verified(trusted) = &controls.trusted_time_source else {
        panic!("the fixture has verified time");
    };
    for (field, object, extension) in [
        (
            "controls",
            serde_json_canonicalizer::to_string(controls).unwrap(),
            "true",
        ),
        (
            "organization_floor",
            serde_json_canonicalizer::to_string(&controls.organization_floor).unwrap(),
            "true",
        ),
        (
            "debt_snapshot",
            serde_json_canonicalizer::to_string(&controls.debt_snapshot).unwrap(),
            "true",
        ),
        (
            "waiver_bundle",
            serde_json_canonicalizer::to_string(&controls.waiver_bundle).unwrap(),
            "true",
        ),
        (
            "execution_constraint",
            serde_json_canonicalizer::to_string(constraint).unwrap(),
            "true",
        ),
        (
            "trusted_time_source",
            serde_json_canonicalizer::to_string(trusted).unwrap(),
            "true",
        ),
        (
            "sandbox",
            serde_json_canonicalizer::to_string(&controls.sandbox).unwrap(),
            "true",
        ),
        (
            "descriptor",
            serde_json_canonicalizer::to_string(&controls.sandbox.descriptor).unwrap(),
            "true",
        ),
        (
            "physical_memory",
            serde_json_canonicalizer::to_string(&controls.sandbox.descriptor.physical_memory)
                .unwrap(),
            "true",
        ),
        (
            "temporary_storage",
            serde_json_canonicalizer::to_string(&controls.sandbox.descriptor.temporary_storage)
                .unwrap(),
            "true",
        ),
        (
            "watchdog",
            serde_json_canonicalizer::to_string(&controls.sandbox.descriptor.watchdog).unwrap(),
            "true",
        ),
        (
            "descriptor",
            serde_json_canonicalizer::to_string(&constraint.descriptor).unwrap(),
            "null",
        ),
        (
            "statement",
            serde_json_canonicalizer::to_string(&trusted.statement).unwrap(),
            "null",
        ),
    ] {
        let extended = object.replacen('{', &format!(r#"{{"future":{extension},"#), 1);
        let wire = corrupt(
            report,
            &format!(r#""{field}":{object}"#),
            &format!(r#""{field}":{extended}"#),
        )
        .unwrap();
        assert_eq!(
            accept(&wire, expectations),
            Err(AcceptanceDefect::Shape),
            "{field}"
        );
    }
}

#[test]
fn sealed_reports_use_the_closed_wire_envelope() {
    let (report, expectations) = &*GOLDEN;
    let wire = serde_json_canonicalizer::to_string(report).unwrap();
    let mut extended = wire.replacen('{', r#"{"future":true,"#, 1);
    extended.push('\n');
    assert_eq!(
        accept(extended.as_bytes(), expectations),
        Err(AcceptanceDefect::Shape)
    );
}

#[test]
fn control_extensions_keep_the_strict_parser_depth_boundary() {
    let (report, expectations) = &*GOLDEN;
    let model::Controls::Resolved(controls) = &report.payload.controls else {
        panic!("the fixture has resolved controls");
    };
    let payload = serde_json_canonicalizer::to_string(&report.payload).unwrap();
    let wire = serde_json_canonicalizer::to_string(report).unwrap();
    let digest = report.payload_digest.to_string();
    let floor = serde_json_canonicalizer::to_string(&controls.organization_floor).unwrap();
    let field = format!(r#""organization_floor":{floor}"#);
    assert_eq!(payload.matches(&field).count(), 1);
    assert_eq!(wire.matches(&payload).count(), 1);
    assert_eq!(wire.matches(&digest).count(), 1);
    let mut captured = Vec::new();
    for depth in [128, 512] {
        let extension = format!(
            r#""future":{}null{},{}"#,
            "[".repeat(depth),
            "]".repeat(depth),
            field
        );
        let changed = payload.replacen(&field, &extension, 1);
        let mut altered = wire.replacen(&payload, &changed, 1).replacen(
            &digest,
            &hb(PAYLOAD_SCHEMA, changed.as_bytes()).to_string(),
            1,
        );
        altered.push('\n');
        assert_eq!(
            accept(altered.as_bytes(), expectations),
            Err(AcceptanceDefect::Shape),
            "{depth}"
        );
        captured.extend_from_slice(altered.as_bytes());
    }
    assert_eq!(
        hb("amiss/test-sealed-control-depth", &captured).to_string(),
        "sha256:14ded5dd113fa73b0700ba3e87f296e677585f851ba74d02c94f3a2068b22a51"
    );
}

#[test]
fn missing_nullable_control_members_are_not_null() {
    let (report, expectations) = &*GOLDEN;
    let model::Controls::Resolved(controls) = &report.payload.controls else {
        panic!("the fixture has resolved controls");
    };
    for (field, object, member) in [
        (
            "debt_snapshot",
            serde_json_canonicalizer::to_string(&controls.debt_snapshot).unwrap(),
            r#""digest":null,"#,
        ),
        (
            "sandbox",
            serde_json_canonicalizer::to_string(&controls.sandbox).unwrap(),
            r#","verification":null"#,
        ),
    ] {
        assert_eq!(object.matches(member).count(), 1);
        let missing = object.replacen(member, "", 1);
        let wire = corrupt(
            report,
            &format!(r#""{field}":{object}"#),
            &format!(r#""{field}":{missing}"#),
        )
        .unwrap();
        assert_eq!(
            accept(&wire, expectations),
            Err(AcceptanceDefect::Shape),
            "{field}"
        );
    }
}
