use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::de::{self, Error, ErrorKind, fail};
use crate::model::Digest;
use crate::model::{ArtifactId, BranchRef, OwnerId, RepositoryIdentity, TreeIdentity, UtcInstant};

use super::fact::fact_digests;
use super::{
    Fact, sorted_set, valid_reason, validate_instant, validate_owner, validate_repository,
    validate_tree,
};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum DebtSnapshotSchema {
    #[strum(serialize = "amiss/debt-snapshot")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
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

impl Serialize for DebtItem {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for DebtItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct DebtSnapshot {
    pub schema: DebtSnapshotSchema,
    pub repository: RepositoryIdentity,
    #[serde(rename = "ref")]
    pub ref_name: BranchRef,
    pub organization_floor_digest: Digest,
    pub adoption_tree: TreeIdentity,
    pub adoption_report_payload_digest: Digest,
    pub created_at: UtcInstant,
    pub items: Vec<DebtItem>,
}

impl Serialize for DebtSnapshot {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for DebtSnapshot {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

/// Parses and validates one adoption-debt snapshot.
///
/// # Errors
///
/// Fails on strict-JSON defects, schema-shape violations, embedded key or
/// fact digests that do not recompute, fact-kind/resolution inconsistencies,
/// causal time-order violations, and unsorted or duplicate items or keys.
pub fn parse_debt_snapshot(bytes: &[u8]) -> Result<DebtSnapshot, Error> {
    de::JsonProfile::validate(bytes)?;
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    deserializer.disable_recursion_limit();
    let snapshot: DebtSnapshot = serde_path_to_error::deserialize(&mut deserializer)
        .map_err(|defect| de::deserialize_error("$", &defect))?;
    deserializer
        .end()
        .map_err(|defect| Error::new("$", ErrorKind::Json(defect.to_string())))?;
    snapshot.validate()?;
    Ok(snapshot)
}

impl DebtSnapshot {
    /// Checks this control's domain rules and resource limits.
    ///
    /// # Errors
    ///
    /// A public field violates the contract enforced by [`parse_debt_snapshot`].
    pub fn validate(&self) -> Result<(), Error> {
        validate_repository("$.repository", &self.repository)?;
        validate_tree("$.adoption_tree", &self.adoption_tree)?;
        validate_instant("$.created_at", &self.created_at)?;
        if self.items.len() > 100_000 {
            return fail("$.items", ErrorKind::LimitExceeded);
        }
        for (index, item) in self.items.iter().enumerate() {
            validate_debt_item(&format!("$.items[{index}]"), item)?;
        }
        sorted_set("$.items", &self.items, |left, right| {
            left.debt_id.as_str().cmp(right.debt_id.as_str())
        })?;
        let mut keys = BTreeSet::new();
        for item in &self.items {
            if !keys.insert(item.finding_key) {
                return fail("$.items", ErrorKind::DuplicateMember);
            }
            if item.created_at > self.created_at {
                return fail("$.items", ErrorKind::Inconsistent);
            }
        }
        Ok(())
    }
}

fn validate_debt_item(path: &str, item: &DebtItem) -> Result<(), Error> {
    let (finding_key, fact_digest) =
        fact_digests(&format!("{path}.accepted_fact"), &item.accepted_fact)?;
    if item.finding_key != finding_key {
        return fail(&format!("{path}.finding_key"), ErrorKind::DigestMismatch);
    }
    if item.accepted_fact_digest != fact_digest {
        return fail(
            &format!("{path}.accepted_fact_digest"),
            ErrorKind::DigestMismatch,
        );
    }
    validate_owner(&format!("{path}.owner"), &item.owner)?;
    if !valid_reason(&item.reason) {
        return fail(&format!("{path}.reason"), ErrorKind::InvalidValue);
    }
    validate_instant(&format!("{path}.created_at"), &item.created_at)?;
    validate_instant(&format!("{path}.expires_at"), &item.expires_at)?;
    (item.created_at < item.expires_at)
        .then_some(())
        .ok_or_else(|| Error::new(path, ErrorKind::Inconsistent))
}
