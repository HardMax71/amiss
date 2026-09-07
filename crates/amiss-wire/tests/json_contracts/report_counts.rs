use amiss_wire::report::model::{
    DocumentCounts, FindingCounts, ReferenceCounts, ReportResult, ReportStatus, Summary,
};

#[test]
fn every_summary_count_enforces_the_integer_contract() {
    let summary = Summary {
        counts_complete: true,
        documents: DocumentCounts::default(),
        findings: FindingCounts::default(),
        governed_claims: 0,
        references: ReferenceCounts::default(),
        unattested_claims: 0,
    };
    let wire = serde_json::to_string(&summary).unwrap();
    let positions: Vec<_> = wire
        .match_indices(":0")
        .map(|(offset, _)| offset + 1)
        .collect();
    assert!(!positions.is_empty());
    for offset in positions {
        for valid in [0, js_int::MAX_SAFE_UINT - 1, js_int::MAX_SAFE_UINT] {
            let mut input = wire.clone();
            input.replace_range(offset..=offset, &valid.to_string());
            let decoded: Summary = serde_json::from_str(&input).unwrap();
            assert_eq!(serde_json::to_string(&decoded).unwrap(), input);
            assert_eq!(
                serde_json_canonicalizer::to_vec(&decoded).unwrap(),
                input.as_bytes()
            );
        }
        for invalid in [
            "9007199254740992",
            "18446744073709551615",
            "-1",
            "-0",
            "0.0",
            "0e0",
            "\"0\"",
            "null",
            "false",
            "[]",
            "{}",
        ] {
            let mut input = wire.clone();
            input.replace_range(offset..=offset, invalid);
            assert!(serde_json::from_str::<Summary>(&input).is_err(), "{input}");
        }
    }
}

#[test]
fn count_models_refuse_invalid_output_without_the_strict_parser() {
    for invalid in [js_int::MAX_SAFE_UINT + 1, u64::MAX] {
        let summary = Summary {
            counts_complete: true,
            documents: DocumentCounts {
                discovered: invalid,
                ..DocumentCounts::default()
            },
            findings: FindingCounts {
                warn: invalid,
                ..FindingCounts::default()
            },
            governed_claims: invalid,
            references: ReferenceCounts {
                extracted: invalid,
                ..ReferenceCounts::default()
            },
            unattested_claims: invalid,
        };
        assert!(serde_json::to_vec(&summary.documents).is_err());
        assert!(serde_json::to_vec(&summary.findings).is_err());
        assert!(serde_json::to_vec(&summary.references).is_err());
        assert!(
            serde_json::to_vec(&Summary {
                documents: DocumentCounts::default(),
                findings: FindingCounts::default(),
                references: ReferenceCounts::default(),
                ..summary
            })
            .is_err()
        );
    }
}

#[test]
fn result_counts_are_bounded_without_changing_the_exit_code() {
    for (error_count, finding_count) in [(js_int::MAX_SAFE_UINT, 0), (0, js_int::MAX_SAFE_UINT)] {
        let result = ReportResult {
            complete: false,
            error_count,
            exit_code: 2,
            finding_count,
            status: ReportStatus::Incomplete,
        };
        let wire = serde_json::to_string(&result).unwrap();
        assert_eq!(serde_json::from_str::<ReportResult>(&wire).unwrap(), result);
        let invalid = wire.replace(&js_int::MAX_SAFE_UINT.to_string(), "9007199254740992");
        assert!(serde_json::from_str::<ReportResult>(&invalid).is_err());
        assert!(
            serde_json::to_vec(&ReportResult {
                error_count: error_count + 1,
                finding_count: finding_count + 1,
                ..result
            })
            .is_err()
        );
    }
}
