use std::collections::BTreeSet;

use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::model::{ArtifactId, BranchRef, OwnerId, RepositoryIdentity, TreeIdentity, UtcInstant};

use super::{
    DEBT_SNAPSHOT_SCHEMA, Fact, decode_branch_ref, decode_debt_item, decode_digest, decode_instant,
    decode_items, decode_repository, decode_tree, root, sorted_set,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebtItem {
    pub debt_id: ArtifactId,
    pub finding_key: Digest,
    pub accepted_fact: Fact,
    pub accepted_fact_digest: Digest,
    pub owner: OwnerId,
    pub reason: String,
    pub created_at: UtcInstant,
    pub expires_at: UtcInstant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebtSnapshot {
    digest: Digest,
    repository: RepositoryIdentity,
    ref_name: BranchRef,
    organization_floor_digest: Digest,
    adoption_tree: TreeIdentity,
    adoption_report_payload_digest: Digest,
    created_at: UtcInstant,
    items: Vec<DebtItem>,
}

impl DebtSnapshot {
    #[must_use]
    pub const fn digest(&self) -> Digest {
        self.digest
    }

    #[must_use]
    pub fn repository(&self) -> &RepositoryIdentity {
        &self.repository
    }

    #[must_use]
    pub fn ref_name(&self) -> &BranchRef {
        &self.ref_name
    }

    #[must_use]
    pub const fn organization_floor_digest(&self) -> Digest {
        self.organization_floor_digest
    }

    #[must_use]
    pub fn adoption_tree(&self) -> &TreeIdentity {
        &self.adoption_tree
    }

    #[must_use]
    pub const fn adoption_report_payload_digest(&self) -> Digest {
        self.adoption_report_payload_digest
    }

    #[must_use]
    pub fn created_at(&self) -> &UtcInstant {
        &self.created_at
    }

    #[must_use]
    pub fn items(&self) -> &[DebtItem] {
        &self.items
    }

    #[must_use]
    pub const fn schema(&self) -> &'static str {
        DEBT_SNAPSHOT_SCHEMA
    }

    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, embedded key or
    /// fact digests that do not recompute, fact-kind/resolution inconsistencies,
    /// causal time-order violations, and unsorted or duplicate items or keys.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let value = root(bytes)?;
        let digest = hj(DEBT_SNAPSHOT_SCHEMA, &value);
        let mut obj = Obj::new("$", value)?;
        obj.required("schema", |path, value| {
            de::const_str(path, value, DEBT_SNAPSHOT_SCHEMA)
        })?;

        let repository = obj.required("repository", decode_repository)?;
        let ref_name = obj.required("ref", decode_branch_ref)?;
        let organization_floor_digest = obj.required("organization_floor_digest", decode_digest)?;
        let adoption_tree = obj.required("adoption_tree", decode_tree)?;
        let adoption_report_payload_digest =
            obj.required("adoption_report_payload_digest", decode_digest)?;
        let created_at = obj.required("created_at", decode_instant)?;

        let items_path = obj.field("items");
        let raw = de::array(&items_path, obj.take("items")?)?;
        let items = decode_items(&items_path, raw, 100_000, decode_debt_item)?;
        sorted_set(&items_path, &items, |a, b| {
            a.debt_id.as_str().cmp(b.debt_id.as_str())
        })?;
        let mut keys: BTreeSet<Digest> = BTreeSet::new();
        for item in &items {
            if !keys.insert(item.finding_key) {
                return fail(&items_path, ErrorKind::DuplicateMember);
            }
            if item.created_at > created_at {
                return fail(&items_path, ErrorKind::Inconsistent);
            }
        }

        obj.finish()?;
        Ok(Self {
            digest,
            repository,
            ref_name,
            organization_floor_digest,
            adoption_tree,
            adoption_report_payload_digest,
            created_at,
            items,
        })
    }
}
