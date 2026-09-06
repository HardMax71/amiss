use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::{Digest, hb};
use crate::model::{ArtifactId, BranchRef, OwnerId, RepositoryIdentity, TreeIdentity, UtcInstant};

use super::fact::fact_digests;
use super::{
    DEBT_SNAPSHOT_SCHEMA, Fact, root, sorted_set, valid_reason, validate_instant, validate_owner,
    validate_repository, validate_tree,
};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum DebtSnapshotSchema {
    #[strum(serialize = "amiss/debt-snapshot")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

/// Parses and validates one adoption-debt snapshot.
///
/// # Errors
///
/// Fails on strict-JSON defects, schema-shape violations, embedded key or
/// fact digests that do not recompute, fact-kind/resolution inconsistencies,
/// causal time-order violations, and unsorted or duplicate items or keys.
pub fn parse_debt_snapshot(bytes: &[u8]) -> Result<DebtSnapshot, Error> {
    root(bytes)?;
    let snapshot = de::deserialize_json(bytes)?;
    validate_debt_snapshot(&snapshot)?;
    Ok(snapshot)
}

/// Produces one valid debt snapshot's canonical bytes and digest.
///
/// # Errors
///
/// A public field violates the same laws [`parse_debt_snapshot`] enforces, or
/// the typed value cannot be serialized.
pub fn canonical_debt_snapshot(snapshot: &DebtSnapshot) -> Result<(Vec<u8>, Digest), Error> {
    validate_debt_snapshot(snapshot)?;
    let bytes = serde_json_canonicalizer::to_vec(snapshot)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    let digest = hb(DEBT_SNAPSHOT_SCHEMA, &bytes);
    Ok((bytes, digest))
}

fn validate_debt_snapshot(snapshot: &DebtSnapshot) -> Result<(), Error> {
    validate_repository("$.repository", &snapshot.repository)?;
    validate_tree("$.adoption_tree", &snapshot.adoption_tree)?;
    validate_instant("$.created_at", &snapshot.created_at)?;
    if snapshot.items.len() > 100_000 {
        return fail("$.items", ErrorKind::LimitExceeded);
    }
    for (index, item) in snapshot.items.iter().enumerate() {
        validate_debt_item(&format!("$.items[{index}]"), item)?;
    }
    sorted_set("$.items", &snapshot.items, |left, right| {
        left.debt_id.as_str().cmp(right.debt_id.as_str())
    })?;
    let mut keys = BTreeSet::new();
    for item in &snapshot.items {
        if !keys.insert(item.finding_key) {
            return fail("$.items", ErrorKind::DuplicateMember);
        }
        if item.created_at > snapshot.created_at {
            return fail("$.items", ErrorKind::Inconsistent);
        }
    }
    Ok(())
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
