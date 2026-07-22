use amiss_bootstrap::result::{
    BootstrapResult, RESULT_BYTES, parse_result, result_bytes, result_exit_code,
};

const RECORDS: [(BootstrapResult, &[u8], i32); 7] = [
    (
        BootstrapResult::Pass,
        b"amiss/bootstrap-result-v1 pass\n",
        0,
    ),
    (
        BootstrapResult::Block,
        b"amiss/bootstrap-result-v1 block\n",
        1,
    ),
    (
        BootstrapResult::MissingOutput,
        b"amiss/bootstrap-result-v1 missing-output\n",
        2,
    ),
    (
        BootstrapResult::Timeout,
        b"amiss/bootstrap-result-v1 timeout\n",
        2,
    ),
    (
        BootstrapResult::OversizedOutput,
        b"amiss/bootstrap-result-v1 oversized-output\n",
        2,
    ),
    (
        BootstrapResult::TamperedRuntime,
        b"amiss/bootstrap-result-v1 tampered-runtime\n",
        2,
    ),
    (
        BootstrapResult::Unavailable,
        b"amiss/bootstrap-result-v1 unavailable\n",
        2,
    ),
];

#[test]
fn every_result_has_one_exact_bounded_record() {
    for (result, record, exit_code) in RECORDS {
        assert_eq!(result_bytes(result), record);
        assert_eq!(result_exit_code(result), exit_code);
        assert_eq!(parse_result(record), Some(result));
        assert!(record.is_ascii());
        assert_eq!(record.last(), Some(&b'\n'));
        assert!(u64::try_from(record.len()).is_ok_and(|size| size <= RESULT_BYTES));
    }
}

#[test]
fn malformed_records_are_not_results() {
    let oversized = vec![
        b'x';
        usize::try_from(RESULT_BYTES)
            .unwrap_or(64)
            .saturating_add(1)
    ];
    let malformed: [&[u8]; 9] = [
        b"",
        b"amiss/bootstrap-result-v1 pass",
        b"amiss/bootstrap-result-v1 pass\r\n",
        b"amiss/bootstrap-result-v1 pass\n\n",
        b" amiss/bootstrap-result-v1 pass\n",
        b"amiss/bootstrap-result-v2 pass\n",
        b"amiss/bootstrap-result-v1 passed\n",
        b"amiss/bootstrap-result-v1 pass extra\n",
        &oversized,
    ];

    for bytes in malformed {
        assert_eq!(parse_result(bytes), None, "{bytes:?}");
    }
}
