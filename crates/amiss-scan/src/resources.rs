use amiss_wire::controls::ResourceName;

use crate::Error;

/// The built-in discovery and parse ceilings. A future organization floor may
/// tighten them and may never raise them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanLimits {
    pub documents_per_snapshot: u64,
    pub document_blob_bytes: u64,
    pub aggregate_document_bytes_per_snapshot: u64,
    pub raw_link_destination_bytes: u64,
    pub parser_nesting: u64,
    pub parser_nodes_per_document: u64,
    pub parser_nodes_per_snapshot: u64,
    pub references_per_document: u64,
    pub references_per_snapshot: u64,
    pub referenced_target_blob_bytes: u64,
    pub aggregate_referenced_target_bytes_per_snapshot: u64,
    pub selected_control_blob_bytes: u64,
    pub aggregate_selected_control_bytes_per_snapshot: u64,
    pub control_input_bytes: u64,
    pub repository_policy_entries: u64,
    pub debt_items: u64,
    pub waiver_items: u64,
    pub errors_retained: u64,
    pub complete_findings: u64,
}

impl ScanLimits {
    pub const CONTRACT: Self = Self {
        documents_per_snapshot: 100_000,
        document_blob_bytes: 4_194_304,
        aggregate_document_bytes_per_snapshot: 536_870_912,
        raw_link_destination_bytes: 16_384,
        parser_nesting: 256,
        parser_nodes_per_document: 250_000,
        parser_nodes_per_snapshot: 5_000_000,
        references_per_document: 4_096,
        references_per_snapshot: 1_000_000,
        referenced_target_blob_bytes: 16_777_216,
        aggregate_referenced_target_bytes_per_snapshot: 536_870_912,
        selected_control_blob_bytes: 16_777_216,
        aggregate_selected_control_bytes_per_snapshot: 67_108_864,
        control_input_bytes: 16_777_216,
        repository_policy_entries: 100_000,
        debt_items: 100_000,
        waiver_items: 100_000,
        errors_retained: 64,
        complete_findings: 100_000,
    };
}

/// Snapshot-scoped charge state. Count resources observe exactly one past the
/// limit and stop; per-value byte resources observe the exact declared value;
/// an aggregate observes the prior charged total plus the first crossing
/// member, and a member rejected by its per-value limit is never charged to
/// the aggregate.
#[derive(Clone, Debug)]
pub struct ScanResources {
    limits: ScanLimits,
    documents: u64,
    document_bytes: u64,
    nodes: u64,
    references: u64,
    target_bytes: u64,
    control_bytes: u64,
}

pub(crate) const fn crossing(
    resource: ResourceName,
    configured_limit: u64,
    observed_lower_bound: u64,
) -> Error {
    Error::ResourceLimit {
        resource,
        configured_limit,
        observed_lower_bound,
    }
}

impl ScanResources {
    #[must_use]
    pub const fn new(limits: ScanLimits) -> Self {
        Self {
            limits,
            documents: 0,
            document_bytes: 0,
            nodes: 0,
            references: 0,
            target_bytes: 0,
            control_bytes: 0,
        }
    }

    #[must_use]
    pub const fn limits(&self) -> &ScanLimits {
        &self.limits
    }

    #[must_use]
    pub const fn documents(&self) -> u64 {
        self.documents
    }

    #[must_use]
    pub const fn document_bytes(&self) -> u64 {
        self.document_bytes
    }

    #[must_use]
    pub const fn nodes(&self) -> u64 {
        self.nodes
    }

    #[must_use]
    pub const fn references(&self) -> u64 {
        self.references
    }

    #[must_use]
    pub const fn target_bytes(&self) -> u64 {
        self.target_bytes
    }

    /// Charges one selected control blob's declared size to the snapshot
    /// aggregate; the per-value cap is enforced where the read happens.
    ///
    /// # Errors
    ///
    /// The aggregate crossing, observing the prior total plus this member.
    pub fn charge_control_bytes(&mut self, declared_bytes: u64) -> Result<(), Error> {
        let total = self.control_bytes.saturating_add(declared_bytes);
        if total > self.limits.aggregate_selected_control_bytes_per_snapshot {
            return Err(crossing(
                ResourceName::AggregateSelectedControlBytesPerSnapshot,
                self.limits.aggregate_selected_control_bytes_per_snapshot,
                total,
            ));
        }
        self.control_bytes = total;
        Ok(())
    }

    /// Charges one referenced target's declared byte size to the snapshot
    /// aggregate; the per-value cap is enforced where the read happens.
    ///
    /// # Errors
    ///
    /// The aggregate crossing, observing the prior charged total plus this
    /// member.
    pub fn charge_target_bytes(&mut self, declared_bytes: u64) -> Result<(), Error> {
        let total = self.target_bytes.saturating_add(declared_bytes);
        if total > self.limits.aggregate_referenced_target_bytes_per_snapshot {
            return Err(crossing(
                ResourceName::AggregateReferencedTargetBytesPerSnapshot,
                self.limits.aggregate_referenced_target_bytes_per_snapshot,
                total,
            ));
        }
        self.target_bytes = total;
        Ok(())
    }

    /// Admits one selected document of `declared_bytes`.
    ///
    /// # Errors
    ///
    /// The document count, per-document byte, or aggregate byte crossing,
    /// checked in that order.
    pub fn charge_document(&mut self, declared_bytes: u64) -> Result<(), Error> {
        self.admit_document()?;
        self.charge_document_bytes(declared_bytes)
    }

    /// Counts one selected document, before its bytes are read.
    ///
    /// # Errors
    ///
    /// The document count crossing.
    pub fn admit_document(&mut self) -> Result<(), Error> {
        self.documents = self.documents.saturating_add(1);
        if self.documents > self.limits.documents_per_snapshot {
            return Err(crossing(
                ResourceName::DocumentsPerSnapshot,
                self.limits.documents_per_snapshot,
                self.limits.documents_per_snapshot.saturating_add(1),
            ));
        }
        Ok(())
    }

    /// Charges one admitted document's declared byte size.
    ///
    /// # Errors
    ///
    /// The per-document byte crossing, then the aggregate crossing; a member
    /// rejected by the first is never charged to the second.
    pub fn charge_document_bytes(&mut self, declared_bytes: u64) -> Result<(), Error> {
        if declared_bytes > self.limits.document_blob_bytes {
            return Err(crossing(
                ResourceName::DocumentBlobBytes,
                self.limits.document_blob_bytes,
                declared_bytes,
            ));
        }
        let total = self.document_bytes.saturating_add(declared_bytes);
        if total > self.limits.aggregate_document_bytes_per_snapshot {
            return Err(crossing(
                ResourceName::AggregateDocumentBytesPerSnapshot,
                self.limits.aggregate_document_bytes_per_snapshot,
                total,
            ));
        }
        self.document_bytes = total;
        Ok(())
    }

    /// Charges one parsed document's node work.
    ///
    /// # Errors
    ///
    /// The nesting, per-document node, or per-snapshot node crossing, checked
    /// in that order.
    pub fn charge_work(&mut self, nodes: u64, nesting: u64) -> Result<(), Error> {
        if nesting > self.limits.parser_nesting {
            return Err(crossing(
                ResourceName::ParserNesting,
                self.limits.parser_nesting,
                self.limits.parser_nesting.saturating_add(1),
            ));
        }
        if nodes > self.limits.parser_nodes_per_document {
            return Err(crossing(
                ResourceName::ParserNodesPerDocument,
                self.limits.parser_nodes_per_document,
                self.limits.parser_nodes_per_document.saturating_add(1),
            ));
        }
        self.nodes = self.nodes.saturating_add(nodes);
        if self.nodes > self.limits.parser_nodes_per_snapshot {
            return Err(crossing(
                ResourceName::ParserNodesPerSnapshot,
                self.limits.parser_nodes_per_snapshot,
                self.limits.parser_nodes_per_snapshot.saturating_add(1),
            ));
        }
        Ok(())
    }

    /// Charges one extracted reference whose raw destination is
    /// `destination_bytes` long, as the `document_references`th reference of
    /// its document.
    ///
    /// # Errors
    ///
    /// The destination byte, per-document reference, or per-snapshot
    /// reference crossing, checked in that order.
    pub fn charge_reference(
        &mut self,
        destination_bytes: u64,
        document_references: u64,
    ) -> Result<(), Error> {
        if destination_bytes > self.limits.raw_link_destination_bytes {
            return Err(crossing(
                ResourceName::RawLinkDestinationBytes,
                self.limits.raw_link_destination_bytes,
                destination_bytes,
            ));
        }
        if document_references > self.limits.references_per_document {
            return Err(crossing(
                ResourceName::ReferencesPerDocument,
                self.limits.references_per_document,
                self.limits.references_per_document.saturating_add(1),
            ));
        }
        self.references = self.references.saturating_add(1);
        if self.references > self.limits.references_per_snapshot {
            return Err(crossing(
                ResourceName::ReferencesPerSnapshot,
                self.limits.references_per_snapshot,
                self.limits.references_per_snapshot.saturating_add(1),
            ));
        }
        Ok(())
    }
}
