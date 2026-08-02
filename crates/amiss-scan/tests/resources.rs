use amiss_scan::resources::Aggregate;
use amiss_scan::{ScanLimits, ScanResources};
use amiss_wire::controls::ResourceName;

fn limits() -> ScanLimits {
    ScanLimits {
        document_blob_bytes: 10,
        aggregate_document_bytes_per_snapshot: 25,
        raw_link_destination_bytes: 4,
        parser_nesting: 3,
        parser_nodes_per_document: 5,
        parser_nodes_per_snapshot: 8,
        references_per_document: 2,
        references_per_snapshot: 3,
        aggregate_heading_anchor_evaluation_bytes_per_snapshot: 16,
        ..ScanLimits::CONTRACT
    }
}

fn resource_of(error: &amiss_scan::Error) -> Option<ResourceName> {
    match error {
        amiss_scan::Error::ResourceLimit { resource, .. } => Some(*resource),
        amiss_scan::Error::Parse(_)
        | amiss_scan::Error::Git(_)
        | amiss_scan::Error::UnrepresentablePath
        | amiss_scan::Error::Internal => None,
    }
}

/// Every ceiling admits its own value and refuses the next one, and the
/// refusal names the resource that was crossed.
#[test]
fn every_charge_admits_its_ceiling_and_refuses_the_next() {
    let mut scan = ScanResources::new(limits());
    assert!(
        scan.charge_document_bytes(10).is_ok(),
        "a document at its cap"
    );
    let crossed = scan.charge_document_bytes(11).unwrap_err();
    assert_eq!(
        resource_of(&crossed),
        Some(ResourceName::DocumentBlobBytes),
        "one byte past the per-document cap"
    );

    let mut scan = ScanResources::new(limits());
    assert!(
        scan.charge_work(5, 3).is_ok(),
        "nodes and nesting at their caps"
    );
    let nesting = scan.charge_work(1, 4).unwrap_err();
    assert_eq!(resource_of(&nesting), Some(ResourceName::ParserNesting));
    let nodes = scan.charge_work(6, 3).unwrap_err();
    assert_eq!(
        resource_of(&nodes),
        Some(ResourceName::ParserNodesPerDocument)
    );
    assert!(
        scan.charge_work(3, 1).is_ok(),
        "the snapshot total at its cap"
    );
    let snapshot = scan.charge_work(1, 1).unwrap_err();
    assert_eq!(
        resource_of(&snapshot),
        Some(ResourceName::ParserNodesPerSnapshot)
    );

    let mut scan = ScanResources::new(limits());
    assert!(
        scan.charge_reference(4, 2).is_ok(),
        "a destination at its cap"
    );
    let destination = scan.charge_reference(5, 0).unwrap_err();
    assert_eq!(
        resource_of(&destination),
        Some(ResourceName::RawLinkDestinationBytes)
    );
    let per_document = scan.charge_reference(1, 3).unwrap_err();
    assert_eq!(
        resource_of(&per_document),
        Some(ResourceName::ReferencesPerDocument)
    );
    assert_eq!(scan.references(), 1, "only the admitted reference counted");
    assert!(scan.charge_reference(1, 0).is_ok());
    assert!(scan.charge_reference(1, 0).is_ok());
    assert_eq!(scan.references(), 3, "the snapshot total at its cap");
    let per_snapshot = scan.charge_reference(1, 0).unwrap_err();
    assert_eq!(
        resource_of(&per_snapshot),
        Some(ResourceName::ReferencesPerSnapshot)
    );
}

/// The heading-anchor allowance is what the ceiling still has left, and it
/// closes exactly when the aggregate is spent.
#[test]
fn the_heading_anchor_allowance_is_what_remains() {
    let mut scan = ScanResources::new(limits());
    assert_eq!(scan.heading_anchor_allowance(), 16);
    scan.charge(Aggregate::HeadingAnchorBytes, 6).unwrap();
    assert_eq!(scan.heading_anchor_allowance(), 10);
    scan.charge(Aggregate::HeadingAnchorBytes, 10).unwrap();
    assert_eq!(scan.heading_anchor_allowance(), 0);
}
