use super::item::{FactInput, check_reason};
use super::{Fact, WAIVER_BUNDLE_SCHEMA, check_len, check_schema, root, sorted_set};
use crate::codec;
use crate::de::{Error, ErrorKind, fail};
use crate::digest::{Digest, hj};
use crate::json::Value;
use crate::model::ObjectFormat;
use crate::model::{ArtifactId, BranchRef, OwnerId, RepositoryIdentity, TreeIdentity, UtcInstant};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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
        Self::from_value(&root(bytes)?)
    }

    /// Checks already decoded controls, including the original nested digests.
    ///
    /// # Errors
    ///
    /// A field, digest, ordering, or causal time law is invalid.
    pub fn from_value(value: &Value) -> Result<Self, Error> {
        let payload: Payload = codec::from_value("$", value)?;
        check_schema("$.schema", &payload.schema, WAIVER_BUNDLE_SCHEMA)?;
        check_len("$.items", payload.items.len(), 100_000)?;
        let items: Vec<WaiverItem> = payload
            .items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                let path = format!("$.items[{index}]");
                check_reason(&format!("{path}.reason"), &item.reason)?;
                let fact = item.authorized_fact.check(
                    &path,
                    item.finding_key,
                    item.authorized_fact_digest,
                    "authorized_fact",
                )?;
                if item.created_at > item.not_before || item.not_before >= item.expires_at {
                    return fail(&path, ErrorKind::Inconsistent);
                }
                Ok(WaiverItem {
                    waiver_id: item.waiver_id,
                    finding_key: item.finding_key,
                    authorized_fact: fact,
                    authorized_fact_digest: item.authorized_fact_digest,
                    owner: item.owner,
                    reason: item.reason,
                    created_at: item.created_at,
                    expires_at: item.expires_at,
                    candidate_tree: item.candidate_tree,
                    issuer: item.issuer,
                    not_before: item.not_before,
                })
            })
            .collect::<Result<_, Error>>()?;
        sorted_set("$.items", &items, |a, b| {
            waiver_sort_key(a).cmp(&waiver_sort_key(b))
        })?;
        for pair in items.windows(2) {
            if let [left, right] = pair
                && left.candidate_tree == right.candidate_tree
                && left.finding_key == right.finding_key
            {
                return fail("$.items", ErrorKind::DuplicateMember);
            }
        }
        let mut ids = BTreeSet::new();
        for item in &items {
            if !ids.insert(item.waiver_id.as_str()) {
                return fail("$.items", ErrorKind::DuplicateMember);
            }
            if item.created_at > payload.created_at {
                return fail("$.items", ErrorKind::Inconsistent);
            }
        }
        Ok(Self {
            digest: hj(WAIVER_BUNDLE_SCHEMA, value),
            repository: payload.repository,
            ref_name: payload.ref_name,
            organization_floor_digest: payload.organization_floor_digest,
            created_at: payload.created_at,
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

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    schema: String,
    repository: RepositoryIdentity,
    #[serde(rename = "ref")]
    ref_name: BranchRef,
    organization_floor_digest: Digest,
    created_at: UtcInstant,
    items: Vec<Item>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    waiver_id: ArtifactId,
    finding_key: Digest,
    authorized_fact: FactInput,
    authorized_fact_digest: Digest,
    owner: OwnerId,
    reason: String,
    created_at: UtcInstant,
    expires_at: UtcInstant,
    candidate_tree: TreeIdentity,
    issuer: OwnerId,
    not_before: UtcInstant,
    residual_disposition: ResidualDisposition,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ResidualDisposition {
    Warn,
}
