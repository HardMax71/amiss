use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::{Digest, hb};
use crate::model::{ArtifactId, BranchRef, OwnerId, RepositoryIdentity, TreeIdentity, UtcInstant};

use super::fact::fact_digests;
use super::{
    Fact, WAIVER_BUNDLE_SCHEMA, root, sorted_set, valid_reason, validate_instant, validate_owner,
    validate_repository, validate_tree,
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
#[serde(deny_unknown_fields)]
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaiverBundle {
    pub schema: WaiverBundleSchema,
    pub repository: RepositoryIdentity,
    #[serde(rename = "ref")]
    pub ref_name: BranchRef,
    pub organization_floor_digest: Digest,
    pub created_at: UtcInstant,
    pub items: Vec<WaiverItem>,
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
    root(bytes)?;
    let bundle = de::deserialize_json(bytes)?;
    validate_waiver_bundle(&bundle)?;
    Ok(bundle)
}

/// Produces one valid waiver bundle's canonical bytes and digest.
///
/// # Errors
///
/// A public field violates the same laws [`parse_waiver_bundle`] enforces, or
/// the typed value cannot be serialized.
pub fn canonical_waiver_bundle(bundle: &WaiverBundle) -> Result<(Vec<u8>, Digest), Error> {
    validate_waiver_bundle(bundle)?;
    let bytes = serde_json_canonicalizer::to_vec(bundle)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    let digest = hb(WAIVER_BUNDLE_SCHEMA, &bytes);
    Ok((bytes, digest))
}

fn validate_waiver_bundle(bundle: &WaiverBundle) -> Result<(), Error> {
    validate_repository("$.repository", &bundle.repository)?;
    validate_instant("$.created_at", &bundle.created_at)?;
    if bundle.items.len() > 100_000 {
        return fail("$.items", ErrorKind::LimitExceeded);
    }
    for (index, item) in bundle.items.iter().enumerate() {
        validate_waiver_item(&format!("$.items[{index}]"), item)?;
    }
    sorted_set("$.items", &bundle.items, |left, right| {
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
    for pair in bundle.items.windows(2) {
        if let [left, right] = pair
            && left.candidate_tree == right.candidate_tree
            && left.finding_key == right.finding_key
        {
            return fail("$.items", ErrorKind::DuplicateMember);
        }
    }
    let mut ids = BTreeSet::new();
    for item in &bundle.items {
        if !ids.insert(item.waiver_id.as_str()) {
            return fail("$.items", ErrorKind::DuplicateMember);
        }
        if item.created_at > bundle.created_at {
            return fail("$.items", ErrorKind::Inconsistent);
        }
    }
    Ok(())
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
