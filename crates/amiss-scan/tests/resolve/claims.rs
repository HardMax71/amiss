#![expect(clippy::expect_used, reason = "fixed claim fixtures must fail loudly")]

use amiss_scan::claim::{ClaimMissingReason, ClaimVerdict, ValueClaim};
use amiss_scan::resolve::resolve_claim;
use amiss_scan::{Error, ScanLimits};
use amiss_wire::model::RepoPath;

use crate::support::{MIXED_LINES, bed, bed_with};

fn claim(path: &str, line: u64, expected: &str) -> ValueClaim {
    ValueClaim {
        name: "case".to_owned(),
        path: RepoPath::new(path.to_owned()).expect("a claim path"),
        line,
        expected: expected.to_owned(),
    }
}

/// The verdict ladder, one rung at a time, over the fixture's mixed-line
/// file: `one\r\ntwo\nthree\rfour`.
#[test]
fn a_claim_answers_by_the_ladder() {
    let mut bed = bed();
    let mut verdict = |value: &ValueClaim| {
        resolve_claim(
            &bed.repo,
            &mut bed.git_resources,
            &mut bed.scan_resources,
            &mut bed.cache,
            &bed.snapshot,
            value,
        )
        .expect("a claim inside every ceiling resolves")
    };

    for (reason, line, expected) in [
        ("a CRLF line answers without its terminator", 1, "one"),
        ("an LF line answers without its terminator", 2, "two"),
        ("a CR line answers without its terminator", 3, "three"),
        ("the last line answers with no terminator at all", 4, "four"),
    ] {
        assert_eq!(
            verdict(&claim("src/lines.rs", line, expected)),
            ClaimVerdict::Attested,
            "{reason}"
        );
    }

    let broken = verdict(&claim("src/lines.rs", 2, "two "));
    assert!(
        matches!(broken, ClaimVerdict::Broken { .. }),
        "one trailing space breaks the claim: {broken:?}"
    );

    for (reason, value, expected_reason) in [
        (
            "a path nothing stands at",
            claim("src/absent.rs", 1, "x"),
            ClaimMissingReason::Absent,
        ),
        (
            "a directory is not a blob",
            claim("docs", 1, "x"),
            ClaimMissingReason::NotABlob,
        ),
        (
            "a symlink is not a blob",
            claim("alias", 1, "x"),
            ClaimMissingReason::NotABlob,
        ),
        (
            "a gitlink is not a blob",
            claim("module", 1, "x"),
            ClaimMissingReason::NotABlob,
        ),
        (
            "an LFS pointer holds no lines to answer with",
            claim("pointer.bin", 1, "x"),
            ClaimMissingReason::LfsPointer,
        ),
        (
            "a line past the end of the file",
            claim("src/lines.rs", 5, "x"),
            ClaimMissingReason::LineOutOfRange,
        ),
        (
            "an empty file has no first line",
            claim("src/empty.rs", 1, ""),
            ClaimMissingReason::LineOutOfRange,
        ),
    ] {
        assert_eq!(
            verdict(&value),
            ClaimVerdict::TargetMissing(expected_reason),
            "{reason}"
        );
    }
}

/// A claim's line selection charges the fragment meter once per file and
/// range, exactly like a reference line fragment, and a budget one byte
/// short refuses before any verdict is spoken.
#[test]
fn a_claim_selection_charges_the_fragment_meter_once() {
    let body = u64::try_from(MIXED_LINES.len()).expect("a small fixture");
    let mut bed = bed_with(ScanLimits {
        aggregate_line_fragment_evaluation_bytes_per_snapshot: body,
        ..ScanLimits::CONTRACT
    });
    let first = resolve_claim(
        &bed.repo,
        &mut bed.git_resources,
        &mut bed.scan_resources,
        &mut bed.cache,
        &bed.snapshot,
        &claim("src/lines.rs", 1, "one"),
    );
    assert_eq!(
        first.expect("the first selection fills the budget exactly"),
        ClaimVerdict::Attested
    );
    let memoized = resolve_claim(
        &bed.repo,
        &mut bed.git_resources,
        &mut bed.scan_resources,
        &mut bed.cache,
        &bed.snapshot,
        &claim("src/lines.rs", 1, "not one"),
    );
    assert!(
        matches!(memoized, Ok(ClaimVerdict::Broken { .. })),
        "the same range answers from the memo with nothing left to spend: {memoized:?}"
    );
    let second_range = resolve_claim(
        &bed.repo,
        &mut bed.git_resources,
        &mut bed.scan_resources,
        &mut bed.cache,
        &bed.snapshot,
        &claim("src/lines.rs", 2, "two"),
    );
    assert!(
        matches!(second_range, Err(Error::ResourceLimit { .. })),
        "a new range needs a budget the snapshot no longer has: {second_range:?}"
    );

    let mut short = bed_with(ScanLimits {
        aggregate_line_fragment_evaluation_bytes_per_snapshot: body.saturating_sub(1),
        ..ScanLimits::CONTRACT
    });
    let refused = resolve_claim(
        &short.repo,
        &mut short.git_resources,
        &mut short.scan_resources,
        &mut short.cache,
        &short.snapshot,
        &claim("src/lines.rs", 1, "one"),
    );
    assert!(
        matches!(refused, Err(Error::ResourceLimit { .. })),
        "one byte short refuses the very first selection: {refused:?}"
    );
}
