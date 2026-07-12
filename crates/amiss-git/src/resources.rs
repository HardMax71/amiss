use std::collections::BTreeSet;

use amiss_wire::controls::ResourceName;

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitLimits {
    pub inflated_object_bytes: u64,
    pub compressed_stream_bytes: u64,
    pub aggregate_compressed_bytes: u64,
}

impl GitLimits {
    pub const CONTRACT: Self = Self {
        inflated_object_bytes: 134_217_728,
        compressed_stream_bytes: 268_435_456,
        aggregate_compressed_bytes: 2_147_483_648,
    };
}

impl Default for GitLimits {
    fn default() -> Self {
        Self::CONTRACT
    }
}

/// Compressed-byte charging for one evaluation side: each member is charged
/// once per selected OID, so cache hits never recharge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitResources {
    limits: GitLimits,
    aggregate_compressed: u64,
    charged: BTreeSet<String>,
}

impl GitResources {
    #[must_use]
    pub fn new(limits: GitLimits) -> Self {
        Self {
            limits,
            aggregate_compressed: 0,
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
            return Err(Error::ResourceLimit {
                resource: ResourceName::GitCompressedObjectBytes,
                configured_limit: self.limits.compressed_stream_bytes,
                observed_lower_bound: bytes,
            });
        }
        if self.charged.contains(member) {
            return Ok(());
        }
        let total = self.aggregate_compressed.saturating_add(bytes);
        if total > self.limits.aggregate_compressed_bytes {
            return Err(Error::ResourceLimit {
                resource: ResourceName::AggregateGitCompressedObjectBytesPerEvaluation,
                configured_limit: self.limits.aggregate_compressed_bytes,
                observed_lower_bound: total,
            });
        }
        self.aggregate_compressed = total;
        self.charged.insert(member.to_owned());
        Ok(())
    }
}
