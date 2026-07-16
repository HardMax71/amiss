use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::json::{self, Value};
use crate::model::{
    ArtifactId, BranchRef, ObjectFormat, Oid, OwnerId, RepoPathText, RepositoryIdentity,
    TreeIdentity, UtcInstant,
};

pub const SCANNER_POLICY_PATH: &str = ".amiss/scanner-policy.json";

const SCANNER_POLICY_SCHEMA: &str = "amiss/scanner-policy/v1";
const ORGANIZATION_FLOOR_SCHEMA: &str = "amiss/organization-floor/v1";
const DEBT_SNAPSHOT_SCHEMA: &str = "amiss/debt-snapshot/v1";
const WAIVER_BUNDLE_SCHEMA: &str = "amiss/waiver-bundle/v1";
const TRUSTED_TIME_STATEMENT_SCHEMA: &str = "amiss/scanner-trusted-time-statement/v1";
const TRUSTED_TIME_CONTROLLER: &str = "github-actions-required-workflow-clock-v1";
const EXECUTION_CONSTRAINT_SCHEMA: &str = "amiss/scanner-execution-constraint/v1";
const ACTION_BOOTSTRAP_CONTRACT: &str = "amiss-action-bootstrap-v1";

/// The controller's maximum statement lifetime: `evaluation_instant <
/// valid_until <= evaluation_instant + 600` whole seconds.
pub const STATEMENT_TTL_MAX_SECONDS: i64 = 600;
const FINDING_KEY_INPUT_SCHEMA: &str = "amiss/scanner-finding-key-input/v1";
const FACT_SCHEMA: &str = "amiss/scanner-fact/v1";
pub const FINDING_KEY_DOMAIN: &str = "amiss/scanner-finding-key/v1";
pub const FACT_DOMAIN: &str = "amiss/scanner-fact/v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum IncludeKind {
    Document,
    Tree,
}

impl IncludeKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Tree => "tree",
        }
    }

    fn decode(path: &str, value: Value) -> Result<Self, Error> {
        match de::string(path, value)?.as_str() {
            "document" => Ok(Self::Document),
            "tree" => Ok(Self::Tree),
            _ => fail(path, ErrorKind::InvalidValue),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Disposition {
    Warn,
    Fail,
}

impl Disposition {
    fn decode(path: &str, value: Value) -> Result<Self, Error> {
        match de::string(path, value)?.as_str() {
            "warn" => Ok(Self::Warn),
            "fail" => Ok(Self::Fail),
            _ => fail(path, ErrorKind::InvalidValue),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Profile {
    Observe,
    Enforce,
}

impl Profile {
    /// # Errors
    ///
    /// A value outside the closed `observe`/`enforce` pair.
    pub fn decode(path: &str, value: Value) -> Result<Self, Error> {
        match de::string(path, value)?.as_str() {
            "observe" => Ok(Self::Observe),
            "enforce" => Ok(Self::Enforce),
            _ => fail(path, ErrorKind::InvalidValue),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PromotableFindingKind {
    ExplicitTargetMissing,
    ExplicitTargetTypeMismatch,
    InvalidReference,
}

impl PromotableFindingKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitTargetMissing => "explicit-target-missing",
            Self::ExplicitTargetTypeMismatch => "explicit-target-type-mismatch",
            Self::InvalidReference => "invalid-reference",
        }
    }

    fn decode(path: &str, value: Value) -> Result<Self, Error> {
        match de::string(path, value)?.as_str() {
            "explicit-target-missing" => Ok(Self::ExplicitTargetMissing),
            "explicit-target-type-mismatch" => Ok(Self::ExplicitTargetTypeMismatch),
            "invalid-reference" => Ok(Self::InvalidReference),
            _ => fail(path, ErrorKind::InvalidValue),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EligibleFindingKind {
    ExplicitTargetMissing,
    ExplicitTargetTypeMismatch,
}

impl EligibleFindingKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitTargetMissing => "explicit-target-missing",
            Self::ExplicitTargetTypeMismatch => "explicit-target-type-mismatch",
        }
    }

    fn decode(path: &str, value: Value) -> Result<Self, Error> {
        match de::string(path, value)?.as_str() {
            "explicit-target-missing" => Ok(Self::ExplicitTargetMissing),
            "explicit-target-type-mismatch" => Ok(Self::ExplicitTargetTypeMismatch),
            _ => fail(path, ErrorKind::InvalidValue),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceConstruct {
    InlineLink,
    FullReferenceLink,
    CollapsedReferenceLink,
    ShortcutReferenceLink,
    Autolink,
    InlineImage,
    FullReferenceImage,
    CollapsedReferenceImage,
    ShortcutReferenceImage,
}

impl SourceConstruct {
    /// Whether the consuming syntax node is an image form, which fixes the
    /// authored target kind.
    #[must_use]
    pub const fn is_image(self) -> bool {
        match self {
            Self::InlineImage
            | Self::FullReferenceImage
            | Self::CollapsedReferenceImage
            | Self::ShortcutReferenceImage => true,
            Self::InlineLink
            | Self::FullReferenceLink
            | Self::CollapsedReferenceLink
            | Self::ShortcutReferenceLink
            | Self::Autolink => false,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InlineLink => "markdown-inline-link",
            Self::FullReferenceLink => "markdown-full-reference-link",
            Self::CollapsedReferenceLink => "markdown-collapsed-reference-link",
            Self::ShortcutReferenceLink => "markdown-shortcut-reference-link",
            Self::Autolink => "markdown-autolink",
            Self::InlineImage => "markdown-inline-image",
            Self::FullReferenceImage => "markdown-full-reference-image",
            Self::CollapsedReferenceImage => "markdown-collapsed-reference-image",
            Self::ShortcutReferenceImage => "markdown-shortcut-reference-image",
        }
    }

    fn decode(path: &str, value: Value) -> Result<Self, Error> {
        match de::string(path, value)?.as_str() {
            "markdown-inline-link" => Ok(Self::InlineLink),
            "markdown-full-reference-link" => Ok(Self::FullReferenceLink),
            "markdown-collapsed-reference-link" => Ok(Self::CollapsedReferenceLink),
            "markdown-shortcut-reference-link" => Ok(Self::ShortcutReferenceLink),
            "markdown-autolink" => Ok(Self::Autolink),
            "markdown-inline-image" => Ok(Self::InlineImage),
            "markdown-full-reference-image" => Ok(Self::FullReferenceImage),
            "markdown-collapsed-reference-image" => Ok(Self::CollapsedReferenceImage),
            "markdown-shortcut-reference-image" => Ok(Self::ShortcutReferenceImage),
            _ => fail(path, ErrorKind::InvalidValue),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TargetKind {
    Blob,
    Tree,
    Either,
}

impl TargetKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blob => "blob",
            Self::Tree => "tree",
            Self::Either => "either",
        }
    }

    fn decode(path: &str, value: Value) -> Result<Self, Error> {
        match de::string(path, value)?.as_str() {
            "blob" => Ok(Self::Blob),
            "tree" => Ok(Self::Tree),
            "either" => Ok(Self::Either),
            _ => fail(path, ErrorKind::InvalidValue),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Blob,
    Tree,
    Symlink,
    Gitlink,
}

impl EntryKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Blob => "blob",
            Self::Tree => "tree",
            Self::Symlink => "symlink",
            Self::Gitlink => "gitlink",
        }
    }

    fn decode(path: &str, value: Value) -> Result<Self, Error> {
        match de::string(path, value)?.as_str() {
            "blob" => Ok(Self::Blob),
            "tree" => Ok(Self::Tree),
            "symlink" => Ok(Self::Symlink),
            "gitlink" => Ok(Self::Gitlink),
            _ => fail(path, ErrorKind::InvalidValue),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GitMode {
    RegularFile,
    ExecutableFile,
    Tree,
    Symlink,
    Gitlink,
}

impl GitMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RegularFile => "100644",
            Self::ExecutableFile => "100755",
            Self::Tree => "040000",
            Self::Symlink => "120000",
            Self::Gitlink => "160000",
        }
    }

    fn decode(path: &str, value: Value) -> Result<Self, Error> {
        match de::string(path, value)?.as_str() {
            "100644" => Ok(Self::RegularFile),
            "100755" => Ok(Self::ExecutableFile),
            "040000" => Ok(Self::Tree),
            "120000" => Ok(Self::Symlink),
            "160000" => Ok(Self::Gitlink),
            _ => fail(path, ErrorKind::InvalidValue),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentAvailability {
    Available,
    NotRead,
    NotApplicable,
    LfsPointerOnly,
}

impl ContentAvailability {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::NotRead => "not-read",
            Self::NotApplicable => "not-applicable",
            Self::LfsPointerOnly => "lfs-pointer-only",
        }
    }

    fn decode(path: &str, value: Value) -> Result<Self, Error> {
        match de::string(path, value)?.as_str() {
            "available" => Ok(Self::Available),
            "not-read" => Ok(Self::NotRead),
            "not-applicable" => Ok(Self::NotApplicable),
            "lfs-pointer-only" => Ok(Self::LfsPointerOnly),
            _ => fail(path, ErrorKind::InvalidValue),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolutionKind {
    Missing,
    TypeMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResourceName {
    GitObjectBytes,
    GitCompressedObjectBytes,
    AggregateGitCompressedObjectBytesPerEvaluation,
    GitPackDirectoryEntries,
    GitPackFiles,
    GitPackIndexBytes,
    AggregateGitPackIndexBytes,
    GitDeltaDepth,
    GitIndexBytes,
    GitTreeEntriesPerSnapshot,
    DocumentsPerSnapshot,
    ControlInputBytes,
    SelectedControlBlobBytes,
    AggregateSelectedControlBytesPerSnapshot,
    RepositoryPolicyEntries,
    DebtItems,
    WaiverItems,
    RawPathBytes,
    DocumentBlobBytes,
    ReferencedTargetBlobBytes,
    AggregateReferencedTargetBytesPerSnapshot,
    AggregateDocumentBytesPerSnapshot,
    RawLinkDestinationBytes,
    ParserNesting,
    ParserNodesPerDocument,
    ParserNodesPerSnapshot,
    ReferencesPerDocument,
    ReferencesPerSnapshot,
    OrganizationPolicyEntries,
    CompleteFindings,
    TypedAnalysisErrorsRetained,
    MachineJsonBytes,
    PrivateTemporaryStorageBytes,
    EvaluatorManagedMemoryBytes,
}

impl ResourceName {
    /// Every resource name in wire-contract order.
    #[must_use]
    pub fn all() -> impl ExactSizeIterator<Item = Self> {
        Self::ALL.iter().map(|(_, resource)| *resource)
    }

    /// The phase a resource crossing reports, from the closed partition.
    #[must_use]
    pub const fn phase(self) -> &'static str {
        match self {
            Self::ControlInputBytes
            | Self::RepositoryPolicyEntries
            | Self::DebtItems
            | Self::WaiverItems
            | Self::OrganizationPolicyEntries => "configuration",
            Self::GitObjectBytes
            | Self::GitCompressedObjectBytes
            | Self::AggregateGitCompressedObjectBytesPerEvaluation
            | Self::GitPackDirectoryEntries
            | Self::GitPackFiles
            | Self::GitPackIndexBytes
            | Self::AggregateGitPackIndexBytes
            | Self::GitDeltaDepth
            | Self::GitIndexBytes
            | Self::GitTreeEntriesPerSnapshot
            | Self::RawPathBytes => "git",
            Self::DocumentsPerSnapshot
            | Self::DocumentBlobBytes
            | Self::AggregateDocumentBytesPerSnapshot
            | Self::SelectedControlBlobBytes
            | Self::AggregateSelectedControlBytesPerSnapshot => "discovery",
            Self::RawLinkDestinationBytes
            | Self::ParserNesting
            | Self::ParserNodesPerDocument
            | Self::ParserNodesPerSnapshot
            | Self::ReferencesPerDocument
            | Self::ReferencesPerSnapshot => "parse",
            Self::ReferencedTargetBlobBytes | Self::AggregateReferencedTargetBytesPerSnapshot => {
                "resolution"
            }
            Self::CompleteFindings => "policy",
            Self::MachineJsonBytes => "output",
            Self::TypedAnalysisErrorsRetained
            | Self::PrivateTemporaryStorageBytes
            | Self::EvaluatorManagedMemoryBytes => "internal",
        }
    }

    const ALL: [(&'static str, Self); 34] = [
        ("git-object-bytes", Self::GitObjectBytes),
        (
            "git-compressed-object-bytes",
            Self::GitCompressedObjectBytes,
        ),
        (
            "aggregate-git-compressed-object-bytes-per-evaluation",
            Self::AggregateGitCompressedObjectBytesPerEvaluation,
        ),
        ("git-pack-directory-entries", Self::GitPackDirectoryEntries),
        ("git-pack-files", Self::GitPackFiles),
        ("git-pack-index-bytes", Self::GitPackIndexBytes),
        (
            "aggregate-git-pack-index-bytes",
            Self::AggregateGitPackIndexBytes,
        ),
        ("git-delta-depth", Self::GitDeltaDepth),
        ("git-index-bytes", Self::GitIndexBytes),
        (
            "git-tree-entries-per-snapshot",
            Self::GitTreeEntriesPerSnapshot,
        ),
        ("documents-per-snapshot", Self::DocumentsPerSnapshot),
        ("control-input-bytes", Self::ControlInputBytes),
        (
            "selected-control-blob-bytes",
            Self::SelectedControlBlobBytes,
        ),
        (
            "aggregate-selected-control-bytes-per-snapshot",
            Self::AggregateSelectedControlBytesPerSnapshot,
        ),
        ("repository-policy-entries", Self::RepositoryPolicyEntries),
        ("debt-items", Self::DebtItems),
        ("waiver-items", Self::WaiverItems),
        ("raw-path-bytes", Self::RawPathBytes),
        ("document-blob-bytes", Self::DocumentBlobBytes),
        (
            "referenced-target-blob-bytes",
            Self::ReferencedTargetBlobBytes,
        ),
        (
            "aggregate-referenced-target-bytes-per-snapshot",
            Self::AggregateReferencedTargetBytesPerSnapshot,
        ),
        (
            "aggregate-document-bytes-per-snapshot",
            Self::AggregateDocumentBytesPerSnapshot,
        ),
        ("raw-link-destination-bytes", Self::RawLinkDestinationBytes),
        ("parser-nesting", Self::ParserNesting),
        ("parser-nodes-per-document", Self::ParserNodesPerDocument),
        ("parser-nodes-per-snapshot", Self::ParserNodesPerSnapshot),
        ("references-per-document", Self::ReferencesPerDocument),
        ("references-per-snapshot", Self::ReferencesPerSnapshot),
        (
            "organization-policy-entries",
            Self::OrganizationPolicyEntries,
        ),
        ("complete-findings", Self::CompleteFindings),
        (
            "typed-analysis-errors-retained",
            Self::TypedAnalysisErrorsRetained,
        ),
        ("machine-json-bytes", Self::MachineJsonBytes),
        (
            "private-temporary-storage-bytes",
            Self::PrivateTemporaryStorageBytes,
        ),
        (
            "evaluator-managed-memory-bytes",
            Self::EvaluatorManagedMemoryBytes,
        ),
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, variant)| *variant == self)
            .map_or("", |(name, _)| name)
    }

    fn decode(path: &str, value: Value) -> Result<Self, Error> {
        let raw = de::string(path, value)?;
        Self::ALL
            .iter()
            .find(|(name, _)| *name == raw)
            .map(|(_, variant)| *variant)
            .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentInclude {
    pub path: RepoPathText,
    pub kind: IncludeKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingDisposition {
    pub finding_kind: PromotableFindingKind,
    pub disposition: Disposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScannerPolicy {
    pub digest: Digest,
    pub document_includes: Vec<DocumentInclude>,
    pub protected_inventory: Vec<RepoPathText>,
    pub finding_dispositions: Vec<FindingDisposition>,
}

impl ScannerPolicy {
    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, unknown fields,
    /// invalid grammar values, and unsorted or duplicate set members.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let value = root(bytes)?;
        let digest = hj(SCANNER_POLICY_SCHEMA, &value);
        let mut obj = Obj::new("$", value)?;
        de::const_str(
            &obj.field("schema"),
            obj.take("schema")?,
            SCANNER_POLICY_SCHEMA,
        )?;

        let includes_path = obj.field("document_includes");
        let includes = de::array(&includes_path, obj.take("document_includes")?)?;
        let document_includes = decode_items(&includes_path, includes, 100_000, decode_include)?;
        sorted_set(&includes_path, &document_includes, |a, b| {
            (a.path.as_str(), a.kind).cmp(&(b.path.as_str(), b.kind))
        })?;

        let inventory_path = obj.field("protected_inventory");
        let protected_inventory =
            decode_path_set(&inventory_path, obj.take("protected_inventory")?)?;

        let dispositions_path = obj.field("finding_dispositions");
        let raw = de::array(&dispositions_path, obj.take("finding_dispositions")?)?;
        let finding_dispositions =
            decode_items(&dispositions_path, raw, 3, decode_disposition_rule)?;
        sorted_set(&dispositions_path, &finding_dispositions, |a, b| {
            a.finding_kind.as_str().cmp(b.finding_kind.as_str())
        })?;

        obj.finish()?;
        Ok(Self {
            digest,
            document_includes,
            protected_inventory,
            finding_dispositions,
        })
    }
}

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
    pub digest: Digest,
    pub floor_id: ArtifactId,
    pub repository: RepositoryIdentity,
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

impl From<Error> for FloorDefect {
    fn from(error: Error) -> Self {
        Self::Schema(error)
    }
}

pub const ORGANIZATION_POLICY_ENTRIES_LIMIT: u64 = 100_000;

impl OrganizationFloor {
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
        de::const_str(
            &obj.field("schema"),
            obj.take("schema")?,
            ORGANIZATION_FLOOR_SCHEMA,
        )?;

        let floor_id = decode_artifact_id(&obj.field("floor_id"), obj.take("floor_id")?)?;
        let repository = decode_repository(&obj.field("repository"), obj.take("repository")?)?;
        let ref_name = decode_branch_ref(&obj.field("ref"), obj.take("ref")?)?;
        let minimum_profile =
            Profile::decode(&obj.field("minimum_profile"), obj.take("minimum_profile")?)?;

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
        let resource_limits = decode_items(&limits_path, limits_raw, 34, decode_resource_limit)?;
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

/// A trusted-time statement issued by the required-workflow clock inside the
/// externally controlled run. Parsing establishes shape and the TTL law; the
/// evaluation-side bindings (repository, ref, candidate identity, run,
/// attempt) are separate verification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedTimeStatement {
    pub digest: Digest,
    pub repository: RepositoryIdentity,
    pub ref_name: BranchRef,
    pub candidate_identity_digest: Digest,
    pub provider_run_id: String,
    pub provider_run_attempt: u64,
    pub evaluation_instant: UtcInstant,
    pub valid_until: UtcInstant,
}

impl TrustedTimeStatement {
    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, invalid grammar
    /// values, and a lifetime outside `0 < valid_until - evaluation_instant
    /// <= 600` seconds.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let value = root(bytes)?;
        let digest = hj(TRUSTED_TIME_STATEMENT_SCHEMA, &value);
        let mut obj = Obj::new("$", value)?;
        de::const_str(
            &obj.field("schema"),
            obj.take("schema")?,
            TRUSTED_TIME_STATEMENT_SCHEMA,
        )?;
        de::const_str(
            &obj.field("controller"),
            obj.take("controller")?,
            TRUSTED_TIME_CONTROLLER,
        )?;
        let repository = decode_repository(&obj.field("repository"), obj.take("repository")?)?;
        let ref_name = decode_branch_ref(&obj.field("ref"), obj.take("ref")?)?;
        let candidate_identity_digest = decode_digest(
            &obj.field("candidate_identity_digest"),
            obj.take("candidate_identity_digest")?,
        )?;
        let run_id_path = obj.field("provider_run_id");
        let provider_run_id = de::string(&run_id_path, obj.take("provider_run_id")?)?;
        let run_id_bytes = provider_run_id.as_bytes();
        if run_id_bytes.is_empty()
            || run_id_bytes.len() > 32
            || !matches!(run_id_bytes.first(), Some(b'1'..=b'9'))
            || !run_id_bytes.iter().all(u8::is_ascii_digit)
        {
            return fail(&run_id_path, ErrorKind::InvalidValue);
        }
        let attempt_path = obj.field("provider_run_attempt");
        let attempt_raw = de::integer(&attempt_path, obj.take("provider_run_attempt")?)?;
        let provider_run_attempt = u64::try_from(attempt_raw)
            .ok()
            .filter(|attempt| *attempt >= 1)
            .ok_or_else(|| Error::new(&attempt_path, ErrorKind::InvalidValue))?;
        let evaluation_instant = decode_instant(
            &obj.field("evaluation_instant"),
            obj.take("evaluation_instant")?,
        )?;
        let until_path = obj.field("valid_until");
        let valid_until = decode_instant(&until_path, obj.take("valid_until")?)?;
        obj.finish()?;
        let lifetime = valid_until
            .epoch_seconds()
            .saturating_sub(evaluation_instant.epoch_seconds());
        if lifetime <= 0 || lifetime > STATEMENT_TTL_MAX_SECONDS {
            return fail(&until_path, ErrorKind::InvalidValue);
        }
        Ok(Self {
            digest,
            repository,
            ref_name,
            candidate_identity_digest,
            provider_run_id,
            provider_run_attempt,
            evaluation_instant,
            valid_until,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintPlatform {
    LinuxX8664,
    LinuxAarch64,
    MacosX8664,
    MacosAarch64,
    WindowsX8664,
    WindowsAarch64,
}

impl ConstraintPlatform {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LinuxX8664 => "linux-x86_64",
            Self::LinuxAarch64 => "linux-aarch64",
            Self::MacosX8664 => "macos-x86_64",
            Self::MacosAarch64 => "macos-aarch64",
            Self::WindowsX8664 => "windows-x86_64",
            Self::WindowsAarch64 => "windows-aarch64",
        }
    }

    /// # Errors
    ///
    /// A value outside the closed six-platform table.
    pub fn decode(path: &str, value: Value) -> Result<Self, Error> {
        match de::string(path, value)?.as_str() {
            "linux-x86_64" => Ok(Self::LinuxX8664),
            "linux-aarch64" => Ok(Self::LinuxAarch64),
            "macos-x86_64" => Ok(Self::MacosX8664),
            "macos-aarch64" => Ok(Self::MacosAarch64),
            "windows-x86_64" => Ok(Self::WindowsX8664),
            "windows-aarch64" => Ok(Self::WindowsAarch64),
            _ => fail(path, ErrorKind::InvalidValue),
        }
    }
}

/// The externally protected allow-list entry for one scanner action tree,
/// release manifest, bootstrap contract, and required provider status name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionConstraintDescriptor {
    pub digest: Digest,
    pub action_repository: RepositoryIdentity,
    pub action_object_format: ObjectFormat,
    pub action_commit_oid: Oid,
    pub action_tree_oid: Oid,
    pub manifest_path: RepoPathText,
    pub release_manifest_digest: Digest,
    pub selected_platform: ConstraintPlatform,
    pub required_status_name: String,
    pub bootstrap_digest: Digest,
}

fn decode_status_name(path: &str, value: Value) -> Result<String, Error> {
    let raw = de::string(path, value)?;
    let bytes = raw.as_bytes();
    let interior = |byte: &u8| {
        byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'.' | b'_' | b'/' | b'-')
    };
    let edge =
        |byte: &u8| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'/' | b'-');
    let valid = match (bytes.first(), bytes.last()) {
        (Some(first), Some(last)) => {
            bytes.len() <= 160
                && first.is_ascii_alphanumeric()
                && (bytes.len() == 1 || edge(last))
                && bytes.iter().all(interior)
        }
        _ => false,
    };
    if valid {
        Ok(raw)
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}

impl ExecutionConstraintDescriptor {
    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, and invalid
    /// grammar values.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let value = root(bytes)?;
        let digest = hj(EXECUTION_CONSTRAINT_SCHEMA, &value);
        let mut obj = Obj::new("$", value)?;
        de::const_str(
            &obj.field("schema"),
            obj.take("schema")?,
            EXECUTION_CONSTRAINT_SCHEMA,
        )?;
        let action_repository = decode_repository(
            &obj.field("action_repository"),
            obj.take("action_repository")?,
        )?;
        let format_path = obj.field("action_object_format");
        let action_object_format =
            match de::string(&format_path, obj.take("action_object_format")?)?.as_str() {
                "sha1" => ObjectFormat::Sha1,
                "sha256" => ObjectFormat::Sha256,
                _ => return fail(&format_path, ErrorKind::InvalidValue),
            };
        let commit_path = obj.field("action_commit_oid");
        let action_commit_oid = Oid::new(
            action_object_format,
            de::string(&commit_path, obj.take("action_commit_oid")?)?,
        )
        .ok_or_else(|| Error::new(&commit_path, ErrorKind::InvalidValue))?;
        let tree_path = obj.field("action_tree_oid");
        let action_tree_oid = Oid::new(
            action_object_format,
            de::string(&tree_path, obj.take("action_tree_oid")?)?,
        )
        .ok_or_else(|| Error::new(&tree_path, ErrorKind::InvalidValue))?;
        let manifest_path =
            decode_repo_path(&obj.field("manifest_path"), obj.take("manifest_path")?)?;
        let release_manifest_digest = decode_digest(
            &obj.field("release_manifest_digest"),
            obj.take("release_manifest_digest")?,
        )?;
        let selected_platform = ConstraintPlatform::decode(
            &obj.field("selected_platform"),
            obj.take("selected_platform")?,
        )?;
        let required_status_name = decode_status_name(
            &obj.field("required_status_name"),
            obj.take("required_status_name")?,
        )?;
        de::const_str(
            &obj.field("bootstrap_contract"),
            obj.take("bootstrap_contract")?,
            ACTION_BOOTSTRAP_CONTRACT,
        )?;
        let bootstrap_digest = decode_digest(
            &obj.field("bootstrap_digest"),
            obj.take("bootstrap_digest")?,
        )?;
        obj.finish()?;
        Ok(Self {
            digest,
            action_repository,
            action_object_format,
            action_commit_oid,
            action_tree_oid,
            manifest_path,
            release_manifest_digest,
            selected_platform,
            required_status_name,
            bootstrap_digest,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetIntent {
    pub path: RepoPathText,
    pub target_kind: TargetKind,
    pub query_digest: Option<Digest>,
    pub fragment_digest: Option<Digest>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingScope {
    pub document: RepoPathText,
    pub source_construct: SourceConstruct,
    pub normalized_target_intent: TargetIntent,
    pub source_projection_digest: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingKeyInput {
    pub finding_kind: EligibleFindingKind,
    pub scope: FindingScope,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolution {
    pub kind: ResolutionKind,
    pub path: Option<RepoPathText>,
    pub entry_kind: Option<EntryKind>,
    pub git_mode: Option<GitMode>,
    pub raw_digest: Option<Digest>,
    pub projection_digest: Option<Digest>,
    pub content_availability: ContentAvailability,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    pub finding_kind: EligibleFindingKind,
    pub key_input: FindingKeyInput,
    pub resolution: Resolution,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebtItem {
    pub debt_id: ArtifactId,
    pub finding_kind: EligibleFindingKind,
    pub key_input: FindingKeyInput,
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
    pub digest: Digest,
    pub repository: RepositoryIdentity,
    pub ref_name: BranchRef,
    pub organization_floor_digest: Digest,
    pub adoption_tree: TreeIdentity,
    pub adoption_report_payload_digest: Digest,
    pub created_at: UtcInstant,
    pub items: Vec<DebtItem>,
}

impl DebtSnapshot {
    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, embedded key or
    /// fact digests that do not recompute, kind or preimage inconsistencies,
    /// causal time-order violations, and unsorted or duplicate items or keys.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let value = root(bytes)?;
        let digest = hj(DEBT_SNAPSHOT_SCHEMA, &value);
        let mut obj = Obj::new("$", value)?;
        de::const_str(
            &obj.field("schema"),
            obj.take("schema")?,
            DEBT_SNAPSHOT_SCHEMA,
        )?;

        let repository = decode_repository(&obj.field("repository"), obj.take("repository")?)?;
        let ref_name = decode_branch_ref(&obj.field("ref"), obj.take("ref")?)?;
        let organization_floor_digest = decode_digest(
            &obj.field("organization_floor_digest"),
            obj.take("organization_floor_digest")?,
        )?;
        let adoption_tree = decode_tree(&obj.field("adoption_tree"), obj.take("adoption_tree")?)?;
        let adoption_report_payload_digest = decode_digest(
            &obj.field("adoption_report_payload_digest"),
            obj.take("adoption_report_payload_digest")?,
        )?;
        let created_at = decode_instant(&obj.field("created_at"), obj.take("created_at")?)?;

        let items_path = obj.field("items");
        let raw = de::array(&items_path, obj.take("items")?)?;
        let items = decode_items(&items_path, raw, 100_000, decode_debt_item)?;
        sorted_set(&items_path, &items, |a, b| {
            a.debt_id.as_str().cmp(b.debt_id.as_str())
        })?;
        let mut keys: BTreeSet<Digest> = BTreeSet::new();
        for item in &items {
            if !keys.insert(item.finding_key) {
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
            adoption_tree,
            adoption_report_payload_digest,
            created_at,
            items,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaiverItem {
    pub waiver_id: ArtifactId,
    pub finding_kind: EligibleFindingKind,
    pub key_input: FindingKeyInput,
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
    pub digest: Digest,
    pub repository: RepositoryIdentity,
    pub ref_name: BranchRef,
    pub organization_floor_digest: Digest,
    pub created_at: UtcInstant,
    pub items: Vec<WaiverItem>,
}

impl WaiverBundle {
    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, embedded key or
    /// fact digests that do not recompute, kind or preimage inconsistencies,
    /// causal time-order violations, duplicate waiver IDs, and duplicate
    /// `(candidate_tree, finding_key)` pairs.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let value = root(bytes)?;
        let digest = hj(WAIVER_BUNDLE_SCHEMA, &value);
        let mut obj = Obj::new("$", value)?;
        de::const_str(
            &obj.field("schema"),
            obj.take("schema")?,
            WAIVER_BUNDLE_SCHEMA,
        )?;

        let repository = decode_repository(&obj.field("repository"), obj.take("repository")?)?;
        let ref_name = decode_branch_ref(&obj.field("ref"), obj.take("ref")?)?;
        let organization_floor_digest = decode_digest(
            &obj.field("organization_floor_digest"),
            obj.take("organization_floor_digest")?,
        )?;
        let created_at = decode_instant(&obj.field("created_at"), obj.take("created_at")?)?;

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
        item.candidate_tree.object_format,
        item.candidate_tree.tree_oid.as_str(),
        item.finding_key,
        item.waiver_id.as_str(),
    )
}

/// The one restricted-JSON root every control document parses through.
///
/// # Errors
///
/// Any strict-JSON defect, carried as `ErrorKind::Json`.
pub fn root(bytes: &[u8]) -> Result<Value, Error> {
    json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))
}

fn decode_items<T>(
    path: &str,
    raw: Vec<Value>,
    limit: usize,
    decode: impl Fn(&str, Value) -> Result<T, Error>,
) -> Result<Vec<T>, Error> {
    if raw.len() > limit {
        return fail(path, ErrorKind::LimitExceeded);
    }
    raw.into_iter()
        .enumerate()
        .map(|(index, value)| decode(&format!("{path}[{index}]"), value))
        .collect()
}

fn sorted_set<T>(
    path: &str,
    items: &[T],
    compare: impl Fn(&T, &T) -> Ordering,
) -> Result<(), Error> {
    for pair in items.windows(2) {
        if let [left, right] = pair {
            match compare(left, right) {
                Ordering::Less => {}
                Ordering::Equal => return fail(path, ErrorKind::DuplicateMember),
                Ordering::Greater => return fail(path, ErrorKind::UnsortedSet),
            }
        }
    }
    Ok(())
}

fn decode_include(path: &str, value: Value) -> Result<DocumentInclude, Error> {
    let mut obj = Obj::new(path, value)?;
    let include_path = decode_repo_path(&obj.field("path"), obj.take("path")?)?;
    let kind = IncludeKind::decode(&obj.field("kind"), obj.take("kind")?)?;
    obj.finish()?;
    Ok(DocumentInclude {
        path: include_path,
        kind,
    })
}

fn decode_disposition_rule(path: &str, value: Value) -> Result<FindingDisposition, Error> {
    let mut obj = Obj::new(path, value)?;
    let finding_kind =
        PromotableFindingKind::decode(&obj.field("finding_kind"), obj.take("finding_kind")?)?;
    let disposition = Disposition::decode(&obj.field("disposition"), obj.take("disposition")?)?;
    obj.finish()?;
    Ok(FindingDisposition {
        finding_kind,
        disposition,
    })
}

fn decode_resource_limit(path: &str, value: Value) -> Result<ResourceLimit, Error> {
    let mut obj = Obj::new(path, value)?;
    let resource = ResourceName::decode(&obj.field("resource"), obj.take("resource")?)?;
    let maximum_path = obj.field("maximum");
    let maximum = de::integer(&maximum_path, obj.take("maximum")?)?;
    obj.finish()?;
    let in_bounds = if resource == ResourceName::TypedAnalysisErrorsRetained {
        (1..=64).contains(&maximum)
    } else if resource == ResourceName::MachineJsonBytes {
        maximum == 67_108_864
    } else {
        maximum >= 0
    };
    if in_bounds {
        Ok(ResourceLimit { resource, maximum })
    } else {
        fail(&maximum_path, ErrorKind::InvalidValue)
    }
}

fn decode_path_set(path: &str, value: Value) -> Result<Vec<RepoPathText>, Error> {
    decode_path_items(path, de::array(path, value)?)
}

fn decode_path_items(path: &str, raw: Vec<Value>) -> Result<Vec<RepoPathText>, Error> {
    let paths = decode_items(path, raw, 100_000, decode_repo_path)?;
    sorted_set(path, &paths, |a, b| a.as_str().cmp(b.as_str()))?;
    Ok(paths)
}

fn decode_owner_items(path: &str, raw: Vec<Value>) -> Result<Vec<OwnerId>, Error> {
    let owners = decode_items(path, raw, 10_000, decode_owner)?;
    sorted_set(path, &owners, |a, b| a.as_str().cmp(b.as_str()))?;
    Ok(owners)
}

fn decode_repo_path(path: &str, value: Value) -> Result<RepoPathText, Error> {
    RepoPathText::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_artifact_id(path: &str, value: Value) -> Result<ArtifactId, Error> {
    ArtifactId::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_owner(path: &str, value: Value) -> Result<OwnerId, Error> {
    OwnerId::new(de::string(path, value)?).ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_branch_ref(path: &str, value: Value) -> Result<BranchRef, Error> {
    BranchRef::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_instant(path: &str, value: Value) -> Result<UtcInstant, Error> {
    UtcInstant::new(de::string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_digest(path: &str, value: Value) -> Result<Digest, Error> {
    let raw = de::string(path, value)?;
    Digest::from_wire(&raw).ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_nullable_digest(path: &str, value: Value) -> Result<Option<Digest>, Error> {
    de::nullable(value)
        .map(|v| decode_digest(path, v))
        .transpose()
}

pub(crate) fn decode_repository(path: &str, value: Value) -> Result<RepositoryIdentity, Error> {
    let mut obj = Obj::new(path, value)?;
    de::const_str(&obj.field("host"), obj.take("host")?, "github.com")?;
    let owner = de::string(&obj.field("owner"), obj.take("owner")?)?;
    let name = de::string(&obj.field("name"), obj.take("name")?)?;
    obj.finish()?;
    RepositoryIdentity::github(owner, name).ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_tree(path: &str, value: Value) -> Result<TreeIdentity, Error> {
    let mut obj = Obj::new(path, value)?;
    let format_path = obj.field("object_format");
    let object_format = match de::string(&format_path, obj.take("object_format")?)?.as_str() {
        "sha1" => ObjectFormat::Sha1,
        "sha256" => ObjectFormat::Sha256,
        _ => return fail(&format_path, ErrorKind::InvalidValue),
    };
    let tree_oid = de::string(&obj.field("tree_oid"), obj.take("tree_oid")?)?;
    obj.finish()?;
    TreeIdentity::new(object_format, tree_oid)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

fn decode_reason(path: &str, value: Value) -> Result<String, Error> {
    let raw = de::string(path, value)?;
    let length = raw.chars().count();
    if (1..=1024).contains(&length) && raw.chars().any(|c| !c.is_whitespace()) {
        Ok(raw)
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}

fn decode_intent(path: &str, value: Value) -> Result<TargetIntent, Error> {
    let mut obj = Obj::new(path, value)?;
    de::const_str(&obj.field("kind"), obj.take("kind")?, "repository-path")?;
    let target_path = decode_repo_path(&obj.field("path"), obj.take("path")?)?;
    let target_kind = TargetKind::decode(&obj.field("target_kind"), obj.take("target_kind")?)?;
    let query_digest =
        decode_nullable_digest(&obj.field("query_digest"), obj.take("query_digest")?)?;
    let fragment_digest =
        decode_nullable_digest(&obj.field("fragment_digest"), obj.take("fragment_digest")?)?;
    obj.finish()?;
    Ok(TargetIntent {
        path: target_path,
        target_kind,
        query_digest,
        fragment_digest,
    })
}

fn decode_scope(path: &str, value: Value) -> Result<FindingScope, Error> {
    let mut obj = Obj::new(path, value)?;
    de::const_str(&obj.field("kind"), obj.take("kind")?, "reference")?;
    let document = decode_repo_path(&obj.field("document"), obj.take("document")?)?;
    let source_construct = SourceConstruct::decode(
        &obj.field("source_construct"),
        obj.take("source_construct")?,
    )?;
    let normalized_target_intent = decode_intent(
        &obj.field("normalized_target_intent"),
        obj.take("normalized_target_intent")?,
    )?;
    let occurrence_path = obj.field("occurrence");
    let mut occurrence = Obj::new(&occurrence_path, obj.take("occurrence")?)?;
    de::const_str(
        &occurrence.field("kind"),
        occurrence.take("kind")?,
        "source-projection",
    )?;
    let source_projection_digest = decode_digest(
        &occurrence.field("source_projection_digest"),
        occurrence.take("source_projection_digest")?,
    )?;
    occurrence.finish()?;
    obj.finish()?;
    Ok(FindingScope {
        document,
        source_construct,
        normalized_target_intent,
        source_projection_digest,
    })
}

fn decode_key_input(path: &str, value: Value) -> Result<(FindingKeyInput, Digest), Error> {
    let digest = hj(FINDING_KEY_DOMAIN, &value);
    let mut obj = Obj::new(path, value)?;
    de::const_str(
        &obj.field("schema"),
        obj.take("schema")?,
        FINDING_KEY_INPUT_SCHEMA,
    )?;
    let finding_kind =
        EligibleFindingKind::decode(&obj.field("finding_kind"), obj.take("finding_kind")?)?;
    let scope = decode_scope(&obj.field("scope"), obj.take("scope")?)?;
    obj.finish()?;
    Ok((
        FindingKeyInput {
            finding_kind,
            scope,
        },
        digest,
    ))
}

fn decode_resolution(path: &str, value: Value) -> Result<Resolution, Error> {
    let mut obj = Obj::new(path, value)?;
    let status_path = obj.field("status");
    let status = de::string(&status_path, obj.take("status")?)?;
    let code = de::string(&obj.field("code"), obj.take("code")?)?;
    let kind = match (status.as_str(), code.as_str()) {
        ("missing", "path-not-found") => ResolutionKind::Missing,
        ("type-mismatch", "target-type-mismatch") => ResolutionKind::TypeMismatch,
        _ => return fail(&status_path, ErrorKind::Inconsistent),
    };
    let res_path = de::nullable(obj.take("path")?)
        .map(|v| decode_repo_path(&obj.field("path"), v))
        .transpose()?;
    let entry_kind = de::nullable(obj.take("entry_kind")?)
        .map(|v| EntryKind::decode(&obj.field("entry_kind"), v))
        .transpose()?;
    let git_mode = de::nullable(obj.take("git_mode")?)
        .map(|v| GitMode::decode(&obj.field("git_mode"), v))
        .transpose()?;
    let raw_digest = decode_nullable_digest(&obj.field("raw_digest"), obj.take("raw_digest")?)?;
    let projection_digest = decode_nullable_digest(
        &obj.field("projection_digest"),
        obj.take("projection_digest")?,
    )?;
    let content_availability = ContentAvailability::decode(
        &obj.field("content_availability"),
        obj.take("content_availability")?,
    )?;
    obj.finish()?;

    let shape_ok = match kind {
        ResolutionKind::Missing => {
            entry_kind.is_none()
                && git_mode.is_none()
                && raw_digest.is_none()
                && projection_digest.is_none()
                && content_availability == ContentAvailability::NotApplicable
        }
        ResolutionKind::TypeMismatch => entry_kind.is_some() && git_mode.is_some(),
    };
    if !shape_ok {
        return fail(path, ErrorKind::Inconsistent);
    }
    Ok(Resolution {
        kind,
        path: res_path,
        entry_kind,
        git_mode,
        raw_digest,
        projection_digest,
        content_availability,
    })
}

fn decode_fact(path: &str, value: Value) -> Result<(Fact, Digest), Error> {
    let digest = hj(FACT_DOMAIN, &value);
    let mut obj = Obj::new(path, value)?;
    de::const_str(&obj.field("schema"), obj.take("schema")?, FACT_SCHEMA)?;
    let finding_kind =
        EligibleFindingKind::decode(&obj.field("finding_kind"), obj.take("finding_kind")?)?;
    let key_path = obj.field("key_input");
    let (key_input, _) = decode_key_input(&key_path, obj.take("key_input")?)?;
    let evidence_path = obj.field("evidence");
    let mut evidence = Obj::new(&evidence_path, obj.take("evidence")?)?;
    de::const_str(&evidence.field("kind"), evidence.take("kind")?, "reference")?;
    let resolution =
        decode_resolution(&evidence.field("resolution"), evidence.take("resolution")?)?;
    let multiplicity_path = evidence.field("occurrence_multiplicity");
    if de::integer(
        &multiplicity_path,
        evidence.take("occurrence_multiplicity")?,
    )? != 1
    {
        return fail(&multiplicity_path, ErrorKind::InvalidValue);
    }
    evidence.finish()?;
    obj.finish()?;

    let kind_matches = match resolution.kind {
        ResolutionKind::Missing => finding_kind == EligibleFindingKind::ExplicitTargetMissing,
        ResolutionKind::TypeMismatch => {
            finding_kind == EligibleFindingKind::ExplicitTargetTypeMismatch
        }
    };
    if !kind_matches || key_input.finding_kind != finding_kind {
        return fail(path, ErrorKind::Inconsistent);
    }
    Ok((
        Fact {
            finding_kind,
            key_input,
            resolution,
        },
        digest,
    ))
}

struct ItemCore {
    finding_kind: EligibleFindingKind,
    key_input: FindingKeyInput,
    finding_key: Digest,
    fact: Fact,
    fact_digest: Digest,
    owner: OwnerId,
    reason: String,
    created_at: UtcInstant,
    expires_at: UtcInstant,
}

fn decode_item_core(path: &str, obj: &mut Obj, fact_field: &str) -> Result<ItemCore, Error> {
    let finding_kind =
        EligibleFindingKind::decode(&obj.field("finding_kind"), obj.take("finding_kind")?)?;
    let key_path = obj.field("key_input");
    let (key_input, computed_key) = decode_key_input(&key_path, obj.take("key_input")?)?;
    let finding_key_path = obj.field("finding_key");
    let finding_key = decode_digest(&finding_key_path, obj.take("finding_key")?)?;
    if finding_key != computed_key {
        return fail(&finding_key_path, ErrorKind::DigestMismatch);
    }
    let fact_path = obj.field(fact_field);
    let (fact, computed_fact) = decode_fact(&fact_path, obj.take(fact_field)?)?;
    let fact_digest_field = format!("{fact_field}_digest");
    let fact_digest_path = obj.field(&fact_digest_field);
    let fact_digest = decode_digest(&fact_digest_path, obj.take(&fact_digest_field)?)?;
    if fact_digest != computed_fact {
        return fail(&fact_digest_path, ErrorKind::DigestMismatch);
    }
    if key_input.finding_kind != finding_kind || fact.key_input != key_input {
        return fail(path, ErrorKind::Inconsistent);
    }
    let owner = decode_owner(&obj.field("owner"), obj.take("owner")?)?;
    let reason = decode_reason(&obj.field("reason"), obj.take("reason")?)?;
    let created_at = decode_instant(&obj.field("created_at"), obj.take("created_at")?)?;
    let expires_at = decode_instant(&obj.field("expires_at"), obj.take("expires_at")?)?;
    Ok(ItemCore {
        finding_kind,
        key_input,
        finding_key,
        fact,
        fact_digest,
        owner,
        reason,
        created_at,
        expires_at,
    })
}

fn decode_debt_item(path: &str, value: Value) -> Result<DebtItem, Error> {
    let mut obj = Obj::new(path, value)?;
    let debt_id = decode_artifact_id(&obj.field("debt_id"), obj.take("debt_id")?)?;
    let core = decode_item_core(path, &mut obj, "accepted_fact")?;
    obj.finish()?;
    if core.created_at >= core.expires_at {
        return fail(path, ErrorKind::Inconsistent);
    }
    Ok(DebtItem {
        debt_id,
        finding_kind: core.finding_kind,
        key_input: core.key_input,
        finding_key: core.finding_key,
        accepted_fact: core.fact,
        accepted_fact_digest: core.fact_digest,
        owner: core.owner,
        reason: core.reason,
        created_at: core.created_at,
        expires_at: core.expires_at,
    })
}

fn decode_waiver_item(path: &str, value: Value) -> Result<WaiverItem, Error> {
    let mut obj = Obj::new(path, value)?;
    let waiver_id = decode_artifact_id(&obj.field("waiver_id"), obj.take("waiver_id")?)?;
    let core = decode_item_core(path, &mut obj, "authorized_fact")?;
    let candidate_tree = decode_tree(&obj.field("candidate_tree"), obj.take("candidate_tree")?)?;
    let issuer = decode_owner(&obj.field("issuer"), obj.take("issuer")?)?;
    let not_before = decode_instant(&obj.field("not_before"), obj.take("not_before")?)?;
    de::const_str(
        &obj.field("residual_disposition"),
        obj.take("residual_disposition")?,
        "warn",
    )?;
    obj.finish()?;
    if core.created_at > not_before || not_before >= core.expires_at {
        return fail(path, ErrorKind::Inconsistent);
    }
    Ok(WaiverItem {
        waiver_id,
        finding_kind: core.finding_kind,
        key_input: core.key_input,
        finding_key: core.finding_key,
        authorized_fact: core.fact,
        authorized_fact_digest: core.fact_digest,
        candidate_tree,
        owner: core.owner,
        issuer,
        reason: core.reason,
        created_at: core.created_at,
        not_before,
        expires_at: core.expires_at,
    })
}
