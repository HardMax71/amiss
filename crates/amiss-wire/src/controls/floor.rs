use crate::de::{self, Error, Obj};
use crate::digest::{Digest, hj};
use crate::model::{ArtifactId, BranchRef, OwnerId, RepoPathText, RepositoryIdentity};

use super::{
    Disposition, EligibleFindingKind, FindingDisposition, ORGANIZATION_FLOOR_SCHEMA, Profile,
    PromotableFindingKind, ResourceName, decode_artifact_id, decode_branch_ref,
    decode_disposition_rule, decode_items, decode_owner_items, decode_path_items,
    decode_repository, decode_resource_limit, root, sorted_set,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceLimit {
    pub resource: ResourceName,
    pub maximum: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FloorDisposition {
    pub finding_kind: PromotableFindingKind,
    pub disposition: Disposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrganizationFloor {
    digest: Digest,
    floor_id: ArtifactId,
    repository: RepositoryIdentity,
    ref_name: BranchRef,
    minimum_profile: Profile,
    minimum_dispositions: Vec<FindingDisposition>,
    protected_inventory: Vec<RepoPathText>,
    protected_control_paths: Vec<RepoPathText>,
    waivable_finding_kinds: Vec<EligibleFindingKind>,
    authorized_debt_owners: Vec<OwnerId>,
    authorized_waiver_issuers: Vec<OwnerId>,
    resource_limits: Vec<ResourceLimit>,
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

impl From<Error> for FloorDefect {
    fn from(error: Error) -> Self {
        Self::Schema(error)
    }
}

pub const ORGANIZATION_POLICY_ENTRIES_LIMIT: u64 = 100_000;

impl OrganizationFloor {
    #[must_use]
    pub const fn digest(&self) -> Digest {
        self.digest
    }

    #[must_use]
    pub const fn minimum_profile(&self) -> Profile {
        self.minimum_profile
    }

    #[must_use]
    pub fn floor_id(&self) -> &ArtifactId {
        &self.floor_id
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
    pub fn minimum_dispositions(&self) -> &[FindingDisposition] {
        &self.minimum_dispositions
    }

    #[must_use]
    pub fn protected_inventory(&self) -> &[RepoPathText] {
        &self.protected_inventory
    }

    #[must_use]
    pub fn protected_control_paths(&self) -> &[RepoPathText] {
        &self.protected_control_paths
    }

    #[must_use]
    pub fn waivable_finding_kinds(&self) -> &[EligibleFindingKind] {
        &self.waivable_finding_kinds
    }

    #[must_use]
    pub fn authorized_debt_owners(&self) -> &[OwnerId] {
        &self.authorized_debt_owners
    }

    #[must_use]
    pub fn authorized_waiver_issuers(&self) -> &[OwnerId] {
        &self.authorized_waiver_issuers
    }

    #[must_use]
    pub fn resource_limits(&self) -> &[ResourceLimit] {
        &self.resource_limits
    }

    #[must_use]
    pub const fn schema(&self) -> &'static str {
        ORGANIZATION_FLOOR_SCHEMA
    }

    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, unknown fields,
    /// invalid grammar values, per-resource bound violations, unsorted or
    /// duplicate set members, and a combined entry count over the built-in
    /// `organization-policy-entries` limit or a tighter self-declared one.
    pub fn parse(bytes: &[u8]) -> Result<Self, FloorDefect> {
        let value = root(bytes)?;
        let digest = hj(ORGANIZATION_FLOOR_SCHEMA, &value);
        let mut obj = Obj::new("$", value)?;
        obj.required("schema", |path, value| {
            de::const_str(path, value, ORGANIZATION_FLOOR_SCHEMA)
        })?;

        let floor_id = obj.required("floor_id", decode_artifact_id)?;
        let repository = obj.required("repository", decode_repository)?;
        let ref_name = obj.required("ref", decode_branch_ref)?;
        let minimum_profile = obj.required("minimum_profile", Profile::decode)?;

        let dispositions_path = obj.field("minimum_dispositions");
        let dispositions_raw = de::array(&dispositions_path, obj.take("minimum_dispositions")?)?;
        let inventory_path = obj.field("protected_inventory");
        let inventory_raw = de::array(&inventory_path, obj.take("protected_inventory")?)?;
        let control_paths_path = obj.field("protected_control_paths");
        let control_paths_raw =
            de::array(&control_paths_path, obj.take("protected_control_paths")?)?;
        let waivable_path = obj.field("waivable_finding_kinds");
        let waivable_raw = de::array(&waivable_path, obj.take("waivable_finding_kinds")?)?;
        let owners_path = obj.field("authorized_debt_owners");
        let owners_raw = de::array(&owners_path, obj.take("authorized_debt_owners")?)?;
        let issuers_path = obj.field("authorized_waiver_issuers");
        let issuers_raw = de::array(&issuers_path, obj.take("authorized_waiver_issuers")?)?;
        let limits_path = obj.field("resource_limits");
        let limits_raw = de::array(&limits_path, obj.take("resource_limits")?)?;

        let combined = [
            dispositions_raw.len(),
            inventory_raw.len(),
            control_paths_raw.len(),
            waivable_raw.len(),
            owners_raw.len(),
            issuers_raw.len(),
            limits_raw.len(),
        ]
        .iter()
        .map(|&len| u64::try_from(len).unwrap_or(u64::MAX))
        .fold(0_u64, u64::saturating_add);
        if combined > ORGANIZATION_POLICY_ENTRIES_LIMIT {
            return Err(FloorDefect::Entries {
                configured_limit: ORGANIZATION_POLICY_ENTRIES_LIMIT,
                observed_lower_bound: ORGANIZATION_POLICY_ENTRIES_LIMIT.saturating_add(1),
            });
        }

        let minimum_dispositions = decode_items(
            &dispositions_path,
            dispositions_raw,
            3,
            decode_disposition_rule,
        )?;
        sorted_set(&dispositions_path, &minimum_dispositions, |a, b| {
            a.finding_kind.as_str().cmp(b.finding_kind.as_str())
        })?;
        let protected_inventory = decode_path_items(&inventory_path, inventory_raw)?;
        let protected_control_paths = decode_path_items(&control_paths_path, control_paths_raw)?;
        let waivable_finding_kinds =
            decode_items(&waivable_path, waivable_raw, 2, |path, value| {
                EligibleFindingKind::decode(path, value)
            })?;
        sorted_set(&waivable_path, &waivable_finding_kinds, |a, b| {
            a.as_str().cmp(b.as_str())
        })?;
        let authorized_debt_owners = decode_owner_items(&owners_path, owners_raw)?;
        let authorized_waiver_issuers = decode_owner_items(&issuers_path, issuers_raw)?;
        let cap = ResourceName::all().len();
        let resource_limits = decode_items(&limits_path, limits_raw, cap, decode_resource_limit)?;
        sorted_set(&limits_path, &resource_limits, |a, b| {
            a.resource.as_str().cmp(b.resource.as_str())
        })?;

        obj.finish()?;
        if let Some(declared) = resource_limits
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
        Ok(Self {
            digest,
            floor_id,
            repository,
            ref_name,
            minimum_profile,
            minimum_dispositions,
            protected_inventory,
            protected_control_paths,
            waivable_finding_kinds,
            authorized_debt_owners,
            authorized_waiver_issuers,
            resource_limits,
        })
    }
}
