use amiss_md::Fault;
use amiss_scan::{
    Error, RAW_DESTINATION_DOMAIN, SOURCE_PROJECTION_DOMAIN, ScanLimits, ScanResources,
    scan::normalize_newlines, scan_document,
};
use amiss_wire::controls::ResourceName;
use amiss_wire::digest::hb;
use amiss_wire::model::Adapter;

fn contract() -> ScanResources {
    ScanResources::new(ScanLimits::CONTRACT)
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn scanned(source: &str) -> amiss_scan::Scanned {
    scan_document(&mut contract(), Adapter::Markdown, source.as_bytes()).expect("scan")
}

#[test]
fn a_scanned_occurrence_carries_digests_and_display() {
    let source = "s\u{1f600}\t[a](b) end\r\nnext line\r\n";
    let got = scanned(source);
    assert_eq!(got.occurrences.len(), 1);
    let Some(entry) = got.occurrences.first() else {
        return;
    };
    assert_eq!(entry.occurrence.span, (6, 12));
    assert_eq!(entry.display.start_line, 1);
    assert_eq!(
        entry.display.start_column, 4,
        "the emoji and the tab are one scalar each"
    );
    assert_eq!(entry.display.end_line, 1);
    assert_eq!(entry.display.end_column, 10);
    assert_eq!(
        entry.raw_destination_digest,
        hb(RAW_DESTINATION_DOMAIN, b"b")
    );
    let block = "s\u{1f600}\t[a](b) end\nnext line";
    assert_eq!(
        entry.projection_digest,
        hb(SOURCE_PROJECTION_DOMAIN, block.as_bytes()),
        "the block is the whole lazily continued paragraph, endings normalized"
    );
}

#[test]
fn the_projection_normalizes_endings_and_nothing_else() {
    assert_eq!(normalize_newlines(b"a\r\nb\rc\nd"), b"a\nb\nc\nd".to_vec());
    assert_eq!(normalize_newlines(b"a\r\n"), b"a\n".to_vec());
    assert_eq!(normalize_newlines(b"\r\r\n\n"), b"\n\n\n".to_vec());
    assert_eq!(
        normalize_newlines("t\u{e9}xt  ".as_bytes()),
        "t\u{e9}xt  ".as_bytes().to_vec()
    );

    let crlf = scanned("- [a](b)\r\n");
    let lf = scanned("- [a](b)\n");
    assert_eq!(
        crlf.occurrences
            .first()
            .map(|entry| entry.projection_digest),
        lf.occurrences.first().map(|entry| entry.projection_digest),
        "one block, one projection digest, either ending"
    );
}

#[test]
fn display_positions_count_scalars_across_lines() {
    let source = "one\r\ntwo [x](y)\nthree\n";
    let got = scanned(source);
    let Some(entry) = got.occurrences.first() else {
        return;
    };
    assert_eq!(entry.display.start_line, 2);
    assert_eq!(entry.display.start_column, 5);
    assert_eq!(entry.display.end_line, 2);
    assert_eq!(entry.display.end_column, 11);
}

#[test]
fn an_empty_destination_hashes_zero_bytes() {
    let got = scanned("[a]()\n");
    assert_eq!(
        got.occurrences
            .first()
            .map(|entry| entry.raw_destination_digest),
        Some(hb(RAW_DESTINATION_DOMAIN, b""))
    );
}

#[test]
fn plain_advisory_charges_work_and_extracts_nothing() {
    let mut resources = contract();
    let got = scan_document(&mut resources, Adapter::PlainAdvisory, b"a\n\nb\n")
        .unwrap_or_else(|_defect| unreachable_scan());
    assert_eq!(got.occurrences, Vec::new());
    assert_eq!(got.work.nodes, 3);
    assert_eq!(resources.nodes(), 3);
    assert_eq!(resources.documents(), 1);
}

#[expect(clippy::panic, reason = "test fixture helper")]
fn unreachable_scan() -> amiss_scan::Scanned {
    panic!("plain advisory cannot fail")
}

#[test]
fn invalid_utf8_is_a_parse_fault_after_admission() {
    let got = scan_document(&mut contract(), Adapter::Markdown, &[0xff, 0xfe]);
    assert_eq!(got, Err(Error::Parse(Fault::DocumentInvalid)));
}

#[test]
fn document_admission_checks_count_then_value_then_aggregate() {
    let limits = ScanLimits {
        documents_per_snapshot: 2,
        document_blob_bytes: 8,
        aggregate_document_bytes_per_snapshot: 10,
        ..ScanLimits::CONTRACT
    };
    let mut resources = ScanResources::new(limits);

    assert!(scan_document(&mut resources, Adapter::Markdown, b"abc\n").is_ok());
    let oversized = scan_document(&mut resources, Adapter::Markdown, &[0xff; 9]);
    assert_eq!(
        oversized,
        Err(Error::ResourceLimit {
            resource: ResourceName::DocumentBlobBytes,
            configured_limit: 8,
            observed_lower_bound: 9,
        }),
        "the per-value check reports the exact size before parsing touches the bytes"
    );
    assert_eq!(
        resources.document_bytes(),
        4,
        "a member rejected by its per-value limit is never charged to the aggregate"
    );

    let third = scan_document(&mut resources, Adapter::Markdown, b"12345678");
    assert_eq!(
        third,
        Err(Error::ResourceLimit {
            resource: ResourceName::DocumentsPerSnapshot,
            configured_limit: 2,
            observed_lower_bound: 3,
        }),
        "the count crossing observes exactly one past the limit"
    );
}

#[test]
fn the_aggregate_observes_prior_total_plus_crossing_member() {
    let limits = ScanLimits {
        aggregate_document_bytes_per_snapshot: 10,
        ..ScanLimits::CONTRACT
    };
    let mut resources = ScanResources::new(limits);
    assert!(scan_document(&mut resources, Adapter::Markdown, b"123456\n").is_ok());
    let second = scan_document(&mut resources, Adapter::Markdown, b"1234567\n");
    assert_eq!(
        second,
        Err(Error::ResourceLimit {
            resource: ResourceName::AggregateDocumentBytesPerSnapshot,
            configured_limit: 10,
            observed_lower_bound: 15,
        })
    );
}

#[test]
fn nesting_and_node_counts_observe_limit_plus_one() {
    let limits = ScanLimits {
        parser_nesting: 3,
        ..ScanLimits::CONTRACT
    };
    let mut resources = ScanResources::new(limits);
    let got = scan_document(&mut resources, Adapter::Markdown, b"> > > deep\n");
    assert_eq!(
        got,
        Err(Error::ResourceLimit {
            resource: ResourceName::ParserNesting,
            configured_limit: 3,
            observed_lower_bound: 4,
        })
    );

    let limits = ScanLimits {
        parser_nodes_per_document: 2,
        ..ScanLimits::CONTRACT
    };
    let mut resources = ScanResources::new(limits);
    let got = scan_document(&mut resources, Adapter::Markdown, b"words\n");
    assert_eq!(
        got,
        Err(Error::ResourceLimit {
            resource: ResourceName::ParserNodesPerDocument,
            configured_limit: 2,
            observed_lower_bound: 3,
        })
    );

    let limits = ScanLimits {
        parser_nodes_per_snapshot: 5,
        ..ScanLimits::CONTRACT
    };
    let mut resources = ScanResources::new(limits);
    assert!(scan_document(&mut resources, Adapter::Markdown, b"a\n").is_ok());
    let second = scan_document(&mut resources, Adapter::Markdown, b"b\n");
    assert_eq!(
        second,
        Err(Error::ResourceLimit {
            resource: ResourceName::ParserNodesPerSnapshot,
            configured_limit: 5,
            observed_lower_bound: 6,
        })
    );
}

#[test]
fn reference_budgets_and_destination_bytes_charge_in_document_order() {
    let limits = ScanLimits {
        references_per_document: 2,
        ..ScanLimits::CONTRACT
    };
    let mut resources = ScanResources::new(limits);
    let got = scan_document(&mut resources, Adapter::Markdown, b"[a](1) [b](2) [c](3)\n");
    assert_eq!(
        got,
        Err(Error::ResourceLimit {
            resource: ResourceName::ReferencesPerDocument,
            configured_limit: 2,
            observed_lower_bound: 3,
        })
    );

    let limits = ScanLimits {
        raw_link_destination_bytes: 4,
        ..ScanLimits::CONTRACT
    };
    let mut resources = ScanResources::new(limits);
    let got = scan_document(&mut resources, Adapter::Markdown, b"[a](abcdef)\n");
    assert_eq!(
        got,
        Err(Error::ResourceLimit {
            resource: ResourceName::RawLinkDestinationBytes,
            configured_limit: 4,
            observed_lower_bound: 6,
        }),
        "a per-value byte resource observes the exact declared length"
    );

    let limits = ScanLimits {
        references_per_snapshot: 1,
        ..ScanLimits::CONTRACT
    };
    let mut resources = ScanResources::new(limits);
    assert!(scan_document(&mut resources, Adapter::Markdown, b"[a](1)\n").is_ok());
    let second = scan_document(&mut resources, Adapter::Markdown, b"[b](2)\n");
    assert_eq!(
        second,
        Err(Error::ResourceLimit {
            resource: ResourceName::ReferencesPerSnapshot,
            configured_limit: 1,
            observed_lower_bound: 2,
        })
    );
}

#[test]
fn errors_map_to_their_analysis_codes() {
    use amiss_wire::report::AnalysisErrorCode;
    assert_eq!(
        Error::Parse(Fault::DocumentInvalid).code(),
        AnalysisErrorCode::DocumentInvalid
    );
    assert_eq!(
        Error::Parse(Fault::ParserPanic).code(),
        AnalysisErrorCode::ParserPanic
    );
    assert_eq!(
        Error::ResourceLimit {
            resource: ResourceName::ParserNesting,
            configured_limit: 1,
            observed_lower_bound: 2,
        }
        .code(),
        AnalysisErrorCode::ResourceLimitExceeded
    );
}
