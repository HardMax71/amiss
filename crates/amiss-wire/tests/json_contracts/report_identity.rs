use amiss_wire::report::model::{
    BaseSnapshot, Evaluation, IdentityPreimage, ResolvedEvaluation, Snapshot,
    UnavailableEvaluation, UnavailableSnapshot, UnavailableSnapshotKind, UnavailableStatus,
};
use amiss_wire::requests::{CandidateIdentity, CandidateIdentitySchema, GitSnapshotKind};

#[test]
fn snapshot_and_unavailable_tags_require_strings() -> serde_json::Result<()> {
    assert_eq!(
        serde_json::to_string(&GitSnapshotKind::GitCommit)?,
        "\"git-commit\""
    );
    assert_eq!(
        serde_json::to_string(&UnavailableStatus::Unavailable)?,
        "\"unavailable\""
    );
    for value in [r#"{"git-commit": null}"#, "null", "false", "1"] {
        assert!(serde_json::from_str::<GitSnapshotKind>(value).is_err());
    }
    for value in [r#"{"unavailable": null}"#, "null", "false", "1"] {
        assert!(serde_json::from_str::<UnavailableStatus>(value).is_err());
    }
    Ok(())
}

#[test]
fn report_identity_projections_keep_the_candidate_contract() -> serde_json::Result<()> {
    for example in [
        include_str!("../../../../spec/examples/candidate-identity.json"),
        include_str!("../../../../spec/examples/candidate-identity-index.json"),
    ] {
        let expected: CandidateIdentity = serde_json::from_str(example)?;
        let evaluation = example.replace(
            &format!("\"schema\": {},", serde_json::to_string(&expected.schema)?),
            "\"evaluation_instant\": null, \"trusted_time\": false,",
        );
        assert_ne!(evaluation, example);
        let mut evaluation: ResolvedEvaluation = serde_json::from_str(&evaluation)?;
        for (instant, trusted) in [
            (None, false),
            (Some("2026-09-06T10:00:00Z"), false),
            (Some("2026-09-06T11:00:00Z"), true),
        ] {
            evaluation.evaluation_instant = instant
                .map(|instant| amiss_wire::model::UtcInstant::new(instant.to_owned()).unwrap());
            evaluation.trusted_time = trusted;
            let preimage = IdentityPreimage {
                evaluation: &evaluation,
                schema: CandidateIdentitySchema::Current,
            };
            let bytes = serde_json_canonicalizer::to_vec(&preimage)?;
            assert_eq!(bytes, serde_json_canonicalizer::to_vec(&expected)?);
            assert_eq!(
                serde_json::from_slice::<CandidateIdentity>(&bytes)?,
                expected
            );
        }
        let encoded = serde_json::to_string(&evaluation)?;
        for reserved in ["null", "false", "\"amiss/scanner-candidate-identity\""] {
            let invalid = encoded.replacen('{', &format!("{{\"schema\":{reserved},"), 1);
            assert!(serde_json::from_str::<ResolvedEvaluation>(&invalid).is_err());
        }
    }
    Ok(())
}

#[test]
fn report_evaluations_and_snapshots_are_closed_objects() {
    let (payload, _, _) = amiss_wire::report::validate_envelope(include_bytes!(
        "../../../../spec/examples/scanner-report.canonical.json"
    ))
    .unwrap();
    let Evaluation::Resolved(mut evaluation) = payload.evaluation else {
        panic!("the report example has a resolved evaluation");
    };
    let unavailable = UnavailableSnapshot {
        kind: UnavailableSnapshotKind::Unavailable,
        reasons: Vec::new(),
        request_digest: None,
    };
    let index: CandidateIdentity = serde_json::from_str(include_str!(
        "../../../../spec/examples/candidate-identity-index.json"
    ))
    .unwrap();
    let mut cases = Vec::new();
    for candidate in [
        evaluation.candidate.clone(),
        Snapshot::Available(index.candidate),
        Snapshot::Unavailable(unavailable.clone()),
    ] {
        evaluation.candidate = candidate;
        cases.push(Evaluation::Resolved(evaluation.clone()));
    }
    evaluation.base = BaseSnapshot::Unavailable(unavailable);
    cases.push(Evaluation::Resolved(evaluation));
    cases.push(Evaluation::Unavailable(UnavailableEvaluation {
        reasons: Vec::new(),
        request_digest: None,
        status: UnavailableStatus::Unavailable,
    }));

    for evaluation in cases {
        let encoded = serde_json::to_string(&evaluation).unwrap();
        assert_eq!(
            serde_json::from_str::<Evaluation>(&encoded).unwrap(),
            evaluation
        );
        for (offset, _) in encoded.match_indices('{') {
            let mut invalid = encoded.clone();
            invalid.insert_str(offset + 1, "\"unexpected\": true,");
            assert!(
                serde_json::from_str::<Evaluation>(&invalid).is_err(),
                "{invalid}"
            );
        }
        if let Evaluation::Resolved(evaluation) = evaluation {
            let positional = serde_json::to_string(&(
                &evaluation.base,
                &evaluation.candidate,
                &evaluation.candidate_ref,
                &evaluation.default_branch_ref,
                &evaluation.evaluation_instant,
                evaluation.event_kind,
                evaluation.finality,
                evaluation.forge,
                evaluation.index_only_materialized_paths,
                evaluation.materialization,
                evaluation.mode,
                &evaluation.repository,
                evaluation.skip_worktree_paths,
                &evaluation.target_ref,
                evaluation.trusted_time,
            ))
            .unwrap();
            assert!(serde_json::from_str::<Evaluation>(&positional).is_err());
        }
    }
}

#[test]
fn positional_snapshot_variants_are_not_objects() {
    let identity: CandidateIdentity = serde_json::from_str(include_str!(
        "../../../../spec/examples/candidate-identity-index.json"
    ))
    .unwrap();
    let amiss_wire::requests::CandidateSnapshot::Index(index) = identity.candidate else {
        panic!("the index fixture has an index candidate");
    };
    let git = identity.base;
    let unavailable = UnavailableSnapshot {
        kind: UnavailableSnapshotKind::Unavailable,
        reasons: Vec::new(),
        request_digest: None,
    };
    for array in [
        serde_json::to_string(&(git.commit_oid, git.kind, git.object_format, git.tree_oid))
            .unwrap(),
        serde_json::to_string(&(
            index.base_commit_oid,
            index.base_object_format,
            index.entry_count,
            index.identity_scope,
            index.index_projection_digest,
            index.kind,
            index.snapshot_digest,
            index.snapshot_schema,
        ))
        .unwrap(),
        serde_json::to_string(&(
            unavailable.kind,
            unavailable.reasons,
            unavailable.request_digest,
        ))
        .unwrap(),
    ] {
        assert!(
            serde_json::from_str::<BaseSnapshot>(&array).is_err(),
            "{array}"
        );
        assert!(serde_json::from_str::<Snapshot>(&array).is_err(), "{array}");
    }
    let array = serde_json::to_string(&(
        Vec::<amiss_wire::report::model::EvaluationUnavailableReason>::new(),
        None::<amiss_wire::digest::Digest>,
        UnavailableStatus::Unavailable,
    ))
    .unwrap();
    assert!(serde_json::from_str::<Evaluation>(&array).is_err());
}
