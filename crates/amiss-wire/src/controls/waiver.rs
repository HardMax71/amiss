use std::collections::BTreeSet;

use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::model::{
    ArtifactId, BranchRef, ObjectFormat, OwnerId, RepositoryIdentity, TreeIdentity, UtcInstant,
};

use super::{
    Fact, WAIVER_BUNDLE_SCHEMA, decode_branch_ref, decode_digest, decode_instant, decode_items,
    decode_repository, decode_waiver_item, root, sorted_set,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaiverItem {
    pub waiver_id: ArtifactId,
    pub finding_key: Digest,
    pub authorized_fact: Fact,
    pub authorized_fact_digest: Digest,
    pub candidate_tree: TreeIdentity,
    pub owner: OwnerId,
    pub issuer: OwnerId,
    pub reason: String,
    pub created_at: UtcInstant,
    pub not_before: UtcInstant,
    pub expires_at: UtcInstant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaiverBundle {
    digest: Digest,
    repository: RepositoryIdentity,
    ref_name: BranchRef,
    organization_floor_digest: Digest,
    created_at: UtcInstant,
    items: Vec<WaiverItem>,
}

impl WaiverBundle {
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
    pub fn created_at(&self) -> &UtcInstant {
        &self.created_at
    }

    #[must_use]
    pub fn items(&self) -> &[WaiverItem] {
        &self.items
    }

    #[must_use]
    pub const fn schema(&self) -> &'static str {
        WAIVER_BUNDLE_SCHEMA
    }

    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, embedded key or
    /// fact digests that do not recompute, fact-kind/resolution inconsistencies,
    /// causal time-order violations, duplicate waiver IDs, and duplicate
    /// `(candidate_tree, finding_key)` pairs.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let value = root(bytes)?;
        let digest = hj(WAIVER_BUNDLE_SCHEMA, &value);
        let mut obj = Obj::new("$", value)?;
        obj.required("schema", |path, value| {
            de::const_str(path, value, WAIVER_BUNDLE_SCHEMA)
        })?;

        let repository = obj.required("repository", decode_repository)?;
        let ref_name = obj.required("ref", decode_branch_ref)?;
        let organization_floor_digest = obj.required("organization_floor_digest", decode_digest)?;
        let created_at = obj.required("created_at", decode_instant)?;

        let items_path = obj.field("items");
        let raw = de::array(&items_path, obj.take("items")?)?;
        let items = decode_items(&items_path, raw, 100_000, decode_waiver_item)?;
        sorted_set(&items_path, &items, |a, b| {
            waiver_sort_key(a).cmp(&waiver_sort_key(b))
        })?;
        for pair in items.windows(2) {
            if let [left, right] = pair
                && left.candidate_tree == right.candidate_tree
                && left.finding_key == right.finding_key
            {
                return fail(&items_path, ErrorKind::DuplicateMember);
            }
        }
        let mut ids: BTreeSet<&str> = BTreeSet::new();
        for item in &items {
            if !ids.insert(item.waiver_id.as_str()) {
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
            created_at,
            items,
        })
    }
}

fn waiver_sort_key(item: &WaiverItem) -> (ObjectFormat, &str, Digest, &str) {
    (
        item.candidate_tree.object_format(),
        item.candidate_tree.tree_oid(),
        item.finding_key,
        item.waiver_id.as_str(),
    )
}
