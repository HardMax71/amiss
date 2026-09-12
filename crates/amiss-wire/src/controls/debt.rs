use super::item::{FactInput, check_reason};
use super::{DEBT_SNAPSHOT_SCHEMA, Fact, check_len, check_schema, root, sorted_set};
use crate::codec;
use crate::de::{Error, ErrorKind, fail};
use crate::digest::{Digest, hj};
use crate::json::Value;
use crate::model::{ArtifactId, BranchRef, OwnerId, RepositoryIdentity, TreeIdentity, UtcInstant};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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
        Self::from_value(&root(bytes)?)
    }

    /// Checks already decoded controls, including the original nested digests.
    ///
    /// # Errors
    ///
    /// A field, digest, ordering, or causal time law is invalid.
    pub fn from_value(value: &Value) -> Result<Self, Error> {
        let payload: Payload = codec::from_value("$", value)?;
        check_schema("$.schema", &payload.schema, DEBT_SNAPSHOT_SCHEMA)?;
        check_len("$.items", payload.items.len(), 100_000)?;
        let items: Vec<DebtItem> = payload
            .items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                let path = format!("$.items[{index}]");
                check_reason(&format!("{path}.reason"), &item.reason)?;
                let fact = item.accepted_fact.check(
                    &path,
                    item.finding_key,
                    item.accepted_fact_digest,
                    "accepted_fact",
                )?;
                if item.created_at >= item.expires_at {
                    return fail(&path, ErrorKind::Inconsistent);
                }
                Ok(DebtItem {
                    debt_id: item.debt_id,
                    finding_key: item.finding_key,
                    accepted_fact: fact,
                    accepted_fact_digest: item.accepted_fact_digest,
                    owner: item.owner,
                    reason: item.reason,
                    created_at: item.created_at,
                    expires_at: item.expires_at,
                })
            })
            .collect::<Result<_, Error>>()?;
        sorted_set("$.items", &items, |a, b| {
            a.debt_id.as_str().cmp(b.debt_id.as_str())
        })?;
        let mut keys = BTreeSet::new();
        for item in &items {
            if !keys.insert(item.finding_key) {
                return fail("$.items", ErrorKind::DuplicateMember);
            }
            if item.created_at > payload.created_at {
                return fail("$.items", ErrorKind::Inconsistent);
            }
        }
        Ok(Self {
            digest: hj(DEBT_SNAPSHOT_SCHEMA, value),
            repository: payload.repository,
            ref_name: payload.ref_name,
            organization_floor_digest: payload.organization_floor_digest,
            created_at: payload.created_at,
            items,
            adoption_tree: payload.adoption_tree,
            adoption_report_payload_digest: payload.adoption_report_payload_digest,
        })
    }
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
    adoption_tree: TreeIdentity,
    adoption_report_payload_digest: Digest,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    debt_id: ArtifactId,
    finding_key: Digest,
    accepted_fact: FactInput,
    accepted_fact_digest: Digest,
    owner: OwnerId,
    reason: String,
    created_at: UtcInstant,
    expires_at: UtcInstant,
}
