use amiss_wire::{
    controls::TrustedTimeStatement,
    report::AnalysisErrorCode,
    report::model::{
        AnalysisError, AnalysisPhase, AvailableFeedback, AvailableFeedbackStatus, ByteSpan,
        DocumentSide, Evaluation, IdentityPreimage, MemoryLimit, PrivateBoundedStorage,
        ReportEnvelope, ResolvedEvaluation, SandboxMechanism, SandboxVerification,
        SandboxVerificationSchema, SandboxVerifier, SourceSpan, StructuralAddress,
        TemporaryStorage, Watchdog,
    },
    requests::{CandidateIdentity, CandidateIdentitySchema},
};
use serde::{Serialize, de::DeserializeOwned};

pub(super) fn assert_safe_numbers<T>(
    make: impl Fn(u64) -> T,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    for number in [0, js_int::MAX_SAFE_UINT - 1, js_int::MAX_SAFE_UINT] {
        let input = make(number);
        let text = serde_json::to_string(&input)?;
        assert_eq!(serde_json::from_str::<T>(&text)?, input);
    }
    let text = serde_json::to_string(&make(js_int::MAX_SAFE_UINT))?;
    let maximum = js_int::MAX_SAFE_UINT.to_string();
    let offsets: Vec<_> = text
        .match_indices(&maximum)
        .map(|(offset, _)| offset)
        .collect();
    assert!(!offsets.is_empty());
    for offset in offsets {
        let end = offset
            .checked_add(maximum.len())
            .ok_or("numeric token range overflow")?;
        for invalid in [
            "9007199254740992",
            "18446744073709551615",
            "-1",
            "-0",
            "1.0",
            "1e0",
            "\"1\"",
            "false",
            "[]",
            "{}",
        ] {
            let mut changed = text.clone();
            changed.replace_range(offset..end, invalid);
            assert!(serde_json::from_str::<T>(&changed).is_err(), "{changed}");
        }
    }
    for number in [js_int::MAX_SAFE_UINT + 1, u64::MAX] {
        let input = make(number);
        assert!(serde_json::to_vec(&input).is_err());
        assert!(serde_json_canonicalizer::to_vec(&input).is_err());
    }
    Ok(())
}

#[test]
fn source_positions_and_document_counts_are_bounded() -> Result<(), Box<dyn std::error::Error>> {
    let report: ReportEnvelope = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-report.canonical.json"
    ))?;
    let document = report.payload.documents[0].candidate.as_ref().unwrap();
    assert_safe_numbers(|number| DocumentSide {
        byte_count: number,
        extracted_references: number,
        frontmatter_bytes: number,
        frontmatter_regions: number,
        opaque_html_bytes: number,
        opaque_html_regions: number,
        opaque_mdx_bytes: number,
        opaque_mdx_regions: number,
        ..document.clone()
    })?;
    let address = &report.payload.observations[0]
        .candidate
        .as_ref()
        .unwrap()
        .observation_id_input
        .structural_address;
    assert_safe_numbers(|number| StructuralAddress {
        construct_index: number,
        duplicate_index: number,
        node_path: vec![number, number],
        ..address.clone()
    })?;
    assert_safe_numbers(|number| SourceSpan {
        end_byte: number,
        end_column: number,
        end_line: number,
        start_byte: number,
        start_column: number,
        start_line: number,
    })?;
    assert_safe_numbers(|number| ByteSpan {
        end_byte: number,
        start_byte: number,
    })?;
    Ok(())
}

#[test]
fn evaluation_and_candidate_preimages_keep_safe_counts() -> Result<(), Box<dyn std::error::Error>> {
    let report: ReportEnvelope = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-report.canonical.json"
    ))?;
    let Evaluation::Resolved(evaluation) = report.payload.evaluation else {
        panic!("the committed report has a resolved evaluation");
    };
    let evaluation = *evaluation;
    assert_safe_numbers(|number| ResolvedEvaluation {
        index_only_materialized_paths: number,
        skip_worktree_paths: number,
        ..evaluation.clone()
    })?;
    let candidate: CandidateIdentity = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/candidate-identity-index.json"
    ))?;
    assert_safe_numbers(|number| CandidateIdentity {
        index_only_materialized_paths: number,
        skip_worktree_paths: number,
        ..candidate.clone()
    })?;
    for number in [
        0,
        js_int::MAX_SAFE_UINT,
        js_int::MAX_SAFE_UINT + 1,
        u64::MAX,
    ] {
        let evaluation = ResolvedEvaluation {
            index_only_materialized_paths: number,
            skip_worktree_paths: number,
            ..evaluation.clone()
        };
        let preimage = IdentityPreimage {
            evaluation: &evaluation,
            schema: CandidateIdentitySchema::Current,
        };
        assert_eq!(
            serde_json_canonicalizer::to_vec(&preimage).is_ok(),
            number <= js_int::MAX_SAFE_UINT
        );
    }
    Ok(())
}

#[test]
fn sandbox_and_trusted_time_counts_are_bounded() -> Result<(), Box<dyn std::error::Error>> {
    assert_safe_numbers(|number| MemoryLimit {
        maximum_bytes: number,
    })?;
    assert_safe_numbers(|number| TemporaryStorage {
        kind: PrivateBoundedStorage::PrivateBounded,
        maximum_bytes: number,
    })?;
    assert_safe_numbers(|number| Watchdog {
        maximum_milliseconds: number,
    })?;
    let statement: TrustedTimeStatement = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-trusted-time-statement.json"
    ))?;
    assert_safe_numbers(|number| TrustedTimeStatement {
        provider_run_attempt: number,
        ..statement.clone()
    })?;
    let descriptor: amiss_wire::controls::ExecutionConstraintDescriptor = serde_json::from_slice(
        include_bytes!("../../../../spec/examples/scanner-execution-constraint.json"),
    )?;
    let provider: amiss_wire::model::ArtifactId = statement.provider.parse()?;
    assert_safe_numbers(|number| SandboxVerification {
        evaluation_identity_digest: statement.candidate_identity_digest,
        execution_constraint_digest: statement.candidate_identity_digest,
        mechanism: SandboxMechanism::OciRootlessSandbox,
        platform: descriptor.selected_platform,
        provider: provider.clone(),
        provider_run_attempt: number,
        provider_run_id: statement.provider_run_id.clone(),
        sandbox_descriptor_digest: statement.candidate_identity_digest,
        schema: SandboxVerificationSchema::Current,
        verifier: SandboxVerifier::ExternalRequiredCheck,
    })?;
    Ok(())
}

#[test]
fn feedback_and_error_counts_keep_their_nullable_contract() -> Result<(), Box<dyn std::error::Error>>
{
    assert_safe_numbers(
        |number| AvailableFeedback::<amiss_wire::report::model::RepoPath> {
            existing_count: number,
            items: Vec::new(),
            status: AvailableFeedbackStatus::Available,
        },
    )?;
    assert_safe_numbers(
        |number| AnalysisError::<amiss_wire::report::model::RepoPath> {
            code: AnalysisErrorCode::InvalidUtf8,
            configured_limit: Some(number),
            description: AnalysisErrorCode::InvalidUtf8.meaning().to_owned(),
            observed_lower_bound: Some(number),
            path: None,
            path_bytes_hex: None,
            phase: AnalysisPhase::Configuration,
            resource: None,
        },
    )?;
    let error: AnalysisError = AnalysisError {
        code: AnalysisErrorCode::InvalidUtf8,
        configured_limit: None,
        description: AnalysisErrorCode::InvalidUtf8.meaning().to_owned(),
        observed_lower_bound: None,
        path: None,
        path_bytes_hex: None,
        phase: AnalysisPhase::Configuration,
        resource: None,
    };
    let text = serde_json::to_string(&error)?;
    assert_eq!(serde_json::from_str::<AnalysisError>(&text)?, error);
    for member in [
        "\"configured_limit\":null,",
        "\"observed_lower_bound\":null,",
    ] {
        assert_eq!(text.matches(member).count(), 1);
        assert!(serde_json::from_str::<AnalysisError>(&text.replacen(member, "", 1)).is_err());
    }
    Ok(())
}
