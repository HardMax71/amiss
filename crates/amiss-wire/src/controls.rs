use std::cmp::Ordering;
use std::collections::BTreeSet;

use strum::{AsRefStr, EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::json::{self, Value};
use crate::model::{
    Adapter, ArtifactId, BranchRef, ObjectFormat, OwnerId, RepoPathText, RepositoryIdentity,
    TreeIdentity, UtcInstant,
};
use crate::resolution::{
    BlobContent, BlobContentTag, BlobMode, BlobTarget, Missing, MissingTag, Resolution,
    ResolutionTag, Target, TargetTag,
};

/// Execution-constraint descriptor, forge-neutral action-repository
/// identity, and closed platform grammar.
mod execution_constraint;
/// Trusted-time statement grammar, digest, and bounded-lifetime parser.
mod trusted_time;
pub(crate) mod value;

pub use execution_constraint::{
    ConstraintPlatform, ExecutionConstraintDescriptor, ExecutionConstraintInput,
    valid_required_status_name,
};
pub use trusted_time::{STATEMENT_TTL_MAX_SECONDS, TrustedTimeInput, TrustedTimeStatement};

pub const SCANNER_POLICY_PATH: &str = ".amiss/scanner-policy.json";

const SCANNER_POLICY_SCHEMA: &str = "amiss/scanner-policy";
const ORGANIZATION_FLOOR_SCHEMA: &str = "amiss/organization-floor";
const DEBT_SNAPSHOT_SCHEMA: &str = "amiss/debt-snapshot";
const WAIVER_BUNDLE_SCHEMA: &str = "amiss/waiver-bundle";

const FINDING_KEY_INPUT_SCHEMA: &str = "amiss/scanner-finding-key-input";
const FACT_SCHEMA: &str = "amiss/scanner-fact";
pub const FINDING_KEY_DOMAIN: &str = "amiss/scanner-finding-key";
pub const FACT_DOMAIN: &str = "amiss/scanner-fact";

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
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }

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
    EnforceIntroduced,
    Enforce,
}

impl Profile {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::EnforceIntroduced => "enforce-introduced",
            Self::Enforce => "enforce",
        }
    }

    #[must_use]
    pub const fn enforces(self) -> bool {
        matches!(self, Self::EnforceIntroduced | Self::Enforce)
    }

    #[must_use]
    pub const fn introduced_only(self) -> bool {
        matches!(self, Self::EnforceIntroduced)
    }

    #[must_use]
    pub const fn policy_defaults(self) -> Self {
        match self {
            Self::Observe => Self::Observe,
            Self::EnforceIntroduced | Self::Enforce => Self::Enforce,
        }
    }

    /// # Errors
    ///
    /// A value outside the closed `observe`/`enforce-introduced`/`enforce`
    /// triple.
    pub fn decode(path: &str, value: Value) -> Result<Self, Error> {
        match de::string(path, value)?.as_str() {
            "observe" => Ok(Self::Observe),
            "enforce-introduced" => Ok(Self::EnforceIntroduced),
            "enforce" => Ok(Self::Enforce),
            _ => fail(path, ErrorKind::InvalidValue),
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AsRefStr,
    EnumIter,
    EnumString,
    IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum PromotableFindingKind {
    ExplicitTargetMissing,
    ExplicitTargetTypeMismatch,
    InvalidReference,
}

impl PromotableFindingKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.into()
    }

    fn decode(path: &str, value: Value) -> Result<Self, Error> {
        let raw = de::string(path, value)?;
        raw.parse()
            .map_err(|_unknown| Error::new(path, ErrorKind::InvalidValue))
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AsRefStr,
    EnumIter,
    EnumString,
    IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum EligibleFindingKind {
    ExplicitTargetMissing,
    ExplicitTargetTypeMismatch,
}

impl EligibleFindingKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.into()
    }

    fn decode(path: &str, value: Value) -> Result<Self, Error> {
        let raw = de::string(path, value)?;
        raw.parse()
            .map_err(|_unknown| Error::new(path, ErrorKind::InvalidValue))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, EnumIter)]
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
    AsciidocCrossReference,
    AsciidocInternalCrossReference,
    AsciidocLinkMacro,
    AsciidocBlockImage,
    AsciidocInlineImage,
    AsciidocInclude,
    RstInlineHyperlink,
    RstNamedTarget,
    RstImageDirective,
    RstIncludeDirective,
    RstFileOption,
    RstDocRole,
    RstRefRole,
    LinkReferenceDefinition,
    HtmlAnchor,
    HtmlImage,
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
            | Self::ShortcutReferenceImage
            | Self::AsciidocBlockImage
            | Self::AsciidocInlineImage
            | Self::RstImageDirective
            | Self::HtmlImage => true,
            Self::InlineLink
            | Self::FullReferenceLink
            | Self::CollapsedReferenceLink
            | Self::ShortcutReferenceLink
            | Self::Autolink
            | Self::AsciidocCrossReference
            | Self::AsciidocInternalCrossReference
            | Self::AsciidocLinkMacro
            | Self::AsciidocInclude
            | Self::RstInlineHyperlink
            | Self::RstNamedTarget
            | Self::RstIncludeDirective
            | Self::RstFileOption
            | Self::RstDocRole
            | Self::RstRefRole
            | Self::LinkReferenceDefinition
            | Self::HtmlAnchor => false,
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
            Self::AsciidocCrossReference => "asciidoc-xref-macro",
            Self::AsciidocInternalCrossReference => "asciidoc-internal-xref",
            Self::AsciidocLinkMacro => "asciidoc-link-macro",
            Self::AsciidocBlockImage => "asciidoc-block-image",
            Self::AsciidocInlineImage => "asciidoc-inline-image",
            Self::AsciidocInclude => "asciidoc-include",
            Self::RstInlineHyperlink => "rst-inline-hyperlink",
            Self::RstNamedTarget => "rst-named-target",
            Self::RstImageDirective => "rst-image-directive",
            Self::RstIncludeDirective => "rst-include-directive",
            Self::RstFileOption => "rst-file-option",
            Self::RstDocRole => "rst-doc-role",
            Self::RstRefRole => "rst-ref-role",
            Self::LinkReferenceDefinition => "markdown-link-reference-definition",
            Self::HtmlAnchor => "html-anchor",
            Self::HtmlImage => "html-image",
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
            "asciidoc-xref-macro" => Ok(Self::AsciidocCrossReference),
            "asciidoc-internal-xref" => Ok(Self::AsciidocInternalCrossReference),
            "asciidoc-link-macro" => Ok(Self::AsciidocLinkMacro),
            "asciidoc-block-image" => Ok(Self::AsciidocBlockImage),
            "asciidoc-inline-image" => Ok(Self::AsciidocInlineImage),
            "asciidoc-include" => Ok(Self::AsciidocInclude),
            "rst-inline-hyperlink" => Ok(Self::RstInlineHyperlink),
            "rst-named-target" => Ok(Self::RstNamedTarget),
            "rst-image-directive" => Ok(Self::RstImageDirective),
            "rst-include-directive" => Ok(Self::RstIncludeDirective),
            "rst-file-option" => Ok(Self::RstFileOption),
            "rst-doc-role" => Ok(Self::RstDocRole),
            "rst-ref-role" => Ok(Self::RstRefRole),
            "markdown-link-reference-definition" => Ok(Self::LinkReferenceDefinition),
            "html-anchor" => Ok(Self::HtmlAnchor),
            "html-image" => Ok(Self::HtmlImage),
            _ => fail(path, ErrorKind::InvalidValue),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, EnumIter)]
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, EnumIter)]
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter)]
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
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AsRefStr,
    EnumString,
    EnumIter,
    IntoStaticStr,
)]
#[strum(serialize_all = "kebab-case")]
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
    IgnoreDeclarationBlobBytes,
    AggregateIgnoreDeclarationBytesPerSnapshot,
    AggregateLineFragmentEvaluationBytesPerSnapshot,
    AggregateHeadingAnchorEvaluationBytesPerSnapshot,
    AggregateDocumentBytesPerSnapshot,
    RawLinkDestinationBytes,
    ParserNesting,
    ParserNodesPerDocument,
    ParserNodesPerSnapshot,
    AggregateEmbeddedCodeEvaluationBytesPerSnapshot,
    ReferencesPerDocument,
    ReferencesPerSnapshot,
    DeclaredLabelsPerSnapshot,
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
        Self::iter()
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
            | Self::AggregateEmbeddedCodeEvaluationBytesPerSnapshot
            | Self::ReferencesPerDocument
            | Self::ReferencesPerSnapshot
            | Self::DeclaredLabelsPerSnapshot => "parse",
            Self::ReferencedTargetBlobBytes
            | Self::AggregateReferencedTargetBytesPerSnapshot
            | Self::IgnoreDeclarationBlobBytes
            | Self::AggregateIgnoreDeclarationBytesPerSnapshot
            | Self::AggregateLineFragmentEvaluationBytesPerSnapshot
            | Self::AggregateHeadingAnchorEvaluationBytesPerSnapshot => "resolution",
            Self::CompleteFindings => "policy",
            Self::MachineJsonBytes => "output",
            Self::TypedAnalysisErrorsRetained
            | Self::PrivateTemporaryStorageBytes
            | Self::EvaluatorManagedMemoryBytes => "internal",
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        self.into()
    }

    fn decode(path: &str, value: Value) -> Result<Self, Error> {
        let raw = de::string(path, value)?;
        let Ok(resource) = raw.parse() else {
            return fail(path, ErrorKind::InvalidValue);
        };
        Ok(resource)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentInclude {
    pub path: RepoPathText,
    pub kind: IncludeKind,
    pub adapter: Option<Adapter>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingDisposition {
    pub finding_kind: PromotableFindingKind,
    pub disposition: Disposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScannerPolicy {
    digest: Digest,
    document_includes: Vec<DocumentInclude>,
    protected_inventory: Vec<RepoPathText>,
    finding_dispositions: Vec<FindingDisposition>,
}

impl ScannerPolicy {
    /// Builds a policy through the same ordering, uniqueness, and digest laws
    /// used for repository-controlled bytes.
    ///
    /// # Errors
    ///
    /// The supplied sets contain duplicates or otherwise fail the
    /// scanner-policy grammar.
    pub fn new(
        mut document_includes: Vec<DocumentInclude>,
        mut protected_inventory: Vec<RepoPathText>,
        mut finding_dispositions: Vec<FindingDisposition>,
    ) -> Result<Self, Error> {
        document_includes.sort_by(|left, right| {
            (left.path.as_str(), left.kind).cmp(&(right.path.as_str(), right.kind))
        });
        protected_inventory.sort();
        finding_dispositions
            .sort_by(|left, right| left.finding_kind.as_str().cmp(right.finding_kind.as_str()));
        let include_rows: Vec<Value> = document_includes
            .into_iter()
            .map(|include| {
                let mut rows = vec![
                    ("path".into(), Value::String(include.path.as_str().into())),
                    ("kind".into(), Value::String(include.kind.as_str().into())),
                ];
                if let Some(adapter) = include.adapter {
                    rows.push(("adapter".into(), Value::String(adapter.adapter_id().into())));
                }
                Value::Object(rows.into_boxed_slice())
            })
            .collect();
        let inventory: Vec<Value> = protected_inventory
            .into_iter()
            .map(|path| Value::String(path.as_str().into()))
            .collect();
        let dispositions: Vec<Value> = finding_dispositions
            .into_iter()
            .map(|row| {
                Value::Object(Box::new([
                    (
                        "finding_kind".into(),
                        Value::String(row.finding_kind.as_str().into()),
                    ),
                    (
                        "disposition".into(),
                        Value::String(row.disposition.as_str().into()),
                    ),
                ]))
            })
            .collect();
        let value = Value::Object(Box::new([
            ("schema".into(), Value::String(SCANNER_POLICY_SCHEMA.into())),
            (
                "document_includes".into(),
                Value::Array(include_rows.into_boxed_slice()),
            ),
            (
                "protected_inventory".into(),
                Value::Array(inventory.into_boxed_slice()),
            ),
            (
                "finding_dispositions".into(),
                Value::Array(dispositions.into_boxed_slice()),
            ),
        ]));
        Self::parse(&json::canonical(&value))
    }

    #[must_use]
    pub const fn digest(&self) -> Digest {
        self.digest
    }

    #[must_use]
    pub fn document_includes(&self) -> &[DocumentInclude] {
        &self.document_includes
    }

    #[must_use]
    pub fn protected_inventory(&self) -> &[RepoPathText] {
        &self.protected_inventory
    }

    #[must_use]
    pub fn finding_dispositions(&self) -> &[FindingDisposition] {
        &self.finding_dispositions
    }

    /// # Errors
    ///
    /// Fails on strict-JSON defects, schema-shape violations, unknown fields,
    /// invalid grammar values, and unsorted or duplicate set members.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let value = root(bytes)?;
        let digest = hj(SCANNER_POLICY_SCHEMA, &value);
        let mut obj = Obj::new("$", value)?;
        obj.required("schema", |path, value| {
            de::const_str(path, value, SCANNER_POLICY_SCHEMA)
        })?;

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
pub struct Fact {
    key_input: FindingKeyInput,
    resolution: Resolution<RepoPathText>,
}

impl Fact {
    /// Builds a structural fact only when the key kind and resolution family agree.
    /// Non-structural resolution families are not eligible for control items.
    #[must_use]
    pub fn new(key_input: FindingKeyInput, resolution: Resolution<RepoPathText>) -> Option<Self> {
        let expected = match &resolution {
            Resolution::Missing(_) => EligibleFindingKind::ExplicitTargetMissing,
            Resolution::TypeMismatch(_) => EligibleFindingKind::ExplicitTargetTypeMismatch,
            Resolution::Resolved(_)
            | Resolution::DeclaredUntracked(_)
            | Resolution::UnsupportedTarget(_)
            | Resolution::UnsupportedSemantics(_)
            | Resolution::UnsupportedVersion(_)
            | Resolution::Invalid(_)
            | Resolution::External(_) => return None,
        };
        (key_input.finding_kind == expected).then_some(Self {
            key_input,
            resolution,
        })
    }

    /// The finding kind fixed by the validated key and resolution family.
    #[must_use]
    pub const fn finding_kind(&self) -> EligibleFindingKind {
        self.key_input.finding_kind
    }

    /// The canonical finding-key preimage embedded in this fact.
    #[must_use]
    pub const fn key_input(&self) -> &FindingKeyInput {
        &self.key_input
    }

    /// The structural resolution evidence embedded in this fact.
    #[must_use]
    pub const fn resolution(&self) -> &Resolution<RepoPathText> {
        &self.resolution
    }
}

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
        let value = root(bytes)?;
        let digest = hj(DEBT_SNAPSHOT_SCHEMA, &value);
        let mut obj = Obj::new("$", value)?;
        obj.required("schema", |path, value| {
            de::const_str(path, value, DEBT_SNAPSHOT_SCHEMA)
        })?;

        let repository = obj.required("repository", decode_repository)?;
        let ref_name = obj.required("ref", decode_branch_ref)?;
        let organization_floor_digest = obj.required("organization_floor_digest", decode_digest)?;
        let adoption_tree = obj.required("adoption_tree", decode_tree)?;
        let adoption_report_payload_digest =
            obj.required("adoption_report_payload_digest", decode_digest)?;
        let created_at = obj.required("created_at", decode_instant)?;

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
        let value = root(bytes)?;
        let digest = hj(WAIVER_BUNDLE_SCHEMA, &value);
        let mut obj = Obj::new("$", value)?;
        obj.required("schema", |path, value| {
            de::const_str(path, value, WAIVER_BUNDLE_SCHEMA)
        })?;

        let repository = obj.required("repository", decode_repository)?;
        let ref_name = obj.required("ref", decode_branch_ref)?;
        let organization_floor_digest = obj.required("organization_floor_digest", decode_digest)?;
        let created_at = obj.required("created_at", decode_instant)?;

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
        item.candidate_tree.object_format(),
        item.candidate_tree.tree_oid(),
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
    let include_path = obj.required("path", decode_repo_path)?;
    let kind = obj.required("kind", IncludeKind::decode)?;
    let adapter = obj
        .take_optional("adapter")
        .map(|value| decode_adapter(&obj.field("adapter"), value))
        .transpose()?;
    obj.finish()?;
    Ok(DocumentInclude {
        path: include_path,
        kind,
        adapter,
    })
}

fn decode_adapter(path: &str, value: Value) -> Result<Adapter, Error> {
    let name = de::string(path, value)?;
    match Adapter::all().find(|adapter| adapter.adapter_id() == name) {
        Some(adapter) => Ok(adapter),
        None => fail(path, ErrorKind::InvalidValue),
    }
}

fn decode_disposition_rule(path: &str, value: Value) -> Result<FindingDisposition, Error> {
    let mut obj = Obj::new(path, value)?;
    let finding_kind = obj.required("finding_kind", PromotableFindingKind::decode)?;
    let disposition = obj.required("disposition", Disposition::decode)?;
    obj.finish()?;
    Ok(FindingDisposition {
        finding_kind,
        disposition,
    })
}

fn decode_resource_limit(path: &str, value: Value) -> Result<ResourceLimit, Error> {
    let mut obj = Obj::new(path, value)?;
    let resource = obj.required("resource", ResourceName::decode)?;
    let maximum_path = obj.field("maximum");
    let maximum = de::integer(&maximum_path, obj.take("maximum")?)?;
    obj.finish()?;
    if in_bounds(resource, maximum) {
        Ok(ResourceLimit { resource, maximum })
    } else {
        fail(&maximum_path, ErrorKind::InvalidValue)
    }
}

/// Two resources fix their own maximum: the retained-error count is a small
/// range, and the report reservation may be declared but never moved.
fn in_bounds(resource: ResourceName, maximum: i64) -> bool {
    if resource == ResourceName::TypedAnalysisErrorsRetained {
        (1..=64).contains(&maximum)
    } else if resource == ResourceName::MachineJsonBytes {
        u64::try_from(maximum).is_ok_and(|value| value == crate::report::MACHINE_JSON_BYTES)
    } else {
        maximum >= 0
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
    let host = obj.required("host", de::string)?;
    let owner = obj.required("owner", de::string)?;
    let name = obj.required("name", de::string)?;
    obj.finish()?;
    RepositoryIdentity::new(host, owner, name)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

pub(crate) fn decode_provider_run_id(path: &str, value: Value) -> Result<String, Error> {
    let raw = de::string(path, value)?;
    let bytes = raw.as_bytes();
    let allowed = |byte: &u8| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-')
    };
    if bytes.is_empty()
        || bytes.len() > 128
        || !bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        || !bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        || !bytes.iter().all(allowed)
    {
        return fail(path, ErrorKind::InvalidValue);
    }
    Ok(raw)
}

pub(crate) fn decode_provider_id(path: &str, value: Value) -> Result<String, Error> {
    let raw = de::string(path, value)?;
    if ArtifactId::new(raw.clone()).is_some() {
        Ok(raw)
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}

fn decode_tree(path: &str, value: Value) -> Result<TreeIdentity, Error> {
    let mut obj = Obj::new(path, value)?;
    let format_path = obj.field("object_format");
    let object_format = match de::string(&format_path, obj.take("object_format")?)?.as_str() {
        "sha1" => ObjectFormat::Sha1,
        "sha256" => ObjectFormat::Sha256,
        _ => return fail(&format_path, ErrorKind::InvalidValue),
    };
    let tree_oid = obj.required("tree_oid", de::string)?;
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
    obj.required("kind", |path, value| {
        de::const_str(path, value, "repository-path")
    })?;
    let target_path = obj.required("path", decode_repo_path)?;
    let target_kind = obj.required("target_kind", TargetKind::decode)?;
    let query_digest = obj.required("query_digest", decode_nullable_digest)?;
    let fragment_digest = obj.required("fragment_digest", decode_nullable_digest)?;
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
    obj.required("kind", |path, value| {
        de::const_str(path, value, "reference")
    })?;
    let document = obj.required("document", decode_repo_path)?;
    let source_construct = obj.required("source_construct", SourceConstruct::decode)?;
    let normalized_target_intent = obj.required("normalized_target_intent", decode_intent)?;
    let occurrence_path = obj.field("occurrence");
    let mut occurrence = Obj::new(&occurrence_path, obj.take("occurrence")?)?;
    occurrence.required("kind", |path, value| {
        de::const_str(path, value, "source-projection")
    })?;
    let source_projection_digest =
        occurrence.required("source_projection_digest", decode_digest)?;
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
    obj.required("schema", |path, value| {
        de::const_str(path, value, FINDING_KEY_INPUT_SCHEMA)
    })?;
    let finding_kind = obj.required("finding_kind", EligibleFindingKind::decode)?;
    let scope = obj.required("scope", decode_scope)?;
    obj.finish()?;
    Ok((
        FindingKeyInput {
            finding_kind,
            scope,
        },
        digest,
    ))
}

fn decode_resolution(path: &str, value: Value) -> Result<Resolution<RepoPathText>, Error> {
    let mut obj = Obj::new(path, value)?;
    let kind_path = obj.field("kind");
    let kind_text = de::string(&kind_path, obj.take("kind")?)?;
    let Ok(kind) = kind_text.parse::<ResolutionTag>() else {
        return fail(&kind_path, ErrorKind::InvalidValue);
    };
    match kind {
        ResolutionTag::Missing => {
            let reason_path = obj.field("reason");
            let reason_text = de::string(&reason_path, obj.take("reason")?)?;
            let Ok(reason) = reason_text.parse::<MissingTag>() else {
                return fail(&reason_path, ErrorKind::InvalidValue);
            };
            if matches!(reason, MissingTag::LabelNotDeclared) {
                obj.finish()?;
                return Ok(Resolution::Missing(Missing::LabelNotDeclared));
            }
            let resolved_path = obj.required("path", decode_repo_path)?;
            let missing = match reason {
                MissingTag::PathNotFound => Missing::PathNotFound {
                    path: resolved_path,
                    near: obj.required("near", |path, value| {
                        de::nullable(value)
                            .map(|value| decode_repo_path(path, value))
                            .transpose()
                    })?,
                },
                MissingTag::LineFragmentOutOfRange => Missing::LineFragmentOutOfRange {
                    path: resolved_path,
                },
                MissingTag::HeadingAnchorNotFound => Missing::HeadingAnchorNotFound {
                    path: resolved_path,
                    near: obj.required("near", |path, value| {
                        de::nullable(value)
                            .map(|value| de::string(path, value))
                            .transpose()
                    })?,
                },
                MissingTag::LabelNotDeclared => Missing::LabelNotDeclared,
            };
            obj.finish()?;
            Ok(Resolution::Missing(missing))
        }
        ResolutionTag::TypeMismatch => {
            let target = obj.required("target", decode_resolution_target)?;
            obj.finish()?;
            Ok(Resolution::TypeMismatch(target))
        }
        ResolutionTag::Resolved
        | ResolutionTag::DeclaredUntracked
        | ResolutionTag::UnsupportedTarget
        | ResolutionTag::UnsupportedSemantics
        | ResolutionTag::UnsupportedVersion
        | ResolutionTag::Invalid
        | ResolutionTag::External => fail(&kind_path, ErrorKind::InvalidValue),
    }
}

fn decode_resolution_target(path: &str, value: Value) -> Result<Target<RepoPathText>, Error> {
    let mut obj = Obj::new(path, value)?;
    let kind_path = obj.field("kind");
    let kind_text = de::string(&kind_path, obj.take("kind")?)?;
    let Ok(kind) = kind_text.parse::<TargetTag>() else {
        return fail(&kind_path, ErrorKind::InvalidValue);
    };
    let resolved_path = obj.required("path", decode_repo_path)?;
    match kind {
        TargetTag::Tree => {
            obj.finish()?;
            Ok(Target::Tree {
                path: resolved_path,
            })
        }
        TargetTag::Blob => {
            let mode_path = obj.field("mode");
            let mode_text = de::string(&mode_path, obj.take("mode")?)?;
            let Ok(mode) = mode_text.parse::<BlobMode>() else {
                return fail(&mode_path, ErrorKind::InvalidValue);
            };
            let content = obj.required("content", decode_resolution_content)?;
            obj.finish()?;
            Ok(Target::Blob(BlobTarget {
                path: resolved_path,
                mode,
                content,
            }))
        }
    }
}

fn decode_resolution_content(path: &str, value: Value) -> Result<BlobContent, Error> {
    let mut obj = Obj::new(path, value)?;
    let kind_path = obj.field("kind");
    let kind_text = de::string(&kind_path, obj.take("kind")?)?;
    let Ok(kind) = kind_text.parse::<BlobContentTag>() else {
        return fail(&kind_path, ErrorKind::InvalidValue);
    };
    let raw_digest = obj.required("raw_digest", decode_digest)?;
    match kind {
        BlobContentTag::Available => {
            let projection_digest = obj.required("projection_digest", decode_digest)?;
            obj.finish()?;
            Ok(BlobContent::Available {
                raw_digest,
                projection_digest,
            })
        }
        BlobContentTag::LfsPointer => {
            obj.finish()?;
            Ok(BlobContent::LfsPointer { raw_digest })
        }
    }
}

struct DecodedFact {
    fact: Fact,
    fact_digest: Digest,
    finding_key: Digest,
}

fn decode_fact(path: &str, value: Value) -> Result<DecodedFact, Error> {
    let fact_digest = hj(FACT_DOMAIN, &value);
    let mut obj = Obj::new(path, value)?;
    obj.required("schema", |path, value| {
        de::const_str(path, value, FACT_SCHEMA)
    })?;
    let finding_kind = obj.required("finding_kind", EligibleFindingKind::decode)?;
    let key_path = obj.field("key_input");
    let (key_input, finding_key) = decode_key_input(&key_path, obj.take("key_input")?)?;
    let evidence_path = obj.field("evidence");
    let mut evidence = Obj::new(&evidence_path, obj.take("evidence")?)?;
    evidence.required("kind", |path, value| {
        de::const_str(path, value, "reference")
    })?;
    let resolution = evidence.required("resolution", decode_resolution)?;
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

    let Some(fact) = Fact::new(key_input, resolution) else {
        return fail(path, ErrorKind::Inconsistent);
    };
    if fact.finding_kind() != finding_kind {
        return fail(path, ErrorKind::Inconsistent);
    }
    Ok(DecodedFact {
        fact,
        fact_digest,
        finding_key,
    })
}

struct ItemCore {
    finding_key: Digest,
    fact: Fact,
    fact_digest: Digest,
    owner: OwnerId,
    reason: String,
    created_at: UtcInstant,
    expires_at: UtcInstant,
}

fn decode_item_core(obj: &mut Obj, fact_field: &str) -> Result<ItemCore, Error> {
    let finding_key_path = obj.field("finding_key");
    let finding_key = decode_digest(&finding_key_path, obj.take("finding_key")?)?;
    let fact_path = obj.field(fact_field);
    let decoded_fact = decode_fact(&fact_path, obj.take(fact_field)?)?;
    if finding_key != decoded_fact.finding_key {
        return fail(&finding_key_path, ErrorKind::DigestMismatch);
    }
    let fact_digest_field = format!("{fact_field}_digest");
    let fact_digest_path = obj.field(&fact_digest_field);
    let fact_digest = decode_digest(&fact_digest_path, obj.take(&fact_digest_field)?)?;
    if fact_digest != decoded_fact.fact_digest {
        return fail(&fact_digest_path, ErrorKind::DigestMismatch);
    }
    let owner = obj.required("owner", decode_owner)?;
    let reason = obj.required("reason", decode_reason)?;
    let created_at = obj.required("created_at", decode_instant)?;
    let expires_at = obj.required("expires_at", decode_instant)?;
    Ok(ItemCore {
        finding_key,
        fact: decoded_fact.fact,
        fact_digest,
        owner,
        reason,
        created_at,
        expires_at,
    })
}

fn decode_debt_item(path: &str, value: Value) -> Result<DebtItem, Error> {
    let mut obj = Obj::new(path, value)?;
    let debt_id = obj.required("debt_id", decode_artifact_id)?;
    let core = decode_item_core(&mut obj, "accepted_fact")?;
    obj.finish()?;
    if core.created_at >= core.expires_at {
        return fail(path, ErrorKind::Inconsistent);
    }
    Ok(DebtItem {
        debt_id,
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
    let waiver_id = obj.required("waiver_id", decode_artifact_id)?;
    let core = decode_item_core(&mut obj, "authorized_fact")?;
    let candidate_tree = obj.required("candidate_tree", decode_tree)?;
    let issuer = obj.required("issuer", decode_owner)?;
    let not_before = obj.required("not_before", decode_instant)?;
    obj.required("residual_disposition", |path, value| {
        de::const_str(path, value, "warn")
    })?;
    obj.finish()?;
    if core.created_at > not_before || not_before >= core.expires_at {
        return fail(path, ErrorKind::Inconsistent);
    }
    Ok(WaiverItem {
        waiver_id,
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
