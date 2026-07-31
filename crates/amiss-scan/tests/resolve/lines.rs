use amiss_scan::resolve::{RAW_EVIDENCE_DOMAIN, TARGET_LINE_PROJECTION_DOMAIN};
use amiss_scan::{Error, Resolution, ScanLimits};
use amiss_wire::controls::{GitMode, ResourceName};
use amiss_wire::digest::{hb, hj};
use amiss_wire::json::Value;
use amiss_wire::resolution::{BlobContent, BlobMode, Missing, Target, UnsupportedSemantics};

use crate::support::{MIXED_LINES, bed, bed_with, gitea_context, github_context, gitlab_context};

#[test]
fn line_fragments_have_a_hard_grammar() {
    let mut bed = bed();
    for destination in ["guide.md#L1", "guide.md#L1-L1"] {
        let row = bed
            .run(None, "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(&row, Resolution::Resolved(Target::Blob(_))),
            "{destination}: {row:?}"
        );
    }
    let row = bed
        .run(None, "docs/guide.md", false, "guide.md#L10-L20")
        .unwrap_or_else(|_defect| panic!("resolve out-of-range lines"))
        .1;
    assert!(matches!(
        row,
        Resolution::Missing(Missing::LineFragmentOutOfRange { .. })
    ));

    for renderer in ["L0", "l5", "L5-L2", "L", "L5x", "L05"] {
        let destination = format!("guide.md#{renderer}");
        let row = bed
            .run(None, "docs/guide.md", false, &destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        assert!(
            matches!(
                &row,
                Resolution::Missing(Missing::HeadingAnchorNotFound { .. })
            ),
            "{renderer} is not the line grammar, so it is read as a heading anchor: {row:?}"
        );
    }
}

fn expected_line_projection(mode: GitMode, selected: &[u8]) -> amiss_wire::digest::Digest {
    let selected_raw = hb(RAW_EVIDENCE_DOMAIN, selected);
    hj(
        TARGET_LINE_PROJECTION_DOMAIN,
        &Value::Object(vec![
            (
                "git_mode".to_owned(),
                Value::String(mode.as_str().to_owned()),
            ),
            (
                "raw_digest".to_owned(),
                Value::String(selected_raw.to_string()),
            ),
        ]),
    )
}

#[test]
fn line_selections_digest_the_exact_raw_inclusive_slice() {
    let mut bed = bed();
    let selections: [(&str, &[u8]); 5] = [
        ("L1", b"one\r\n"),
        ("L2", b"two\n"),
        ("L3", b"three\r"),
        ("L4", b"four"),
        ("L2-L4", b"two\nthree\rfour"),
    ];

    for (fragment, selected) in selections {
        let row = bed
            .run(
                None,
                "docs/guide.md",
                false,
                &format!("../src/lines.rs#{fragment}"),
            )
            .unwrap_or_else(|_defect| panic!("resolve {fragment}"))
            .1;
        let Resolution::Resolved(Target::Blob(blob)) = row else {
            panic!("unexpected resolution for {fragment}: {row:?}");
        };
        let BlobContent::Available {
            raw_digest,
            projection_digest,
        } = blob.content
        else {
            panic!("unexpected content for {fragment}: {:?}", blob.content);
        };
        assert_eq!(
            raw_digest,
            hb(RAW_EVIDENCE_DOMAIN, MIXED_LINES),
            "the evidence digest remains the complete target for {fragment}"
        );
        assert_eq!(
            projection_digest,
            expected_line_projection(GitMode::RegularFile, selected),
            "the target projection is the exact selected bytes for {fragment}"
        );
    }

    let complete = bed
        .run(None, "docs/guide.md", false, "../src/lines.rs")
        .unwrap_or_else(|_defect| panic!("resolve complete target"))
        .1;
    let all_lines = bed
        .run(None, "docs/guide.md", false, "../src/lines.rs#L1-L4")
        .unwrap_or_else(|_defect| panic!("resolve all lines"))
        .1;
    let Resolution::Resolved(Target::Blob(complete)) = complete else {
        panic!("unexpected complete-target resolution: {complete:?}");
    };
    let Resolution::Resolved(Target::Blob(all_lines)) = all_lines else {
        panic!("unexpected all-lines resolution: {all_lines:?}");
    };
    assert_ne!(
        all_lines.content.projection_digest(),
        complete.content.projection_digest(),
        "a line selection stays domain-separated even when it spans the complete target"
    );
    assert_eq!(
        all_lines.content.projection_digest(),
        Some(expected_line_projection(GitMode::RegularFile, MIXED_LINES))
    );
}

#[test]
fn line_projection_ignores_bytes_outside_the_selected_slice() {
    let mut bed = bed();
    let original = bed
        .run(None, "docs/guide.md", false, "../src/lines.rs#L2")
        .unwrap_or_else(|_defect| panic!("resolve original"))
        .1;
    let outside_changed = bed
        .run(
            None,
            "docs/guide.md",
            false,
            "../src/lines-outside-changed.rs#L2",
        )
        .unwrap_or_else(|_defect| panic!("resolve outside-changed"))
        .1;
    let Resolution::Resolved(Target::Blob(original)) = original else {
        panic!("unexpected original resolution: {original:?}");
    };
    let Resolution::Resolved(Target::Blob(outside_changed)) = outside_changed else {
        panic!("unexpected outside-changed resolution: {outside_changed:?}");
    };
    let BlobContent::Available {
        raw_digest: original_raw,
        projection_digest: original_projection,
    } = original.content
    else {
        panic!("unexpected original content: {:?}", original.content);
    };
    let BlobContent::Available {
        raw_digest: changed_raw,
        projection_digest: changed_projection,
    } = outside_changed.content
    else {
        panic!(
            "unexpected outside-changed content: {:?}",
            outside_changed.content
        );
    };
    assert_ne!(original_raw, changed_raw);
    assert_eq!(
        original_projection, changed_projection,
        "equal selected raw bytes stay equal when only bytes outside them differ"
    );
}

#[test]
fn executable_line_selections_bind_the_executable_mode() {
    let mut bed = bed();
    let row = bed
        .run(None, "docs/guide.md", false, "../src/executable.sh#L2")
        .unwrap_or_else(|_defect| panic!("resolve executable line"))
        .1;
    let Resolution::Resolved(Target::Blob(blob)) = row else {
        panic!("unexpected executable resolution: {row:?}");
    };
    assert_eq!(blob.mode, BlobMode::Executable);
    assert_eq!(
        blob.content.projection_digest(),
        Some(expected_line_projection(GitMode::ExecutableFile, b"two\n"))
    );
}

#[test]
fn line_selection_bounds_are_structural_missing_outcomes() {
    let mut bed = bed();
    for fragment in ["L5", "L4-L5", "L5-L5", "L9007199254740991"] {
        let row = bed
            .run(
                None,
                "docs/guide.md",
                false,
                &format!("../src/lines.rs#{fragment}"),
            )
            .unwrap_or_else(|_defect| panic!("resolve {fragment}"))
            .1;
        let Resolution::Missing(Missing::LineFragmentOutOfRange { path }) = row else {
            panic!("unexpected resolution for {fragment}: {row:?}");
        };
        assert_eq!(path.as_str(), Some("src/lines.rs"));
    }

    let empty = bed
        .run(None, "docs/guide.md", false, "../src/empty.rs#L1")
        .unwrap_or_else(|_defect| panic!("resolve empty target"))
        .1;
    assert!(matches!(
        empty,
        Resolution::Missing(Missing::LineFragmentOutOfRange { .. })
    ));

    for malformed in ["L0", "l2", "L", "L02", "L2-L1", "L2-3", "L9007199254740992"] {
        let row = bed
            .run(
                None,
                "docs/guide.md",
                false,
                &format!("../src/lines.rs#{malformed}"),
            )
            .unwrap_or_else(|_defect| panic!("resolve {malformed}"))
            .1;
        assert!(
            matches!(
                &row,
                Resolution::UnsupportedSemantics(UnsupportedSemantics::CodeFragment(_))
            ),
            "malformed line spelling {malformed} remains an unsupported code fragment: {row:?}"
        );
    }
}

#[test]
fn native_and_absolute_line_ranges_follow_the_declared_forge_dialect() {
    let mut bed = bed();
    let contexts = [github_context(), gitlab_context(), gitea_context()];
    let native_cases = [
        (&contexts[0], "L2-L3", "L2-3"),
        (&contexts[1], "L2-3", "L2-L3"),
        (&contexts[2], "L2-L3", "L2-3"),
    ];
    let expected = Some(expected_line_projection(
        GitMode::RegularFile,
        b"two\nthree\r",
    ));

    for (context, accepted, rejected) in native_cases {
        let row = bed
            .run(
                Some(context),
                "docs/guide.md",
                false,
                &format!("../src/lines.rs#{accepted}"),
            )
            .unwrap_or_else(|_defect| panic!("resolve {accepted}"))
            .1;
        let Resolution::Resolved(Target::Blob(blob)) = row else {
            panic!("unexpected resolution for {accepted}: {row:?}");
        };
        assert_eq!(blob.content.projection_digest(), expected, "{accepted}");

        let row = bed
            .run(
                Some(context),
                "docs/guide.md",
                false,
                &format!("../src/lines.rs#{rejected}"),
            )
            .unwrap_or_else(|_defect| panic!("resolve {rejected}"))
            .1;
        assert!(
            matches!(
                &row,
                Resolution::UnsupportedSemantics(UnsupportedSemantics::CodeFragment(_))
            ),
            "{rejected} is not the declared dialect's range spelling: {row:?}"
        );

        let out_of_range = bed
            .run(Some(context), "docs/guide.md", false, "../src/lines.rs#L5")
            .unwrap_or_else(|_defect| panic!("resolve out of range"))
            .1;
        assert!(matches!(
            out_of_range,
            Resolution::Missing(Missing::LineFragmentOutOfRange { .. })
        ));
    }

    let absolute_cases = [
        (
            &contexts[0],
            "https://github.com/acme/widgets/blob/feature/x/src/lines.rs#L2-L3",
        ),
        (
            &contexts[1],
            "https://gitlab.com/acme/widgets/-/blob/feature/x/src/lines.rs#L2-3",
        ),
        (
            &contexts[2],
            "https://codeberg.org/acme/widgets/src/branch/feature/x/src/lines.rs#L2-L3",
        ),
    ];
    for (context, destination) in absolute_cases {
        let row = bed
            .run(Some(context), "docs/guide.md", false, destination)
            .unwrap_or_else(|_defect| panic!("resolve {destination}"))
            .1;
        let Resolution::Resolved(Target::Blob(blob)) = row else {
            panic!("unexpected resolution for {destination}: {row:?}");
        };
        assert_eq!(blob.content.projection_digest(), expected, "{destination}");
    }
}

#[test]
fn distinct_line_selections_are_bounded_and_cached() {
    let target_bytes = u64::try_from(MIXED_LINES.len()).unwrap_or(u64::MAX);
    let mut bed = bed_with(ScanLimits {
        aggregate_line_fragment_evaluation_bytes_per_snapshot: target_bytes,
        ..ScanLimits::CONTRACT
    });

    assert!(
        bed.run(None, "docs/guide.md", false, "../src/lines.rs#L2")
            .is_ok()
    );
    assert_eq!(bed.scan_resources.line_fragment_bytes(), target_bytes);
    assert!(
        bed.run(None, "docs/guide.md", false, "../src/lines.rs#L2")
            .is_ok()
    );
    assert_eq!(
        bed.scan_resources.line_fragment_bytes(),
        target_bytes,
        "an identical selection reuses its cached projection"
    );

    let crossing = bed.run(None, "docs/guide.md", false, "../src/lines.rs#L3");
    assert_eq!(
        crossing,
        Err(Error::ResourceLimit {
            resource: ResourceName::AggregateLineFragmentEvaluationBytesPerSnapshot,
            configured_limit: target_bytes,
            observed_lower_bound: target_bytes.saturating_mul(2),
        })
    );

    let mut missing_bed = bed_with(ScanLimits {
        aggregate_line_fragment_evaluation_bytes_per_snapshot: target_bytes,
        ..ScanLimits::CONTRACT
    });
    let first_missing = missing_bed
        .run(None, "docs/guide.md", false, "../src/lines.rs#L5")
        .unwrap_or_else(|_defect| panic!("resolve first out-of-range selection"));
    let repeated_missing = missing_bed
        .run(None, "docs/guide.md", false, "../src/lines.rs#L5")
        .unwrap_or_else(|_defect| panic!("resolve cached out-of-range selection"));
    assert_eq!(first_missing, repeated_missing);
    assert_eq!(
        missing_bed.scan_resources.line_fragment_bytes(),
        target_bytes,
        "an out-of-range selection caches its absence as well as its charge"
    );
}
