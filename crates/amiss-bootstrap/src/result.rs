const PASS: &[u8] = b"amiss/bootstrap-result-v1 pass\n";
const BLOCK: &[u8] = b"amiss/bootstrap-result-v1 block\n";
const MISSING_OUTPUT: &[u8] = b"amiss/bootstrap-result-v1 missing-output\n";
const TIMEOUT: &[u8] = b"amiss/bootstrap-result-v1 timeout\n";
const OVERSIZED_OUTPUT: &[u8] = b"amiss/bootstrap-result-v1 oversized-output\n";
const TAMPERED_RUNTIME: &[u8] = b"amiss/bootstrap-result-v1 tampered-runtime\n";
const UNAVAILABLE: &[u8] = b"amiss/bootstrap-result-v1 unavailable\n";

/// Maximum size of one bootstrap result record.
pub const RESULT_BYTES: u64 = 64;

const RECORDS: [(BootstrapResult, &[u8], i32); 7] = [
    (BootstrapResult::Pass, PASS, 0),
    (BootstrapResult::Block, BLOCK, 1),
    (BootstrapResult::MissingOutput, MISSING_OUTPUT, 2),
    (BootstrapResult::Timeout, TIMEOUT, 2),
    (BootstrapResult::OversizedOutput, OVERSIZED_OUTPUT, 2),
    (BootstrapResult::TamperedRuntime, TAMPERED_RUNTIME, 2),
    (BootstrapResult::Unavailable, UNAVAILABLE, 2),
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
pub fn result_bytes(result: BootstrapResult) -> &'static [u8] {
    record(result).0
}

/// Returns the required process exit code for one result record.
#[must_use]
pub fn result_exit_code(result: BootstrapResult) -> i32 {
    record(result).1
}

fn record(result: BootstrapResult) -> (&'static [u8], i32) {
    RECORDS
        .iter()
        .find_map(|(candidate, bytes, exit_code)| {
            (*candidate == result).then_some((*bytes, *exit_code))
        })
        .unwrap_or((UNAVAILABLE, 2))
}

/// Parses one exact result record. Whitespace and unknown versions are not
/// accepted.
#[must_use]
pub fn parse_result(bytes: &[u8]) -> Option<BootstrapResult> {
    RECORDS
        .iter()
        .find_map(|(result, record, _)| (*record == bytes).then_some(*result))
}
