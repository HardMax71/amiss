use amiss_wire::report::{ReportDefect, model::ReportEnvelope, validate_envelope};

const REPORT: &[u8] = include_bytes!("../../../../spec/examples/scanner-report.canonical.json");

#[test]
fn report_ingress_rejects_invalid_text_and_incomplete_streams() {
    let text = std::str::from_utf8(REPORT).unwrap();
    let member = "\"compatibility\":\"1\"";
    assert_eq!(text.matches(member).count(), 1);
    for value in [
        "\"\\uD800\"",
        "\"\\uDC00\"",
        "\"\\x31\"",
        "\"1\n\"",
        "\"1\u{0}\"",
    ] {
        let changed = text.replace(member, &format!("\"compatibility\":{value}"));
        assert_eq!(
            validate_envelope(changed.as_bytes()).map(drop),
            Err(ReportDefect::NotAReport)
        );
    }
    for bytes in [
        Vec::new(),
        REPORT[..REPORT.len() / 2].to_vec(),
        [b"\xef\xbb\xbf".as_slice(), REPORT].concat(),
        [REPORT, b"\xff"].concat(),
        [REPORT, REPORT].concat(),
        [REPORT, b"true"].concat(),
        [REPORT, b"// comment"].concat(),
    ] {
        assert_eq!(
            validate_envelope(&bytes).map(drop),
            Err(ReportDefect::NotAReport)
        );
    }
}

#[test]
fn report_ingress_bounds_nested_input_before_canonical_comparison() {
    let report: ReportEnvelope = serde_json::from_slice(REPORT).unwrap();
    let text = std::str::from_utf8(REPORT).unwrap();
    let feedback = serde_json_canonicalizer::to_string(&report.payload.feedback).unwrap();
    assert_eq!(text.matches(&feedback).count(), 1);
    for depth in [127, 128, 256, 512, 513, 4096] {
        let nested = format!("{}null{}", "[".repeat(depth), "]".repeat(depth));
        for changed in [
            text.replacen(&feedback, &nested, 1),
            text.replacen('{', &format!("{{\"future\":{nested},"), 1),
        ] {
            assert_eq!(
                validate_envelope(changed.as_bytes()).map(drop),
                Err(ReportDefect::NotAReport),
                "depth {depth}"
            );
        }
    }
}
