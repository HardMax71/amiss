use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::de::{self, Error, ErrorKind, fail};
use crate::digest::{Digest, hb};
use crate::model::{ArtifactId, BranchRef, OwnerId, RepoPathText, RepositoryIdentity};

use super::{
    EligibleFindingKind, FindingDisposition, ORGANIZATION_FLOOR_SCHEMA, Profile, ResourceName,
    root, sorted_set, validate_owner, validate_repository,
};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum OrganizationFloorSchema {
    #[strum(serialize = "amiss/organization-floor")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceLimit {
    pub resource: ResourceName,
    pub maximum: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationFloor {
    pub schema: OrganizationFloorSchema,
    pub floor_id: ArtifactId,
    pub repository: RepositoryIdentity,
    #[serde(rename = "ref")]
    pub ref_name: BranchRef,
    pub minimum_profile: Profile,
    pub minimum_dispositions: Vec<FindingDisposition>,
    pub protected_inventory: Vec<RepoPathText>,
    pub protected_control_paths: Vec<RepoPathText>,
    pub waivable_finding_kinds: Vec<EligibleFindingKind>,
    pub authorized_debt_owners: Vec<OwnerId>,
    pub authorized_waiver_issuers: Vec<OwnerId>,
    pub resource_limits: Vec<ResourceLimit>,
}

/// A floor rejection: a schema-layer defect, or the combined
/// `organization-policy-entries` count crossing its effective limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FloorDefect {
    Schema(Error),
    Entries {
        configured_limit: u64,
        observed_lower_bound: u64,
    },
}

pub const ORGANIZATION_POLICY_ENTRIES_LIMIT: u64 = 100_000;

/// Parses and validates one organization floor.
///
/// # Errors
///
/// Fails on strict-JSON defects, schema-shape violations, unknown fields,
/// invalid grammar values, per-resource bound violations, unsorted or
/// duplicate set members, and a combined entry count over the built-in
/// `organization-policy-entries` limit or a tighter self-declared one.
pub fn parse_organization_floor(bytes: &[u8]) -> Result<OrganizationFloor, FloorDefect> {
    root(bytes).map_err(FloorDefect::Schema)?;
    let floor = de::deserialize_json(bytes).map_err(FloorDefect::Schema)?;
    validate_organization_floor(&floor)?;
    Ok(floor)
}

/// Produces one valid organization floor's canonical bytes and digest.
///
/// # Errors
///
/// A public field violates the same laws [`parse_organization_floor`]
/// enforces, or the typed value cannot be serialized.
pub fn canonical_organization_floor(
    floor: &OrganizationFloor,
) -> Result<(Vec<u8>, Digest), FloorDefect> {
    validate_organization_floor(floor)?;
    let bytes = serde_json_canonicalizer::to_vec(floor)
        .map_err(|_defect| FloorDefect::Schema(Error::new("$", ErrorKind::InvalidValue)))?;
    let digest = hb(ORGANIZATION_FLOOR_SCHEMA, &bytes);
    Ok((bytes, digest))
}

fn validate_organization_floor(floor: &OrganizationFloor) -> Result<(), FloorDefect> {
    validate_repository("$.repository", &floor.repository).map_err(FloorDefect::Schema)?;
    let combined = [
        floor.minimum_dispositions.len(),
        floor.protected_inventory.len(),
        floor.protected_control_paths.len(),
        floor.waivable_finding_kinds.len(),
        floor.authorized_debt_owners.len(),
        floor.authorized_waiver_issuers.len(),
        floor.resource_limits.len(),
    ]
    .into_iter()
    .map(|length| u64::try_from(length).unwrap_or(u64::MAX))
    .fold(0_u64, u64::saturating_add);
    if combined > ORGANIZATION_POLICY_ENTRIES_LIMIT {
        return Err(FloorDefect::Entries {
            configured_limit: ORGANIZATION_POLICY_ENTRIES_LIMIT,
            observed_lower_bound: ORGANIZATION_POLICY_ENTRIES_LIMIT.saturating_add(1),
        });
    }

    validate_floor_shape(floor).map_err(FloorDefect::Schema)?;
    if let Some(declared) = floor
        .resource_limits
        .iter()
        .find(|row| row.resource == ResourceName::OrganizationPolicyEntries)
    {
        let declared = u64::try_from(declared.maximum).unwrap_or(u64::MAX);
        if combined > declared {
            return Err(FloorDefect::Entries {
                configured_limit: declared,
                observed_lower_bound: declared.saturating_add(1),
            });
        }
    }
    Ok(())
}

fn validate_floor_shape(floor: &OrganizationFloor) -> Result<(), Error> {
    if floor.minimum_dispositions.len() > 3 {
        return fail("$.minimum_dispositions", ErrorKind::LimitExceeded);
    }
    sorted_set(
        "$.minimum_dispositions",
        &floor.minimum_dispositions,
        |left, right| left.finding_kind.as_ref().cmp(right.finding_kind.as_ref()),
    )?;

    for (path, items) in [
        ("$.protected_inventory", &floor.protected_inventory),
        ("$.protected_control_paths", &floor.protected_control_paths),
    ] {
        if items.len() > 100_000 {
            return fail(path, ErrorKind::LimitExceeded);
        }
        sorted_set(path, items, |left, right| left.as_str().cmp(right.as_str()))?;
    }

    if floor.waivable_finding_kinds.len() > 2 {
        return fail("$.waivable_finding_kinds", ErrorKind::LimitExceeded);
    }
    sorted_set(
        "$.waivable_finding_kinds",
        &floor.waivable_finding_kinds,
        |left, right| left.as_ref().cmp(right.as_ref()),
    )?;

    for (path, owners) in [
        ("$.authorized_debt_owners", &floor.authorized_debt_owners),
        (
            "$.authorized_waiver_issuers",
            &floor.authorized_waiver_issuers,
        ),
    ] {
        if owners.len() > 10_000 {
            return fail(path, ErrorKind::LimitExceeded);
        }
        for (index, owner) in owners.iter().enumerate() {
            validate_owner(&format!("{path}[{index}]"), owner)?;
        }
        sorted_set(path, owners, |left, right| {
            left.as_str().cmp(right.as_str())
        })?;
    }

    if floor.resource_limits.len() > ResourceName::all().len() {
        return fail("$.resource_limits", ErrorKind::LimitExceeded);
    }
    for (index, limit) in floor.resource_limits.iter().enumerate() {
        if !resource_maximum_valid(limit.resource, limit.maximum) {
            return fail(
                &format!("$.resource_limits[{index}].maximum"),
                ErrorKind::InvalidValue,
            );
        }
    }
    sorted_set(
        "$.resource_limits",
        &floor.resource_limits,
        |left, right| left.resource.as_str().cmp(right.resource.as_str()),
    )
}

fn resource_maximum_valid(resource: ResourceName, maximum: i64) -> bool {
    if resource == ResourceName::TypedAnalysisErrorsRetained {
        (1..=64).contains(&maximum)
    } else if resource == ResourceName::MachineJsonBytes {
        u64::try_from(maximum).is_ok_and(|value| value == crate::report::MACHINE_JSON_BYTES)
    } else {
        (0..=crate::json::MAX_SAFE_INTEGER).contains(&maximum)
    }
}
