use std::num::NonZeroU64;

use amiss_bootstrap::supervise::{AcceptanceDefect, accept};
use amiss_wire::{
    digest::hb,
    report::{
        PAYLOAD_SCHEMA,
        model::{Feedback, ReportEnvelope},
    },
};

use super::accepted_report;

#[test]
fn feedback_bounds_precede_canonicality_and_payload_digest()
-> Result<(), Box<dyn std::error::Error>> {
    let (wire, expectations) = accepted_report();
    let original: ReportEnvelope = serde_json::from_slice(&wire)?;
    let maximum = u64::try_from(amiss_wire::json::MAX_SAFE_INTEGER)?;
    for (count, shape) in [
        (1, Ok(())),
        (maximum, Ok(())),
        (maximum + 1, Err(AcceptanceDefect::Shape)),
        (u64::MAX, Err(AcceptanceDefect::Shape)),
    ] {
        let mut report = original.clone();
        let Feedback::Available(feedback) = &mut report.payload.feedback else {
            panic!("the committed report has available feedback");
        };
        let mut item = feedback
            .items
            .first()
            .ok_or("missing feedback fixture")?
            .clone();
        item.location_count = NonZeroU64::try_from(count)?;
        feedback.items.push(item);
        let digest = hb(
            PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&report.payload)?,
        );
        assert_ne!(digest, original.payload_digest);
        for (digest, identity) in [
            (digest, Ok(0)),
            (
                original.payload_digest,
                Err(AcceptanceDefect::PayloadDigest),
            ),
        ] {
            report.payload_digest = digest;
            let mut wire = serde_json_canonicalizer::to_vec(&report)?;
            wire.push(b'\n');
            assert_eq!(accept(&wire, &expectations), shape.and(identity), "{count}");
            assert_eq!(
                accept(&[b" ", wire.as_slice()].concat(), &expectations),
                shape.and(Err(AcceptanceDefect::Noncanonical)),
                "{count}"
            );
        }
    }
    Ok(())
}

#[test]
fn framing_and_complete_stream_checks_keep_distinct_defects() {
    let (wire, expectations) = accepted_report();
    let document = wire.strip_suffix(b"\n").unwrap();
    for (suffix, expected) in [
        (b"\n".as_slice(), Ok(0)),
        (b"", Err(AcceptanceDefect::Noncanonical)),
        (b" ", Err(AcceptanceDefect::Noncanonical)),
        (b" \n", Err(AcceptanceDefect::Noncanonical)),
        (b"\n\n", Err(AcceptanceDefect::Noncanonical)),
        (b"\r\n", Err(AcceptanceDefect::Noncanonical)),
        (b"true\n", Err(AcceptanceDefect::Shape)),
        (b"{}\n", Err(AcceptanceDefect::Shape)),
        (b"null\n", Err(AcceptanceDefect::Shape)),
        (b"// comment\n", Err(AcceptanceDefect::Shape)),
        (b"\xff\n", Err(AcceptanceDefect::Shape)),
    ] {
        assert_eq!(
            accept(&[document, suffix].concat(), &expectations),
            expected,
            "{suffix:?}"
        );
    }
    for bytes in [
        [wire.as_slice(), wire.as_slice()].concat(),
        [b"\xef\xbb\xbf", wire.as_slice()].concat(),
        [b"\xff", wire.as_slice()].concat(),
        [b"{not-json".as_slice(), b"\n"].concat(),
        b"\n".to_vec(),
    ] {
        assert_eq!(accept(&bytes, &expectations), Err(AcceptanceDefect::Shape));
    }
    assert_eq!(
        accept(b"{not-json", &expectations),
        Err(AcceptanceDefect::Noncanonical)
    );
}

#[test]
fn nested_feedback_and_extensions_are_bounded_before_binding()
-> Result<(), Box<dyn std::error::Error>> {
    let (wire, expectations) = accepted_report();
    let report: ReportEnvelope = serde_json::from_slice(&wire)?;
    let text = std::str::from_utf8(&wire)?;
    let feedback = serde_json_canonicalizer::to_string(&report.payload.feedback)?;
    assert_eq!(text.matches(&feedback).count(), 1);
    for depth in [127, 128, 256, 512, 513, 4096] {
        let nested = format!("{}null{}", "[".repeat(depth), "]".repeat(depth));
        for changed in [
            text.replacen(&feedback, &nested, 1),
            text.replacen('{', &format!("{{\"future\":{nested},"), 1),
        ] {
            assert_eq!(
                accept(changed.as_bytes(), &expectations),
                Err(AcceptanceDefect::Shape),
                "depth {depth}"
            );
        }
    }
    Ok(())
}
