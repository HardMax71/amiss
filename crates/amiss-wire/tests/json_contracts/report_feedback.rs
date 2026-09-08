use std::num::NonZeroU64;

use amiss_wire::{
    digest::hb,
    report::{
        PAYLOAD_SCHEMA, ReportDefect, emit_report,
        model::{Feedback, FeedbackItem, ReportEnvelope},
        validate_envelope,
    },
};
use wary::Validate as _;

const REPORT: &[u8] = include_bytes!("../../../../spec/examples/scanner-report.canonical.json");

#[test]
fn feedback_count_requires_a_nonzero_integer() -> Result<(), Box<dyn std::error::Error>> {
    let report: ReportEnvelope = serde_json::from_slice(REPORT)?;
    let Feedback::Available(feedback) = &report.payload.feedback else {
        panic!("the committed report has available feedback");
    };
    assert!(!feedback.items.is_empty());
    for item in &feedback.items {
        let text = serde_json::to_string(item)?;
        assert_eq!(serde_json::from_str::<FeedbackItem>(&text)?, *item);
        let member = format!("\"location_count\":{}", item.location_count);
        assert_eq!(text.matches(&member).count(), 1);
        for invalid in [
            "0", "-0", "-1", "1.0", "1e0", "\"1\"", "null", "false", "[]", "{}",
        ] {
            let changed = text.replacen(&member, &format!("\"location_count\":{invalid}"), 1);
            assert!(
                serde_json::from_str::<FeedbackItem>(&changed).is_err(),
                "{changed}"
            );
        }
        let missing = text.replacen(&format!("{member},"), "", 1);
        assert_ne!(text, missing);
        assert!(serde_json::from_str::<FeedbackItem>(&missing).is_err());
    }
    Ok(())
}

#[test]
fn feedback_count_bounds_hold_at_report_input_and_output() -> Result<(), Box<dyn std::error::Error>>
{
    for (count, valid) in [
        (1, true),
        (js_int::MAX_SAFE_UINT - 1, true),
        (js_int::MAX_SAFE_UINT, true),
        (js_int::MAX_SAFE_UINT + 1, false),
        (u64::MAX, false),
    ] {
        for empty_rows in [false, true] {
            let mut report: ReportEnvelope = serde_json::from_slice(REPORT)?;
            let Feedback::Available(feedback) = &mut report.payload.feedback else {
                panic!("the committed report has available feedback");
            };
            let mut item = feedback
                .items
                .first()
                .ok_or("missing feedback fixture")?
                .clone();
            item.location_count = NonZeroU64::try_from(count)?;
            assert_eq!(item.validate(&()).is_ok(), valid, "{count}");
            feedback.items.push(item);
            assert_eq!(feedback.validate(&()).is_ok(), valid, "{count}");
            assert_eq!(
                report.payload.feedback.validate(&()).is_ok(),
                valid,
                "{count}"
            );
            if empty_rows {
                report.payload.documents.clear();
                report.payload.findings.clear();
                report.payload.observations.clear();
            }
            let original_digest = report.payload_digest;
            let payload = serde_json_canonicalizer::to_vec(&report.payload)?;
            report.payload_digest = hb(PAYLOAD_SCHEMA, &payload);
            let prefix = b"existing output";
            let mut output = prefix.to_vec();
            let emitted = emit_report(&report, &mut output);
            if valid {
                let wire = &output[prefix.len()..];
                assert_eq!(emitted?, u64::try_from(wire.len())?);
                assert_eq!(&output[..prefix.len()], prefix);
                assert_eq!(validate_envelope(wire)?.0, report);
                continue;
            }
            assert_eq!(emitted.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
            assert_eq!(output, prefix);
            for digest in [report.payload_digest, original_digest] {
                report.payload_digest = digest;
                let wire = serde_json_canonicalizer::to_vec(&report)?;
                assert_eq!(
                    validate_envelope(&wire).map(drop),
                    Err(ReportDefect::NotAReport)
                );
            }
        }
    }
    Ok(())
}

#[test]
fn feedback_validation_accepts_borrowed_paths_and_unavailable_output()
-> Result<(), Box<dyn std::error::Error>> {
    let mut report: ReportEnvelope = serde_json::from_slice(REPORT)?;
    let Feedback::Available(feedback) = &report.payload.feedback else {
        panic!("the committed report has available feedback");
    };
    let item = feedback.items.first().ok_or("missing feedback fixture")?;
    let borrowed = FeedbackItem {
        action: item.action,
        annotation: item.annotation.clone(),
        effective_disposition: item.effective_disposition,
        finding_kinds: item.finding_kinds.clone(),
        location_count: item.location_count,
        target: item.target.as_ref(),
    };
    borrowed.validate(&())?;
    assert_eq!(serde_json::to_vec(&borrowed)?, serde_json::to_vec(item)?);
    report.payload.feedback =
        Feedback::Unavailable(amiss_wire::report::model::UnavailableFeedback {
            status: amiss_wire::report::model::UnavailableStatus::Unavailable,
        });
    report.payload.feedback.validate(&())?;
    report.payload_digest = hb(
        PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&report.payload)?,
    );
    let mut output = Vec::new();
    emit_report(&report, &mut output)?;
    assert_eq!(validate_envelope(&output)?.0, report);
    Ok(())
}
