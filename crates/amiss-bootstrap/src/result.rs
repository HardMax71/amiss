const PASS: &[u8] = b"amiss/bootstrap-result-v1 pass\n";
const BLOCK: &[u8] = b"amiss/bootstrap-result-v1 block\n";
const MISSING_OUTPUT: &[u8] = b"amiss/bootstrap-result-v1 missing-output\n";
const TIMEOUT: &[u8] = b"amiss/bootstrap-result-v1 timeout\n";
const OVERSIZED_OUTPUT: &[u8] = b"amiss/bootstrap-result-v1 oversized-output\n";
const TAMPERED_RUNTIME: &[u8] = b"amiss/bootstrap-result-v1 tampered-runtime\n";
const UNAVAILABLE: &[u8] = b"amiss/bootstrap-result-v1 unavailable\n";

/// Maximum size of one bootstrap result record.
pub const RESULT_BYTES: u64 = 64;

const RECORDS: [(BootstrapResult, &[u8]); 7] = [
    (BootstrapResult::Pass, PASS),
    (BootstrapResult::Block, BLOCK),
    (BootstrapResult::MissingOutput, MISSING_OUTPUT),
    (BootstrapResult::Timeout, TIMEOUT),
    (BootstrapResult::OversizedOutput, OVERSIZED_OUTPUT),
    (BootstrapResult::TamperedRuntime, TAMPERED_RUNTIME),
    (BootstrapResult::Unavailable, UNAVAILABLE),
];

/// The closed outcome written by one trusted bootstrap process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootstrapResult {
    Pass,
    Block,
    MissingOutput,
    Timeout,
    OversizedOutput,
    TamperedRuntime,
    Unavailable,
}

/// Returns the exact versioned record for one result.
#[must_use]
pub const fn result_bytes(result: BootstrapResult) -> &'static [u8] {
    match result {
        BootstrapResult::Pass => PASS,
        BootstrapResult::Block => BLOCK,
        BootstrapResult::MissingOutput => MISSING_OUTPUT,
        BootstrapResult::Timeout => TIMEOUT,
        BootstrapResult::OversizedOutput => OVERSIZED_OUTPUT,
        BootstrapResult::TamperedRuntime => TAMPERED_RUNTIME,
        BootstrapResult::Unavailable => UNAVAILABLE,
    }
}

/// Returns the required process exit code for one result record.
#[must_use]
pub const fn result_exit_code(result: BootstrapResult) -> i32 {
    match result {
        BootstrapResult::Pass => 0,
        BootstrapResult::Block => 1,
        BootstrapResult::MissingOutput
        | BootstrapResult::Timeout
        | BootstrapResult::OversizedOutput
        | BootstrapResult::TamperedRuntime
        | BootstrapResult::Unavailable => 2,
    }
}

/// Parses one exact result record. Whitespace and unknown versions are not
/// accepted.
#[must_use]
pub fn parse_result(bytes: &[u8]) -> Option<BootstrapResult> {
    RECORDS
        .iter()
        .find_map(|(result, record)| (*record == bytes).then_some(*result))
}
