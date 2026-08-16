use std::collections::BTreeMap;
use std::sync::Arc;

use amiss_wire::controls::ResourceName;
use amiss_wire::model::{Adapter, Oid};

use crate::Error;
use crate::scan::Scanned;

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
    pub aggregate_embedded_code_evaluation_bytes_per_snapshot: u64,
    pub references_per_document: u64,
    pub references_per_snapshot: u64,
    pub declared_labels_per_snapshot: u64,
    pub referenced_target_blob_bytes: u64,
    pub aggregate_referenced_target_bytes_per_snapshot: u64,
    pub ignore_declaration_blob_bytes: u64,
    pub aggregate_ignore_declaration_bytes_per_snapshot: u64,
    pub aggregate_line_fragment_evaluation_bytes_per_snapshot: u64,
    pub aggregate_heading_anchor_evaluation_bytes_per_snapshot: u64,
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
        aggregate_embedded_code_evaluation_bytes_per_snapshot: 536_870_912,
        references_per_document: 16_384,
        references_per_snapshot: 1_000_000,
        declared_labels_per_snapshot: 1_000_000,
        referenced_target_blob_bytes: 16_777_216,
        aggregate_referenced_target_bytes_per_snapshot: 536_870_912,
        ignore_declaration_blob_bytes: 1_048_576,
        aggregate_ignore_declaration_bytes_per_snapshot: 16_777_216,
        aggregate_line_fragment_evaluation_bytes_per_snapshot: 536_870_912,
        aggregate_heading_anchor_evaluation_bytes_per_snapshot: 536_870_912,
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

/// The snapshot aggregates a caller charges by declared bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aggregate {
    SelectedControlBytes,
    ReferencedTargetBytes,
    IgnoreDeclarationBytes,
    LineFragmentBytes,
    HeadingAnchorBytes,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ScanIdentity {
    pub oid: Oid,
    pub adapter: Adapter,
    pub embedded_code_allowance: Option<u64>,
}

/// Snapshot-scoped charge state. Count resources observe exactly one past the
/// limit and stop; per-value byte resources observe the exact declared value;
/// an aggregate observes the prior charged total plus the first crossing
/// member, and a member rejected by its per-value limit is never charged to
/// the aggregate.
#[derive(Debug)]
pub struct ScanResources {
    cache_scope: Arc<()>,
    pub(crate) scans: BTreeMap<ScanIdentity, Arc<Scanned>>,
    limits: ScanLimits,
    documents: u64,
    document_bytes: u64,
    nodes: u64,
    embedded_code_bytes: u64,
    references: u64,
    labels: u64,
    target_bytes: u64,
    ignore_declaration_bytes: u64,
    line_fragment_bytes: u64,
    heading_anchor_bytes: u64,
    control_bytes: u64,
}

impl Clone for ScanResources {
    fn clone(&self) -> Self {
        Self {
            cache_scope: Arc::new(()),
            scans: BTreeMap::new(),
            limits: self.limits,
            documents: self.documents,
            labels: self.labels,
            document_bytes: self.document_bytes,
            nodes: self.nodes,
            embedded_code_bytes: self.embedded_code_bytes,
            ignore_declaration_bytes: self.ignore_declaration_bytes,
            references: self.references,
            target_bytes: self.target_bytes,
            line_fragment_bytes: self.line_fragment_bytes,
            heading_anchor_bytes: self.heading_anchor_bytes,
            control_bytes: self.control_bytes,
        }
    }
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

fn within_limit(
    value: u64,
    limit: u64,
    resource: ResourceName,
    observed_lower_bound: u64,
) -> Result<(), Error> {
    (value <= limit)
        .then_some(())
        .ok_or_else(|| crossing(resource, limit, observed_lower_bound))
}

fn bounded_value(value: u64, limit: u64, resource: ResourceName) -> Result<u64, Error> {
    within_limit(value, limit, resource, value).map(|()| value)
}

impl ScanResources {
    #[must_use]
    pub fn new(limits: ScanLimits) -> Self {
        Self {
            cache_scope: Arc::new(()),
            scans: BTreeMap::new(),
            limits,
            documents: 0,
            document_bytes: 0,
            nodes: 0,
            embedded_code_bytes: 0,
            references: 0,
            labels: 0,
            target_bytes: 0,
            ignore_declaration_bytes: 0,
            line_fragment_bytes: 0,
            heading_anchor_bytes: 0,
            control_bytes: 0,
        }
    }

    /// One aggregate charge: the prior total plus this member, refused whole
    /// when the sum crosses, so a rejected member is never counted.
    fn charge_aggregate(
        total: &mut u64,
        limit: u64,
        resource: ResourceName,
        declared_bytes: u64,
    ) -> Result<(), Error> {
        let charged = total.saturating_add(declared_bytes);
        within_limit(charged, limit, resource, charged)?;
        *total = charged;
        Ok(())
    }

    fn charge_count(
        total: &mut u64,
        increment: u64,
        limit: u64,
        resource: ResourceName,
    ) -> Result<(), Error> {
        *total = total.saturating_add(increment);
        within_limit(*total, limit, resource, limit.saturating_add(1))
    }

    #[must_use]
    pub const fn limits(&self) -> &ScanLimits {
        &self.limits
    }

    pub(crate) const fn cache_scope(&self) -> &Arc<()> {
        &self.cache_scope
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
    pub const fn embedded_code_bytes(&self) -> u64 {
        self.embedded_code_bytes
    }

    /// The embedded-code evaluation bytes still grantable to the next parse:
    /// the aggregate ceiling minus what earlier parses charged.
    #[must_use]
    pub const fn embedded_code_allowance(&self) -> u64 {
        self.limits
            .aggregate_embedded_code_evaluation_bytes_per_snapshot
            .saturating_sub(self.embedded_code_bytes)
    }

    /// Accumulates one completed parse's embedded-code bytes. Infallible: a
    /// parse granted the remaining allowance can only spend inside it, so the
    /// total never crosses the ceiling here. Crate-private because that
    /// invariant is the parse path's, not a caller contract.
    pub(crate) fn charge_embedded_code(&mut self, spent: u64) {
        self.embedded_code_bytes = self.embedded_code_bytes.saturating_add(spent);
    }

    /// The aggregate crossing for a parse the in-parse meter ended, observing
    /// the prior charged total plus the ended parse's spent bytes.
    pub(crate) const fn embedded_code_crossing(&self, spent: u64) -> Error {
        crossing(
            ResourceName::AggregateEmbeddedCodeEvaluationBytesPerSnapshot,
            self.limits
                .aggregate_embedded_code_evaluation_bytes_per_snapshot,
            self.embedded_code_bytes.saturating_add(spent),
        )
    }

    #[must_use]
    pub const fn references(&self) -> u64 {
        self.references
    }

    #[must_use]
    pub const fn target_bytes(&self) -> u64 {
        self.target_bytes
    }

    #[must_use]
    pub const fn line_fragment_bytes(&self) -> u64 {
        self.line_fragment_bytes
    }

    #[must_use]
    pub const fn heading_anchor_bytes(&self) -> u64 {
        self.heading_anchor_bytes
    }

    /// The heading-anchor evaluation bytes still grantable to the next target
    /// parse.
    #[must_use]
    pub const fn heading_anchor_allowance(&self) -> u64 {
        self.limits
            .aggregate_heading_anchor_evaluation_bytes_per_snapshot
            .saturating_sub(self.heading_anchor_bytes)
    }

    /// Charges one member to a snapshot aggregate. The per-value cap, where a
    /// resource has one, is enforced where the read happens.
    ///
    /// # Errors
    ///
    /// The aggregate crossing, observing the prior total plus this member.
    pub fn charge(&mut self, aggregate: Aggregate, declared_bytes: u64) -> Result<(), Error> {
        let limits = self.limits;
        let (total, limit, resource) = match aggregate {
            Aggregate::SelectedControlBytes => (
                &mut self.control_bytes,
                limits.aggregate_selected_control_bytes_per_snapshot,
                ResourceName::AggregateSelectedControlBytesPerSnapshot,
            ),
            Aggregate::ReferencedTargetBytes => (
                &mut self.target_bytes,
                limits.aggregate_referenced_target_bytes_per_snapshot,
                ResourceName::AggregateReferencedTargetBytesPerSnapshot,
            ),
            Aggregate::IgnoreDeclarationBytes => (
                &mut self.ignore_declaration_bytes,
                limits.aggregate_ignore_declaration_bytes_per_snapshot,
                ResourceName::AggregateIgnoreDeclarationBytesPerSnapshot,
            ),
            Aggregate::LineFragmentBytes => (
                &mut self.line_fragment_bytes,
                limits.aggregate_line_fragment_evaluation_bytes_per_snapshot,
                ResourceName::AggregateLineFragmentEvaluationBytesPerSnapshot,
            ),
            Aggregate::HeadingAnchorBytes => (
                &mut self.heading_anchor_bytes,
                limits.aggregate_heading_anchor_evaluation_bytes_per_snapshot,
                ResourceName::AggregateHeadingAnchorEvaluationBytesPerSnapshot,
            ),
        };
        Self::charge_aggregate(total, limit, resource, declared_bytes)
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
        Self::charge_count(
            &mut self.documents,
            1,
            self.limits.documents_per_snapshot,
            ResourceName::DocumentsPerSnapshot,
        )
    }

    /// Charges one admitted document's declared byte size.
    ///
    /// # Errors
    ///
    /// The per-document byte crossing, then the aggregate crossing; a member
    /// rejected by the first is never charged to the second.
    pub fn charge_document_bytes(&mut self, declared_bytes: u64) -> Result<(), Error> {
        bounded_value(
            declared_bytes,
            self.limits.document_blob_bytes,
            ResourceName::DocumentBlobBytes,
        )
        .and_then(|bytes| {
            Self::charge_aggregate(
                &mut self.document_bytes,
                self.limits.aggregate_document_bytes_per_snapshot,
                ResourceName::AggregateDocumentBytesPerSnapshot,
                bytes,
            )
        })
    }

    /// Charges one parsed document's node work.
    ///
    /// # Errors
    ///
    /// The nesting, per-document node, or per-snapshot node crossing, checked
    /// in that order.
    pub fn charge_work(&mut self, nodes: u64, nesting: u64) -> Result<(), Error> {
        within_limit(
            nesting,
            self.limits.parser_nesting,
            ResourceName::ParserNesting,
            self.limits.parser_nesting.saturating_add(1),
        )?;
        within_limit(
            nodes,
            self.limits.parser_nodes_per_document,
            ResourceName::ParserNodesPerDocument,
            self.limits.parser_nodes_per_document.saturating_add(1),
        )?;
        Self::charge_count(
            &mut self.nodes,
            nodes,
            self.limits.parser_nodes_per_snapshot,
            ResourceName::ParserNodesPerSnapshot,
        )
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
        within_limit(
            destination_bytes,
            self.limits.raw_link_destination_bytes,
            ResourceName::RawLinkDestinationBytes,
            destination_bytes,
        )?;
        within_limit(
            document_references,
            self.limits.references_per_document,
            ResourceName::ReferencesPerDocument,
            self.limits.references_per_document.saturating_add(1),
        )?;
        Self::charge_count(
            &mut self.references,
            1,
            self.limits.references_per_snapshot,
            ResourceName::ReferencesPerSnapshot,
        )
    }

    /// One declared label admitted to the snapshot's table.
    ///
    /// # Errors
    ///
    /// The `declared-labels-per-snapshot` crossing.
    pub(crate) fn charge_label(&mut self) -> Result<(), Error> {
        Self::charge_count(
            &mut self.labels,
            1,
            self.limits.declared_labels_per_snapshot,
            ResourceName::DeclaredLabelsPerSnapshot,
        )
    }
}
