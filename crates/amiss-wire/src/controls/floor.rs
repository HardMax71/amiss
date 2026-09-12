use super::{
    Disposition, EligibleFindingKind, FindingDisposition, ORGANIZATION_FLOOR_SCHEMA, Profile,
    PromotableFindingKind, ResourceName, check_len, check_schema, in_bounds, root, sorted_set,
};
use crate::codec;
use crate::de::{Error, ErrorKind};
use crate::digest::{Digest, hj};
use crate::json::Value;
use crate::model::{ArtifactId, BranchRef, OwnerId, RepoPathText, RepositoryIdentity};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceLimit {
    pub resource: ResourceName,
    pub maximum: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
        Self::from_value(&root(bytes)?)
    }

    /// Checks an already decoded floor and its combined entry budget.
    ///
    /// # Errors
    ///
    /// The floor violates its shape, ordering, resource bounds, or total budget.
    pub fn from_value(value: &Value) -> Result<Self, FloorDefect> {
        let payload: FloorPayload = codec::from_value("$", value)?;
        payload.check()?;
        Ok(Self {
            digest: hj(ORGANIZATION_FLOOR_SCHEMA, value),
            floor_id: payload.floor_id,
            repository: payload.repository,
            ref_name: payload.ref_name,
            minimum_profile: payload.minimum_profile,
            minimum_dispositions: payload.minimum_dispositions,
            protected_inventory: payload.protected_inventory,
            protected_control_paths: payload.protected_control_paths,
            waivable_finding_kinds: payload.waivable_finding_kinds,
            authorized_debt_owners: payload.authorized_debt_owners,
            authorized_waiver_issuers: payload.authorized_waiver_issuers,
            resource_limits: payload.resource_limits,
        })
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FloorPayload {
    schema: String,
    floor_id: ArtifactId,
    repository: RepositoryIdentity,
    #[serde(rename = "ref")]
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

impl FloorPayload {
    fn check(&self) -> Result<(), FloorDefect> {
        check_schema("$.schema", &self.schema, ORGANIZATION_FLOOR_SCHEMA)?;
        let combined = [
            self.minimum_dispositions.len(),
            self.protected_inventory.len(),
            self.protected_control_paths.len(),
            self.waivable_finding_kinds.len(),
            self.authorized_debt_owners.len(),
            self.authorized_waiver_issuers.len(),
            self.resource_limits.len(),
        ]
        .into_iter()
        .map(|len| u64::try_from(len).unwrap_or(u64::MAX))
        .fold(0_u64, u64::saturating_add);
        entries_limit(combined, ORGANIZATION_POLICY_ENTRIES_LIMIT)?;
        check_len("$.minimum_dispositions", self.minimum_dispositions.len(), 3)?;
        sorted_set(
            "$.minimum_dispositions",
            &self.minimum_dispositions,
            |a, b| a.finding_kind.as_ref().cmp(b.finding_kind.as_ref()),
        )?;
        for (path, paths) in [
            ("$.protected_inventory", &self.protected_inventory),
            ("$.protected_control_paths", &self.protected_control_paths),
        ] {
            check_len(path, paths.len(), 100_000)?;
            sorted_set(path, paths, Ord::cmp)?;
        }
        check_len(
            "$.waivable_finding_kinds",
            self.waivable_finding_kinds.len(),
            2,
        )?;
        sorted_set(
            "$.waivable_finding_kinds",
            &self.waivable_finding_kinds,
            |a, b| a.as_ref().cmp(b.as_ref()),
        )?;
        for (path, owners) in [
            ("$.authorized_debt_owners", &self.authorized_debt_owners),
            (
                "$.authorized_waiver_issuers",
                &self.authorized_waiver_issuers,
            ),
        ] {
            check_len(path, owners.len(), 10_000)?;
            sorted_set(path, owners, Ord::cmp)?;
        }
        check_len(
            "$.resource_limits",
            self.resource_limits.len(),
            ResourceName::all().len(),
        )?;
        for (index, limit) in self.resource_limits.iter().enumerate() {
            if !in_bounds(limit.resource, limit.maximum) {
                return Err(Error::new(
                    &format!("$.resource_limits[{index}].maximum"),
                    ErrorKind::InvalidValue,
                )
                .into());
            }
        }
        sorted_set("$.resource_limits", &self.resource_limits, |a, b| {
            a.resource.as_str().cmp(b.resource.as_str())
        })?;
        if let Some(declared) = self
            .resource_limits
            .iter()
            .find(|row| row.resource == ResourceName::OrganizationPolicyEntries)
        {
            entries_limit(
                combined,
                u64::try_from(declared.maximum).unwrap_or(u64::MAX),
            )?;
        }
        Ok(())
    }
}

fn entries_limit(combined: u64, limit: u64) -> Result<(), FloorDefect> {
    if combined > limit {
        Err(FloorDefect::Entries {
            configured_limit: limit,
            observed_lower_bound: limit.saturating_add(1),
        })
    } else {
        Ok(())
    }
}
