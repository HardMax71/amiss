use amiss_wire::{
    de::{Error, ErrorKind},
    digest::hb,
    external::{
        self, AssessDefect, EvidenceDefect, ExternalEvidence, ExternalEvidenceRow, ForgeRepository,
        ForgeTail, ProbeFailure, ProbeMethod,
    },
    json,
};

const EVIDENCE: &[u8] = include_bytes!("../../../../spec/examples/scanner-external-evidence.json");
const PLAN: &[u8] = include_bytes!("../../../../spec/examples/scanner-external-plan.json");

#[test]
fn evidence_models_reject_unknown_fields() {
    let mut document: ExternalEvidence = serde_json::from_slice(EVIDENCE).unwrap();
    document.rows.extend([
        ExternalEvidenceRow::HttpProbe {
            destination: "https://example.com/unavailable".to_owned(),
            method: ProbeMethod::Get,
            status: None,
            failure: Some(ProbeFailure::Tls),
            final_destination: None,
            redirect_chain_permanent: None,
            checked_at: "2026-08-14T00:00:00Z".to_owned(),
        },
        ExternalEvidenceRow::ForgeApi {
            destination: "https://github.com/acme/widgets".to_owned(),
            repository: ForgeRepository::Readable,
            tail: Some(ForgeTail::Resolved),
            checked_at: "2026-08-14T00:00:00Z".to_owned(),
        },
    ]);
    let wire = String::from_utf8(external::evidence(&document).unwrap()).unwrap();
    assert_eq!(
        external::parse_evidence(wire.as_bytes()).unwrap().0,
        document
    );
    for (offset, _) in wire.match_indices('{') {
        let mut extended = wire.clone();
        extended.insert_str(offset + 1, "\"future\":true,");
        assert_eq!(
            [
                serde_json::from_str::<ExternalEvidence>(&extended).is_err(),
                matches!(
                    external::parse_evidence(extended.as_bytes()),
                    Err(EvidenceDefect::Wire(Error {
                        kind: ErrorKind::UnknownField,
                        ..
                    }))
                ),
            ],
            [true; 2],
            "{extended}"
        );
    }
    let extended = std::str::from_utf8(EVIDENCE)
        .unwrap()
        .replacen('{', "{\"future\":true,", 1);
    assert!(matches!(
        external::assess(PLAN, extended.as_bytes(), "0.0.0", hb("test", b"engine")),
        Err(AssessDefect::Evidence(_))
    ));
}

#[test]
fn evidence_does_not_normalize_positional_data() {
    let mut document: ExternalEvidence = serde_json::from_slice(EVIDENCE).unwrap();
    document.rows = vec![ExternalEvidenceRow::ForgeApi {
        destination: "https://github.com/acme/widgets".to_owned(),
        repository: ForgeRepository::Readable,
        tail: Some(ForgeTail::Resolved),
        checked_at: "2026-08-14T00:00:00Z".to_owned(),
    }];
    let wire = String::from_utf8(external::evidence(&document).unwrap()).unwrap();
    for (object, positional) in [
        (
            serde_json_canonicalizer::to_vec(&document).unwrap(),
            serde_json::to_string(&(
                document.schema,
                document.plan_payload_digest,
                &document.producer,
                &document.rows,
            ))
            .unwrap(),
        ),
        (
            serde_json_canonicalizer::to_vec(&document.producer).unwrap(),
            serde_json::to_string(&(&document.producer.name, &document.producer.version)).unwrap(),
        ),
        (
            serde_json_canonicalizer::to_vec(&document.rows[0]).unwrap(),
            serde_json::to_string(&(
                "forge-api",
                "https://github.com/acme/widgets",
                ForgeRepository::Readable,
                ForgeTail::Resolved,
                "2026-08-14T00:00:00Z",
            ))
            .unwrap(),
        ),
    ] {
        let object = String::from_utf8(object).unwrap();
        let changed = wire.replace(&object, &positional);
        assert_ne!(changed, wire);
        assert!(
            external::parse_evidence(changed.as_bytes()).is_err(),
            "{changed}"
        );
    }
}

#[test]
fn assessments_use_the_verified_evidence_identity() {
    let mut document: ExternalEvidence = serde_json::from_slice(EVIDENCE).unwrap();
    let original_digest = external::parse_evidence(EVIDENCE).unwrap().1;
    document.producer.name = "probe \"quoted\" \\ \n \t 😀 \u{e000}".to_owned();
    let canonical = external::evidence(&document).unwrap();
    let expected = hb(external::EVIDENCE_SCHEMA, &canonical);
    assert_ne!(expected, original_digest);
    for bytes in [
        canonical,
        serde_json::to_vec_pretty(&document).unwrap(),
        serde_json::to_string(&document)
            .unwrap()
            .replace("https://", "https:\\/\\/")
            .replace("producer", "pro\\u0064ucer")
            .into_bytes(),
    ] {
        let (parsed, digest) = external::parse_evidence(&bytes).unwrap();
        assert_eq!(parsed, document);
        assert_eq!(digest, expected);
        let assessment = external::assess(PLAN, &bytes, "0.0.0", hb("test", b"engine")).unwrap();
        assert_eq!(
            external::parse_assessment(&assessment)
                .unwrap()
                .payload
                .subject
                .evidence_digest,
            expected
        );
    }
    document.producer.version.push_str("-changed");
    assert_ne!(
        external::parse_evidence(&external::evidence(&document).unwrap())
            .unwrap()
            .1,
        expected
    );
    let mut trailing = external::evidence(&document).unwrap();
    trailing.extend_from_slice(b" null");
    assert!(matches!(
        external::assess(PLAN, &trailing, "0.0.0", hb("test", b"engine")),
        Err(AssessDefect::Evidence(_))
    ));
}

#[test]
fn evidence_keeps_derived_validation_and_nonnull_optional_fields() {
    let mut document: ExternalEvidence = serde_json::from_slice(EVIDENCE).unwrap();
    let row =
        String::from_utf8(serde_json_canonicalizer::to_vec(&document.rows[0]).unwrap()).unwrap();
    let wire = String::from_utf8(external::evidence(&document).unwrap()).unwrap();
    let without = serde_json::to_string(&ExternalEvidenceRow::HttpProbe {
        destination: "https://example.com/manual".to_owned(),
        method: ProbeMethod::Get,
        status: None,
        failure: None,
        final_destination: None,
        redirect_chain_permanent: None,
        checked_at: "2026-08-14T00:00:00Z".to_owned(),
    })
    .unwrap();
    for field in [
        "status",
        "failure",
        "final_destination",
        "redirect_chain_permanent",
    ] {
        let malformed = without.replacen('{', &format!("{{\"{field}\":null,"), 1);
        let changed = wire.replace(&row, &malformed);
        assert_ne!(changed, wire);
        assert!(matches!(
            external::parse_evidence(changed.as_bytes()),
            Err(EvidenceDefect::Wire(Error {
                kind: ErrorKind::WrongType,
                ..
            }))
        ));
    }
    document.producer.name.clear();
    assert!(matches!(
        external::parse_evidence(&serde_json::to_vec(&document).unwrap()),
        Err(EvidenceDefect::Contract(_))
    ));
    assert!(matches!(
        external::evidence(&document),
        Err(EvidenceDefect::Contract(_))
    ));
}

#[test]
fn evidence_capture_keeps_strict_bounds_and_requires_an_object() {
    let document: ExternalEvidence = serde_json::from_slice(EVIDENCE).unwrap();
    let wire = serde_json::to_string(&document).unwrap();
    let nested = format!("{}null{}", "[".repeat(511), "]".repeat(511));
    let bounded = wire.replacen('{', &format!("{{\"future\":{nested},"), 1);
    assert!(matches!(
        external::parse_evidence(bounded.as_bytes()),
        Err(EvidenceDefect::Wire(Error {
            kind: ErrorKind::UnknownField,
            ..
        }))
    ));
    let too_deep = bounded.replace(&nested, &format!("[{nested}]"));
    assert!(matches!(
        external::parse_evidence(too_deep.as_bytes()),
        Err(EvidenceDefect::Wire(Error {
            kind: ErrorKind::Json(json::Error {
                kind: json::ErrorKind::DepthLimit,
                ..
            }),
            ..
        }))
    ));
    for invalid in [
        br#"{"future":0,"\u0066uture":1}"#.as_slice(),
        br#"{"future":-0}"#,
        br#"{"future":0.5}"#,
        br#"{"future":1e0}"#,
        br#"{"future":9007199254740992}"#,
        b"{} {}",
        b"\xff",
    ] {
        assert!(matches!(
            external::parse_evidence(invalid),
            Err(EvidenceDefect::Wire(Error {
                kind: ErrorKind::Json(_),
                ..
            }))
        ));
    }
    let oversized = vec![b' '; usize::try_from(external::EXTERNAL_DOCUMENT_BYTES + 1).unwrap()];
    assert!(matches!(
        external::parse_evidence(&oversized),
        Err(EvidenceDefect::Wire(Error {
            kind: ErrorKind::LimitExceeded,
            ..
        }))
    ));
}
