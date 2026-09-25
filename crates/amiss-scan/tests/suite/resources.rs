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
        projection_assertions_per_snapshot: 2,
        aggregate_projection_selected_bytes_per_snapshot: 5,
        projection_records_compared_per_snapshot: 3,
        aggregate_projection_projected_bytes_per_snapshot: 7,
        aggregate_projection_preview_bytes_per_snapshot: 4,
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
    assert!(scan.charge_reference(2).is_ok(), "a document at its cap");
    let per_document = scan.charge_reference(3).unwrap_err();
    assert_eq!(
        resource_of(&per_document),
        Some(ResourceName::ReferencesPerDocument)
    );
    assert_eq!(scan.references(), 1, "only the admitted reference counted");
    assert!(scan.charge_reference(0).is_ok());
    assert!(scan.charge_reference(0).is_ok());
    assert_eq!(scan.references(), 3, "the snapshot total at its cap");
    let per_snapshot = scan.charge_reference(0).unwrap_err();
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

#[test]
fn projection_totals_cross_independently() {
    for (aggregate, ceiling, resource) in [
        (
            Aggregate::ProjectionAssertions,
            2,
            ResourceName::ProjectionAssertionsPerSnapshot,
        ),
        (
            Aggregate::ProjectionSelectedBytes,
            5,
            ResourceName::AggregateProjectionSelectedBytesPerSnapshot,
        ),
        (
            Aggregate::ProjectionComparedRecords,
            3,
            ResourceName::ProjectionRecordsComparedPerSnapshot,
        ),
        (
            Aggregate::ProjectionProjectedBytes,
            7,
            ResourceName::AggregateProjectionProjectedBytesPerSnapshot,
        ),
        (
            Aggregate::ProjectionPreviewBytes,
            4,
            ResourceName::AggregateProjectionPreviewBytesPerSnapshot,
        ),
    ] {
        let mut scan = ScanResources::new(limits());
        assert!(scan.charge(aggregate, ceiling).is_ok(), "{resource:?}");
        let crossing = scan.charge(aggregate, 1).unwrap_err();
        assert_eq!(resource_of(&crossing), Some(resource));
    }
}

/// The document budget and the address-space ceiling the binary imposes on
/// itself are one pair, and nothing else checks that they agree. Snapshots
/// from 10 to 87 MiB of documents were measured peaking at about ten times
/// their document bytes over a fixed 150 MiB base, so a budget over a twelfth
/// of the ceiling aborts in the allocator before this limit can report.
#[test]
fn the_document_budget_fits_inside_the_address_space_the_engine_allows_itself() {
    let budget = ScanLimits::CONTRACT.aggregate_document_bytes_per_snapshot;
    assert!(
        budget.saturating_mul(12) <= amiss_wire::report::EVALUATOR_MANAGED_MEMORY_BYTES,
        "{budget} document bytes does not fit under the {} ceiling",
        amiss_wire::report::EVALUATOR_MANAGED_MEMORY_BYTES
    );
}
