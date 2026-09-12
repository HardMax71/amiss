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
pub enum WaiverBundleSchema {
    #[strum(serialize = "amiss/waiver-bundle")]
    Current,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum WaiverResidualDisposition {
    #[strum(serialize = "warn")]
    Warn,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
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
    pub residual_disposition: WaiverResidualDisposition,
}

impl Serialize for WaiverItem {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for WaiverItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
pub struct WaiverBundle {
    pub schema: WaiverBundleSchema,
    pub repository: RepositoryIdentity,
    #[serde(rename = "ref")]
    pub ref_name: BranchRef,
    pub organization_floor_digest: Digest,
    pub created_at: UtcInstant,
    pub items: Vec<WaiverItem>,
}

impl Serialize for WaiverBundle {
    fn serialize<Ser: serde::Serializer>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for WaiverBundle {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(serde_with::with_prefix::WithPrefix {
            delegate: deserializer,
            prefix: "",
        })
    }
}

/// Parses and validates one waiver bundle.
///
/// # Errors
///
/// Fails on strict-JSON defects, schema-shape violations, embedded key or
/// fact digests that do not recompute, fact-kind/resolution inconsistencies,
/// causal time-order violations, duplicate waiver IDs, and duplicate
/// `(candidate_tree, finding_key)` pairs.
pub fn parse_waiver_bundle(bytes: &[u8]) -> Result<WaiverBundle, Error> {
    de::JsonProfile::validate(bytes)?;
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    deserializer.disable_recursion_limit();
    let bundle: WaiverBundle = serde_path_to_error::deserialize(&mut deserializer)
        .map_err(|defect| de::deserialize_error("$", &defect))?;
    deserializer
        .end()
        .map_err(|defect| Error::new("$", ErrorKind::Json(defect.to_string())))?;
    bundle.validate()?;
    Ok(bundle)
}

impl WaiverBundle {
    /// Checks this control's domain rules and resource limits.
    ///
    /// # Errors
    ///
    /// A public field violates the contract enforced by [`parse_waiver_bundle`].
    pub fn validate(&self) -> Result<(), Error> {
        validate_repository("$.repository", &self.repository)?;
        validate_instant("$.created_at", &self.created_at)?;
        if self.items.len() > 100_000 {
            return fail("$.items", ErrorKind::LimitExceeded);
        }
        for (index, item) in self.items.iter().enumerate() {
            validate_waiver_item(&format!("$.items[{index}]"), item)?;
        }
        sorted_set("$.items", &self.items, |left, right| {
            (
                left.candidate_tree.object_format,
                left.candidate_tree.tree_oid.as_str(),
                left.finding_key,
                left.waiver_id.as_str(),
            )
                .cmp(&(
                    right.candidate_tree.object_format,
                    right.candidate_tree.tree_oid.as_str(),
                    right.finding_key,
                    right.waiver_id.as_str(),
                ))
        })?;
        for pair in self.items.windows(2) {
            if let [left, right] = pair
                && left.candidate_tree == right.candidate_tree
                && left.finding_key == right.finding_key
            {
                return fail("$.items", ErrorKind::DuplicateMember);
            }
        }
        let mut ids = BTreeSet::new();
        for item in &self.items {
            if !ids.insert(item.waiver_id.as_str()) {
                return fail("$.items", ErrorKind::DuplicateMember);
            }
            if item.created_at > self.created_at {
                return fail("$.items", ErrorKind::Inconsistent);
            }
        }
        Ok(())
    }
}

fn validate_waiver_item(path: &str, item: &WaiverItem) -> Result<(), Error> {
    let (finding_key, fact_digest) =
        fact_digests(&format!("{path}.authorized_fact"), &item.authorized_fact)?;
    if item.finding_key != finding_key {
        return fail(&format!("{path}.finding_key"), ErrorKind::DigestMismatch);
    }
    if item.authorized_fact_digest != fact_digest {
        return fail(
            &format!("{path}.authorized_fact_digest"),
            ErrorKind::DigestMismatch,
        );
    }
    validate_tree(&format!("{path}.candidate_tree"), &item.candidate_tree)?;
    validate_owner(&format!("{path}.owner"), &item.owner)?;
    validate_owner(&format!("{path}.issuer"), &item.issuer)?;
    if !valid_reason(&item.reason) {
        return fail(&format!("{path}.reason"), ErrorKind::InvalidValue);
    }
    validate_instant(&format!("{path}.created_at"), &item.created_at)?;
    validate_instant(&format!("{path}.not_before"), &item.not_before)?;
    validate_instant(&format!("{path}.expires_at"), &item.expires_at)?;
    (item.created_at <= item.not_before && item.not_before < item.expires_at)
        .then_some(())
        .ok_or_else(|| Error::new(path, ErrorKind::Inconsistent))
}
