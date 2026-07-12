use std::collections::BTreeSet;

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

/// Byte charging for one evaluation side: each member is charged once per
/// selected member key, so cache hits never recharge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitResources {
    limits: GitLimits,
    aggregate_compressed: u64,
    aggregate_index: u64,
    charged: BTreeSet<String>,
}

impl GitResources {
    #[must_use]
    pub fn new(limits: GitLimits) -> Self {
        Self {
            limits,
            aggregate_compressed: 0,
            aggregate_index: 0,
            charged: BTreeSet::new(),
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
        if bytes > self.limits.compressed_stream_bytes {
            return Err(crossing(
                ResourceName::GitCompressedObjectBytes,
                self.limits.compressed_stream_bytes,
                bytes,
            ));
        }
        if self.charged.contains(member) {
            return Ok(());
        }
        let total = self.aggregate_compressed.saturating_add(bytes);
        if total > self.limits.aggregate_compressed_bytes {
            return Err(crossing(
                ResourceName::AggregateGitCompressedObjectBytesPerEvaluation,
                self.limits.aggregate_compressed_bytes,
                total,
            ));
        }
        self.aggregate_compressed = total;
        self.charged.insert(member.to_owned());
        Ok(())
    }

    /// # Errors
    ///
    /// Fails when one index crosses the per-index cap or the running total
    /// crosses the aggregate cap.
    pub fn charge_index(&mut self, member: &str, bytes: u64) -> Result<(), Error> {
        if bytes > self.limits.pack_index_bytes {
            return Err(crossing(
                ResourceName::GitPackIndexBytes,
                self.limits.pack_index_bytes,
                bytes,
            ));
        }
        let key = format!("idx:{member}");
        if self.charged.contains(&key) {
            return Ok(());
        }
        let total = self.aggregate_index.saturating_add(bytes);
        if total > self.limits.aggregate_pack_index_bytes {
            return Err(crossing(
                ResourceName::AggregateGitPackIndexBytes,
                self.limits.aggregate_pack_index_bytes,
                total,
            ));
        }
        self.aggregate_index = total;
        self.charged.insert(key);
        Ok(())
    }
}
