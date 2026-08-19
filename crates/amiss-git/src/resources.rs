use std::collections::BTreeSet;
use std::fmt;

use amiss_wire::controls::ResourceName;

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitLimits {
    pub inflated_object_bytes: u64,
    pub compressed_stream_bytes: u64,
    pub aggregate_compressed_bytes: u64,
    pub pack_directory_entries: u64,
    pub pack_files: u64,
    pub pack_index_bytes: u64,
    pub aggregate_pack_index_bytes: u64,
    pub delta_depth: u64,
    pub index_bytes: u64,
    pub tree_entries_per_snapshot: u64,
    pub raw_path_bytes: u64,
}

/// A smaller contextual inflated cap (a document, target, or control blob)
/// that applies before the general Git object cap when the object header
/// declares a larger value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValueCap {
    pub resource: ResourceName,
    pub limit: u64,
}

impl GitLimits {
    pub const CONTRACT: Self = Self {
        inflated_object_bytes: 134_217_728,
        compressed_stream_bytes: 268_435_456,
        aggregate_compressed_bytes: 2_147_483_648,
        pack_directory_entries: 8_192,
        pack_files: 4_096,
        pack_index_bytes: 536_870_912,
        aggregate_pack_index_bytes: 1_073_741_824,
        delta_depth: 128,
        index_bytes: 268_435_456,
        tree_entries_per_snapshot: 1_000_000,
        raw_path_bytes: 4_096,
    };
}

impl Default for GitLimits {
    fn default() -> Self {
        Self::CONTRACT
    }
}

pub(crate) fn crossing(resource: ResourceName, configured_limit: u64, observed: u64) -> Error {
    Error::ResourceLimit {
        resource,
        configured_limit,
        observed_lower_bound: observed,
    }
}

/// Resource accounting and reusable decode state for one evaluation side.
/// Compressed objects and pack indexes have independent identity sets, and
/// cache hits within either never recharge.
pub struct GitResources {
    limits: GitLimits,
    compressed: ByteMeter,
    pack_indexes: ByteMeter,
    pub(crate) loose_inflater: Option<flate2::Decompress>,
}

impl Clone for GitResources {
    fn clone(&self) -> Self {
        Self {
            limits: self.limits,
            compressed: self.compressed.clone(),
            pack_indexes: self.pack_indexes.clone(),
            loose_inflater: None,
        }
    }
}

impl fmt::Debug for GitResources {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitResources")
            .field("limits", &self.limits)
            .field("compressed", &self.compressed)
            .field("pack_indexes", &self.pack_indexes)
            .finish_non_exhaustive()
    }
}

impl PartialEq for GitResources {
    fn eq(&self, other: &Self) -> bool {
        self.limits == other.limits
            && self.compressed == other.compressed
            && self.pack_indexes == other.pack_indexes
    }
}

impl Eq for GitResources {}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ByteMeter {
    total: u64,
    charged: BTreeSet<Box<str>>,
}

impl GitResources {
    #[must_use]
    pub fn new(limits: GitLimits) -> Self {
        Self {
            limits,
            compressed: ByteMeter::default(),
            pack_indexes: ByteMeter::default(),
            loose_inflater: None,
        }
    }

    #[must_use]
    pub const fn limits(&self) -> GitLimits {
        self.limits
    }

    /// # Errors
    ///
    /// Fails when the stream crosses the per-stream cap or the running total
    /// crosses the aggregate cap.
    pub fn charge_compressed(&mut self, member: &str, bytes: u64) -> Result<(), Error> {
        charge(
            &mut self.compressed,
            member,
            bytes,
            ResourceName::GitCompressedObjectBytes,
            self.limits.compressed_stream_bytes,
            ResourceName::AggregateGitCompressedObjectBytesPerEvaluation,
            self.limits.aggregate_compressed_bytes,
        )
    }

    /// # Errors
    ///
    /// Fails when one index crosses the per-index cap or the running total
    /// crosses the aggregate cap.
    pub fn charge_index(&mut self, member: &str, bytes: u64) -> Result<(), Error> {
        charge(
            &mut self.pack_indexes,
            member,
            bytes,
            ResourceName::GitPackIndexBytes,
            self.limits.pack_index_bytes,
            ResourceName::AggregateGitPackIndexBytes,
            self.limits.aggregate_pack_index_bytes,
        )
    }
}

fn charge(
    meter: &mut ByteMeter,
    member: &str,
    bytes: u64,
    member_resource: ResourceName,
    member_limit: u64,
    aggregate_resource: ResourceName,
    aggregate_limit: u64,
) -> Result<(), Error> {
    if bytes > member_limit {
        return Err(crossing(member_resource, member_limit, bytes));
    }
    if meter.charged.contains(member) {
        return Ok(());
    }
    let total = meter.total.saturating_add(bytes);
    if total > aggregate_limit {
        return Err(crossing(aggregate_resource, aggregate_limit, total));
    }
    meter.total = total;
    meter.charged.insert(member.into());
    Ok(())
}
