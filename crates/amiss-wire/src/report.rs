use std::collections::BTreeSet;

use crate::digest::{Digest, hj};
use crate::json::{Scratch, Sink, Value, canonical, canonical_length};
use crate::model::Adapter;

pub const ENGINE_CONTRACT: &str = "amiss/scanner";

/// The exact `machine-json-bytes` reservation: the report wire, canonical
/// envelope plus the trailing newline, never exceeds this.
pub const MACHINE_JSON_BYTES: u64 = 67_108_864;

/// The evaluator-managed memory ceiling asserted by the sandbox descriptor.
pub const EVALUATOR_MANAGED_MEMORY_BYTES: u64 = 1_073_741_824;

/// The private temporary-storage ceiling asserted by the sandbox descriptor.
pub const PRIVATE_TEMPORARY_STORAGE_BYTES: u64 = 67_108_864;

/// The watchdog ceiling asserted by the sandbox descriptor.
pub const WATCHDOG_MILLISECONDS: u64 = 120_000;

/// The fatal serializer's fixed scratch allowance: the staging buffer it
/// reserves up front plus every transient allocation one streaming emission
/// may make. The E0 maximal golden proves emission stays inside it.
pub const FATAL_SCRATCH_BYTES: usize = 65_536;

/// The streaming fatal-envelope serializer and its fixed scratch space. A
/// binary reserves one before evaluator allocation accounting begins, so a
/// fatal projection is always emittable: emission streams `JCS(envelope)`
/// and the trailing newline through the reserved staging buffer without
/// materializing the wire.
pub struct FatalSerializer {
    staging: Vec<u8>,
    scratch: Scratch,
}

impl FatalSerializer {
    /// Reserves the staging buffer and serializer scratch.
    #[must_use]
    pub fn new() -> Self {
        Self {
            staging: Vec::with_capacity(FATAL_SCRATCH_BYTES),
            scratch: Scratch::new(),
        }
    }

    /// Streams the envelope's wire (`JCS(envelope) || LF`) into the writer
    /// through the reserved scratch and returns the byte count.
    ///
    /// # Errors
    ///
    /// The first writer error; the wire is incomplete in that case and the
    /// caller treats the emission as failed.
    pub fn emit(&mut self, envelope: &Value, out: &mut dyn std::io::Write) -> std::io::Result<u64> {
        self.staging.clear();
        let mut sink = StagedSink {
            staging: &mut self.staging,
            out,
            written: 0,
            error: None,
        };
        self.scratch.stream(envelope, &mut sink);
        sink.write("\n");
        let written = sink.flush();
        self.staging.clear();
        written
    }

    /// The materialized wire for a caller that must inspect the bytes (the
    /// wrapper's acceptance): one counting pass sizes the allocation
    /// exactly, then one streaming pass fills it.
    #[must_use]
    pub fn wire_bytes(&mut self, envelope: &Value) -> Vec<u8> {
        let exact = canonical_length(envelope).saturating_add(1);
        let mut wire = Vec::with_capacity(usize::try_from(exact).unwrap_or(0));
        if self.emit(envelope, &mut wire).is_err() {
            wire.clear();
        }
        wire
    }
}

impl Default for FatalSerializer {
    fn default() -> Self {
        Self::new()
    }
}

struct StagedSink<'a> {
    staging: &'a mut Vec<u8>,
    out: &'a mut dyn std::io::Write,
    written: u64,
    error: Option<std::io::Error>,
}

impl StagedSink<'_> {
    fn drain(&mut self) {
        if self.error.is_none() {
            match self.out.write_all(self.staging) {
                Ok(()) => {
                    self.written = self
                        .written
                        .saturating_add(u64::try_from(self.staging.len()).unwrap_or(u64::MAX));
                }
                Err(defect) => self.error = Some(defect),
            }
        }
        self.staging.clear();
    }

    fn flush(&mut self) -> std::io::Result<u64> {
        self.drain();
        match self.error.take() {
            Some(defect) => Err(defect),
            None => Ok(self.written),
        }
    }
}

impl Sink for StagedSink<'_> {
    fn write(&mut self, piece: &str) {
        if piece.len() >= FATAL_SCRATCH_BYTES {
            self.drain();
            if self.error.is_none() {
                match self.out.write_all(piece.as_bytes()) {
                    Ok(()) => {
                        self.written = self
                            .written
                            .saturating_add(u64::try_from(piece.len()).unwrap_or(u64::MAX));
                    }
                    Err(defect) => self.error = Some(defect),
                }
            }
            return;
        }
        if self.staging.len().saturating_add(piece.len()) > FATAL_SCRATCH_BYTES {
            self.drain();
        }
        self.staging.extend_from_slice(piece.as_bytes());
    }
}
pub const ENGINE_DOMAIN: &str = "amiss/scanner-engine";
pub const ENVELOPE_SCHEMA: &str = "amiss/scanner-report-envelope";
pub const PAYLOAD_SCHEMA: &str = "amiss/scanner-report-payload";
pub const ADAPTER_CONTRACT_SCHEMA: &str = "amiss/scanner-adapter-contract";
pub const BUILT_IN_POLICY: &str = "scanner-policy-defaults";

/// The closed analysis-error codes in schema declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnalysisErrorCode {
    InvalidInvocation,
    InvalidEvent,
    InvalidProfile,
    RequestUnreadable,
    ConfigurationInvalid,
    DuplicateJsonKey,
    InvalidUtf8,
    InvalidJson,
    UnknownSchema,
    UnknownField,
    NoncanonicalArray,
    DigestMismatch,
    ControlBindingMismatch,
    ExceptionOverlap,
    UnsupportedCapability,
    GitRepositoryUnavailable,
    GitObjectMissing,
    GitObjectWrongKind,
    GitObjectUnreadable,
    GitIndexInvalid,
    GitIndexUnmerged,
    GitIntentToAdd,
    GitSnapshotChanged,
    UnrepresentablePath,
    DocumentInvalid,
    ParserError,
    ParserPanic,
    InvalidSourceSpan,
    ResolutionError,
    ResourceLimitExceeded,
    OutputLimitExceeded,
    TooManyErrors,
    ReportConstructionFailed,
    SandboxViolation,
    TrustedTimeInvalid,
    InternalError,
}

impl AnalysisErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInvocation => "INVALID_INVOCATION",
            Self::InvalidEvent => "INVALID_EVENT",
            Self::InvalidProfile => "INVALID_PROFILE",
            Self::RequestUnreadable => "REQUEST_UNREADABLE",
            Self::ConfigurationInvalid => "CONFIGURATION_INVALID",
            Self::DuplicateJsonKey => "DUPLICATE_JSON_KEY",
            Self::InvalidUtf8 => "INVALID_UTF8",
            Self::InvalidJson => "INVALID_JSON",
            Self::UnknownSchema => "UNKNOWN_SCHEMA",
            Self::UnknownField => "UNKNOWN_FIELD",
            Self::NoncanonicalArray => "NONCANONICAL_ARRAY",
            Self::DigestMismatch => "DIGEST_MISMATCH",
            Self::ControlBindingMismatch => "CONTROL_BINDING_MISMATCH",
            Self::ExceptionOverlap => "EXCEPTION_OVERLAP",
            Self::UnsupportedCapability => "UNSUPPORTED_CAPABILITY",
            Self::GitRepositoryUnavailable => "GIT_REPOSITORY_UNAVAILABLE",
            Self::GitObjectMissing => "GIT_OBJECT_MISSING",
            Self::GitObjectWrongKind => "GIT_OBJECT_WRONG_KIND",
            Self::GitObjectUnreadable => "GIT_OBJECT_UNREADABLE",
            Self::GitIndexInvalid => "GIT_INDEX_INVALID",
            Self::GitIndexUnmerged => "GIT_INDEX_UNMERGED",
            Self::GitIntentToAdd => "GIT_INTENT_TO_ADD",
            Self::GitSnapshotChanged => "GIT_SNAPSHOT_CHANGED",
            Self::UnrepresentablePath => "UNREPRESENTABLE_PATH",
            Self::DocumentInvalid => "DOCUMENT_INVALID",
            Self::ParserError => "PARSER_ERROR",
            Self::ParserPanic => "PARSER_PANIC",
            Self::InvalidSourceSpan => "INVALID_SOURCE_SPAN",
            Self::ResolutionError => "RESOLUTION_ERROR",
            Self::ResourceLimitExceeded => "RESOURCE_LIMIT_EXCEEDED",
            Self::OutputLimitExceeded => "OUTPUT_LIMIT_EXCEEDED",
            Self::TooManyErrors => "TOO_MANY_ERRORS",
            Self::ReportConstructionFailed => "REPORT_CONSTRUCTION_FAILED",
            Self::SandboxViolation => "SANDBOX_VIOLATION",
            Self::TrustedTimeInvalid => "TRUSTED_TIME_INVALID",
            Self::InternalError => "INTERNAL_ERROR",
        }
    }

    /// Fixed phase for non-resource codes; `RESOURCE_LIMIT_EXCEEDED` takes its
    /// phase from the resource partition and has none here.
    #[must_use]
    pub const fn fixed_phase(self) -> Option<&'static str> {
        match self {
            Self::InvalidInvocation
            | Self::InvalidEvent
            | Self::InvalidProfile
            | Self::RequestUnreadable => Some("invocation"),
            Self::ConfigurationInvalid
            | Self::DuplicateJsonKey
            | Self::InvalidUtf8
            | Self::InvalidJson
            | Self::UnknownSchema
            | Self::UnknownField
            | Self::NoncanonicalArray
            | Self::DigestMismatch
            | Self::ControlBindingMismatch
            | Self::ExceptionOverlap
            | Self::TrustedTimeInvalid => Some("configuration"),
            Self::GitRepositoryUnavailable
            | Self::GitObjectMissing
            | Self::GitObjectWrongKind
            | Self::GitObjectUnreadable
            | Self::GitIndexInvalid
            | Self::GitIndexUnmerged
            | Self::GitIntentToAdd
            | Self::GitSnapshotChanged
            | Self::UnrepresentablePath => Some("git"),
            Self::DocumentInvalid
            | Self::ParserError
            | Self::ParserPanic
            | Self::InvalidSourceSpan => Some("parse"),
            Self::ResolutionError => Some("resolution"),
            Self::UnsupportedCapability => Some("policy"),
            Self::OutputLimitExceeded | Self::ReportConstructionFailed => Some("output"),
            Self::SandboxViolation | Self::TooManyErrors | Self::InternalError => Some("internal"),
            Self::ResourceLimitExceeded => None,
        }
    }

    const fn evaluation_reason(self) -> Option<&'static str> {
        match self {
            Self::InvalidInvocation => Some("invalid-invocation"),
            Self::InvalidEvent => Some("invalid-event"),
            Self::InvalidProfile => Some("invalid-profile"),
            Self::RequestUnreadable => Some("request-unreadable"),
            Self::ConfigurationInvalid
            | Self::DuplicateJsonKey
            | Self::InvalidUtf8
            | Self::InvalidJson
            | Self::UnknownSchema
            | Self::UnknownField
            | Self::NoncanonicalArray
            | Self::DigestMismatch
            | Self::ControlBindingMismatch
            | Self::ExceptionOverlap
            | Self::UnsupportedCapability
            | Self::GitRepositoryUnavailable
            | Self::GitObjectMissing
            | Self::GitObjectWrongKind
            | Self::GitObjectUnreadable
            | Self::GitIndexInvalid
            | Self::GitIndexUnmerged
            | Self::GitIntentToAdd
            | Self::GitSnapshotChanged
            | Self::UnrepresentablePath
            | Self::DocumentInvalid
            | Self::ParserError
            | Self::ParserPanic
            | Self::InvalidSourceSpan
            | Self::ResolutionError
            | Self::ResourceLimitExceeded
            | Self::OutputLimitExceeded
            | Self::TooManyErrors
            | Self::ReportConstructionFailed
            | Self::SandboxViolation
            | Self::TrustedTimeInvalid
            | Self::InternalError => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineProvenance {
    pub version: String,
    pub digest: Digest,
}

/// Builds the canonical fatal-incomplete wire (`JCS(envelope) || LF`) for an
/// invocation rejection: every detail array empty, every count zero, unavailable
/// evaluation and controls with their reason sets, exit class 2.
///
/// Returns `None` when `codes` is empty or contains a non-invocation code.
#[must_use]
pub fn invocation_failure_wire(
    engine: &EngineProvenance,
    codes: &BTreeSet<AnalysisErrorCode>,
) -> Option<Vec<u8>> {
    unavailable_evaluation_wire(engine, codes, None, None)
}

/// The envelope value behind [`invocation_failure_wire`], for emission
/// through the reserved fatal serializer.
#[must_use]
pub fn invocation_failure_envelope(
    engine: &EngineProvenance,
    codes: &BTreeSet<AnalysisErrorCode>,
) -> Option<Value> {
    unavailable_evaluation_envelope(engine, codes, None, None)
}

/// The fatal unavailable-evaluation envelope for the request-wire lane: the
/// same closed projection, carrying each request's diagnostic digest where
/// its byte stream was completely captured.
///
/// Returns `None` when no code is supplied or a code has no evaluation
/// reason, exactly as the invocation form.
#[must_use]
pub fn unavailable_evaluation_wire(
    engine: &EngineProvenance,
    codes: &BTreeSet<AnalysisErrorCode>,
    evaluation_request_digest: Option<Digest>,
    controls_request_digest: Option<Digest>,
) -> Option<Vec<u8>> {
    let envelope = unavailable_evaluation_envelope(
        engine,
        codes,
        evaluation_request_digest,
        controls_request_digest,
    )?;
    let mut wire = canonical(&envelope);
    wire.push(b'\n');
    Some(wire)
}

/// The envelope value behind [`unavailable_evaluation_wire`], for emission
/// through the reserved fatal serializer.
#[must_use]
pub fn unavailable_evaluation_envelope(
    engine: &EngineProvenance,
    codes: &BTreeSet<AnalysisErrorCode>,
    evaluation_request_digest: Option<Digest>,
    controls_request_digest: Option<Digest>,
) -> Option<Value> {
    if codes.is_empty() {
        return None;
    }
    let mut reasons = Vec::new();
    for code in codes {
        reasons.push(Value::String(code.evaluation_reason()?.to_owned()));
    }

    let mut errors: Vec<(AnalysisErrorCode, &'static str)> = Vec::new();
    for code in codes {
        errors.push((*code, code.fixed_phase()?));
    }
    errors.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    let error_rows: Vec<Value> = errors
        .iter()
        .map(|(code, phase)| {
            error_row(
                &ErrorDetail {
                    code: *code,
                    path: None,
                    path_bytes: None,
                    resource: None,
                },
                phase,
            )
        })
        .collect();
    let error_count = i64::try_from(error_rows.len()).ok()?;

    let payload = object(vec![
        ("schema", string(PAYLOAD_SCHEMA)),
        ("compatibility", string("experimental")),
        ("engine", engine_block(engine)),
        (
            "evaluation",
            object(vec![
                ("status", string("unavailable")),
                (
                    "request_digest",
                    evaluation_request_digest
                        .map_or(Value::Null, |digest| string(&digest.to_string())),
                ),
                ("reasons", Value::Array(reasons)),
            ]),
        ),
        (
            "controls",
            object(vec![
                ("status", string("unavailable")),
                (
                    "request_digest",
                    controls_request_digest
                        .map_or(Value::Null, |digest| string(&digest.to_string())),
                ),
                ("reasons", Value::Array(vec![string("not-parsed")])),
            ]),
        ),
        (
            "result",
            object(vec![
                ("complete", Value::Bool(false)),
                ("status", string("incomplete")),
                ("exit_code", Value::Integer(2)),
                ("finding_count", Value::Integer(0)),
                ("error_count", Value::Integer(error_count)),
            ]),
        ),
        ("summary", zero_summary()),
        ("documents", Value::Array(Vec::new())),
        ("observations", Value::Array(Vec::new())),
        ("findings", Value::Array(Vec::new())),
        ("errors", Value::Array(error_rows)),
    ]);

    let payload_digest = hj(PAYLOAD_SCHEMA, &payload);
    Some(object(vec![
        ("schema", string(ENVELOPE_SCHEMA)),
        ("payload", payload),
        ("payload_digest", string(&payload_digest.to_string())),
    ]))
}

/// One adapter's complete contract descriptor and its digest, which every
/// occurrence embeds through its observation-identity input.
#[must_use]
pub fn adapter_contract(engine: &EngineProvenance, adapter: Adapter) -> (Value, Digest) {
    let descriptor = object(vec![
        ("schema", string(ADAPTER_CONTRACT_SCHEMA)),
        ("adapter_id", string(adapter.adapter_id())),
        ("parser_name", string(adapter.parser_name())),
        ("parser_version", string(&engine.version)),
        ("grammar_profile", string(adapter.grammar_profile())),
        (
            "frontmatter_contract",
            string(adapter.frontmatter_contract()),
        ),
        ("source_projection", string(adapter.source_projection())),
        ("structural_address", string(adapter.structural_address())),
    ]);
    let digest = hj(ADAPTER_CONTRACT_SCHEMA, &descriptor);
    (descriptor, digest)
}

/// The complete engine block: contract, version, digest, provenance, policy
/// version, and the three adapter descriptors with their digests.
#[must_use]
pub fn engine_block(engine: &EngineProvenance) -> Value {
    let adapter_rows: Vec<Value> = Adapter::ALL
        .iter()
        .map(|adapter| {
            let (descriptor, digest) = adapter_contract(engine, *adapter);
            object(vec![
                ("adapter_id", string(adapter.adapter_id())),
                ("contract_descriptor", descriptor),
                ("contract_digest", string(&digest.to_string())),
            ])
        })
        .collect();
    object(vec![
        ("engine_contract", string(ENGINE_CONTRACT)),
        ("engine_version", string(&engine.version)),
        ("engine_digest", string(&engine.digest.to_string())),
        ("action_provenance", object(vec![("kind", string("local"))])),
        ("built_in_policy", string(BUILT_IN_POLICY)),
        ("adapters", Value::Array(adapter_rows)),
    ])
}

fn zero_summary() -> Value {
    let documents = [
        "discovered",
        "outside_document_set",
        "scanned",
        "unsupported",
        "excluded_builtin",
        "unlinked",
        "frontmatter_documents",
        "opaque_mdx_documents",
        "opaque_html_documents",
        "opaque_mdx_regions",
        "opaque_mdx_bytes",
        "opaque_html_regions",
        "opaque_html_bytes",
        "frontmatter_regions",
        "frontmatter_bytes",
    ];
    let references = [
        "extracted",
        "explicit_local",
        "same_repository",
        "external_out_of_scope",
        "unsupported",
        "resolved",
        "missing",
    ];
    let findings = [
        "total",
        "record",
        "warn",
        "fail",
        "introduced",
        "pre_existing",
        "resolved",
        "unknown",
        "not_applicable",
        "debt_tolerated",
        "waived",
        "analysis_errors",
        "unsupported_capabilities",
    ];
    object(vec![
        ("counts_complete", Value::Bool(false)),
        ("documents", zero_counts(&documents)),
        ("references", zero_counts(&references)),
        ("findings", zero_counts(&findings)),
        ("human_details_truncated", Value::Integer(0)),
        ("governed_claims", Value::Integer(0)),
        ("unattested_claims", Value::Integer(0)),
    ])
}

fn zero_counts(fields: &[&str]) -> Value {
    Value::Object(
        fields
            .iter()
            .map(|field| ((*field).to_owned(), Value::Integer(0)))
            .collect(),
    )
}

fn object(members: Vec<(&str, Value)>) -> Value {
    Value::Object(
        members
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn string(value: &str) -> Value {
    Value::String(value.to_owned())
}

/// The six resolution statuses. The status is total in the code: no other
/// pairing exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResolutionStatus {
    Resolved,
    Missing,
    TypeMismatch,
    Unsupported,
    Invalid,
    ExternalOutOfScope,
}

impl ResolutionStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            Self::Missing => "missing",
            Self::TypeMismatch => "type-mismatch",
            Self::Unsupported => "unsupported",
            Self::Invalid => "invalid",
            Self::ExternalOutOfScope => "external-out-of-scope",
        }
    }
}

/// The closed resolution codes in schema declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResolutionCode {
    ExactPath,
    PathNotFound,
    TargetTypeMismatch,
    SymlinkEntry,
    GitlinkEntry,
    UnsupportedQuerySemantics,
    UnsupportedFragmentSemantics,
    UnsupportedVersionScope,
    SiteRouteUnsupported,
    NetworkPathUnsupported,
    CodeFragmentUnevaluated,
    InvalidUri,
    InvalidPercentEncoding,
    DecodedPathControl,
    PathTraversal,
    BackslashSeparator,
    EncodedSlash,
    InvalidFragmentEncoding,
    InvalidReference,
    ExternalUrl,
    ForeignRepository,
}

impl ResolutionCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExactPath => "exact-path",
            Self::PathNotFound => "path-not-found",
            Self::TargetTypeMismatch => "target-type-mismatch",
            Self::SymlinkEntry => "symlink-entry",
            Self::GitlinkEntry => "gitlink-entry",
            Self::UnsupportedQuerySemantics => "unsupported-query-semantics",
            Self::UnsupportedFragmentSemantics => "unsupported-fragment-semantics",
            Self::UnsupportedVersionScope => "unsupported-version-scope",
            Self::SiteRouteUnsupported => "site-route-unsupported",
            Self::NetworkPathUnsupported => "network-path-unsupported",
            Self::CodeFragmentUnevaluated => "code-fragment-unevaluated",
            Self::InvalidUri => "invalid-uri",
            Self::InvalidPercentEncoding => "invalid-percent-encoding",
            Self::DecodedPathControl => "decoded-path-control",
            Self::PathTraversal => "path-traversal",
            Self::BackslashSeparator => "backslash-separator",
            Self::EncodedSlash => "encoded-slash",
            Self::InvalidFragmentEncoding => "invalid-fragment-encoding",
            Self::InvalidReference => "invalid-reference",
            Self::ExternalUrl => "external-url",
            Self::ForeignRepository => "foreign-repository",
        }
    }

    #[must_use]
    pub const fn status(self) -> ResolutionStatus {
        match self {
            Self::ExactPath => ResolutionStatus::Resolved,
            Self::PathNotFound => ResolutionStatus::Missing,
            Self::TargetTypeMismatch => ResolutionStatus::TypeMismatch,
            Self::SymlinkEntry
            | Self::GitlinkEntry
            | Self::UnsupportedQuerySemantics
            | Self::UnsupportedFragmentSemantics
            | Self::UnsupportedVersionScope
            | Self::SiteRouteUnsupported
            | Self::NetworkPathUnsupported
            | Self::CodeFragmentUnevaluated => ResolutionStatus::Unsupported,
            Self::InvalidUri
            | Self::InvalidPercentEncoding
            | Self::DecodedPathControl
            | Self::PathTraversal
            | Self::BackslashSeparator
            | Self::EncodedSlash
            | Self::InvalidFragmentEncoding
            | Self::InvalidReference => ResolutionStatus::Invalid,
            Self::ExternalUrl | Self::ForeignRepository => ResolutionStatus::ExternalOutOfScope,
        }
    }
}

/// The target-intent variants an occurrence can carry, in schema
/// declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum IntentKind {
    RepositoryPath,
    SameRepositoryGithub,
    SameRepositoryGitlab,
    SameRepositoryGitea,
    ExternalUrl,
    SiteRoute,
    Unsupported,
}

impl IntentKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RepositoryPath => "repository-path",
            Self::SameRepositoryGithub => "same-repository-github",
            Self::SameRepositoryGitlab => "same-repository-gitlab",
            Self::SameRepositoryGitea => "same-repository-gitea",
            Self::ExternalUrl => "external-url",
            Self::SiteRoute => "site-route",
            Self::Unsupported => "unsupported",
        }
    }
}

/// The four finding scopes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindingScope {
    Reference,
    Observation,
    Document,
    Control,
}

/// The closed disposition values a policy step can produce.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Disposition {
    Record,
    Warn,
    Fail,
}

impl Disposition {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

/// The complete closed finding taxonomy, in schema declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FindingKind {
    ExplicitTargetMissing,
    ExplicitTargetTypeMismatch,
    InvalidReference,
    UnsupportedReferenceSemantics,
    UnsupportedDocumentFormat,
    UnsupportedTargetKind,
    UnsupportedVersionScope,
    UnsupportedCapability,
    DependencyChangedSubjectUnchanged,
    DependencyAndSubjectCochanged,
    SubjectChanged,
    ExplicitReferenceRemoved,
    DocumentRemoved,
    ExternalOutOfScope,
    OpaqueMdxRegion,
    OpaqueHtmlRegion,
    ObservationCorrelationAmbiguous,
    UnlinkedDocument,
    PolicyWeakened,
    CoverageReduced,
    ControlPlaneChanged,
    DebtWorsened,
    DebtExpired,
    WaiverInvalid,
}

impl FindingKind {
    /// Every finding kind in schema declaration order.
    pub const ALL: [Self; 24] = [
        Self::ExplicitTargetMissing,
        Self::ExplicitTargetTypeMismatch,
        Self::InvalidReference,
        Self::UnsupportedReferenceSemantics,
        Self::UnsupportedDocumentFormat,
        Self::UnsupportedTargetKind,
        Self::UnsupportedVersionScope,
        Self::UnsupportedCapability,
        Self::DependencyChangedSubjectUnchanged,
        Self::DependencyAndSubjectCochanged,
        Self::SubjectChanged,
        Self::ExplicitReferenceRemoved,
        Self::DocumentRemoved,
        Self::ExternalOutOfScope,
        Self::OpaqueMdxRegion,
        Self::OpaqueHtmlRegion,
        Self::ObservationCorrelationAmbiguous,
        Self::UnlinkedDocument,
        Self::PolicyWeakened,
        Self::CoverageReduced,
        Self::ControlPlaneChanged,
        Self::DebtWorsened,
        Self::DebtExpired,
        Self::WaiverInvalid,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitTargetMissing => "explicit-target-missing",
            Self::ExplicitTargetTypeMismatch => "explicit-target-type-mismatch",
            Self::InvalidReference => "invalid-reference",
            Self::UnsupportedReferenceSemantics => "unsupported-reference-semantics",
            Self::UnsupportedDocumentFormat => "unsupported-document-format",
            Self::UnsupportedTargetKind => "unsupported-target-kind",
            Self::UnsupportedVersionScope => "unsupported-version-scope",
            Self::UnsupportedCapability => "unsupported-capability",
            Self::DependencyChangedSubjectUnchanged => "dependency-changed-subject-unchanged",
            Self::DependencyAndSubjectCochanged => "dependency-and-subject-cochanged",
            Self::SubjectChanged => "subject-changed",
            Self::ExplicitReferenceRemoved => "explicit-reference-removed",
            Self::DocumentRemoved => "document-removed",
            Self::ExternalOutOfScope => "external-out-of-scope",
            Self::OpaqueMdxRegion => "opaque-mdx-region",
            Self::OpaqueHtmlRegion => "opaque-html-region",
            Self::ObservationCorrelationAmbiguous => "observation-correlation-ambiguous",
            Self::UnlinkedDocument => "unlinked-document",
            Self::PolicyWeakened => "policy-weakened",
            Self::CoverageReduced => "coverage-reduced",
            Self::ControlPlaneChanged => "control-plane-changed",
            Self::DebtWorsened => "debt-worsened",
            Self::DebtExpired => "debt-expired",
            Self::WaiverInvalid => "waiver-invalid",
        }
    }

    /// The closed key-scope assignment.
    #[must_use]
    pub const fn scope(self) -> FindingScope {
        match self {
            Self::ExplicitTargetMissing | Self::ExplicitTargetTypeMismatch => {
                FindingScope::Reference
            }
            Self::InvalidReference
            | Self::UnsupportedReferenceSemantics
            | Self::UnsupportedTargetKind
            | Self::UnsupportedVersionScope
            | Self::DependencyChangedSubjectUnchanged
            | Self::DependencyAndSubjectCochanged
            | Self::SubjectChanged
            | Self::ExplicitReferenceRemoved
            | Self::ExternalOutOfScope
            | Self::ObservationCorrelationAmbiguous => FindingScope::Observation,
            Self::UnsupportedDocumentFormat
            | Self::DocumentRemoved
            | Self::OpaqueMdxRegion
            | Self::OpaqueHtmlRegion
            | Self::UnlinkedDocument => FindingScope::Document,
            Self::UnsupportedCapability
            | Self::PolicyWeakened
            | Self::CoverageReduced
            | Self::ControlPlaneChanged
            | Self::DebtWorsened
            | Self::DebtExpired
            | Self::WaiverInvalid => FindingScope::Control,
        }
    }

    /// The first policy-step result for a candidate fact under
    /// `scanner-policy-defaults`, per profile.
    #[must_use]
    pub const fn built_in_disposition(self, enforce: bool) -> Disposition {
        match self {
            Self::ExplicitTargetMissing
            | Self::ExplicitTargetTypeMismatch
            | Self::InvalidReference => {
                if enforce {
                    Disposition::Fail
                } else {
                    Disposition::Warn
                }
            }
            Self::UnsupportedCapability
            | Self::PolicyWeakened
            | Self::CoverageReduced
            | Self::ControlPlaneChanged
            | Self::DebtWorsened
            | Self::DebtExpired
            | Self::WaiverInvalid => Disposition::Fail,
            Self::DependencyChangedSubjectUnchanged | Self::ExplicitReferenceRemoved => {
                Disposition::Warn
            }
            Self::UnsupportedReferenceSemantics
            | Self::UnsupportedDocumentFormat
            | Self::UnsupportedTargetKind
            | Self::UnsupportedVersionScope
            | Self::DependencyAndSubjectCochanged
            | Self::SubjectChanged
            | Self::DocumentRemoved
            | Self::ExternalOutOfScope
            | Self::OpaqueMdxRegion
            | Self::OpaqueHtmlRegion
            | Self::ObservationCorrelationAmbiguous
            | Self::UnlinkedDocument => Disposition::Record,
        }
    }
}

pub const SANDBOX_SCHEMA: &str = "amiss/scanner-sandbox-profile";

/// The zero-capability sandbox descriptor the engine asserts for itself, and
/// its digest. A future wrapper verifies rather than asserts it.
#[must_use]
pub fn sandbox_descriptor() -> (Value, Digest) {
    let descriptor = object(vec![
        ("schema", string(SANDBOX_SCHEMA)),
        ("profile", string("scanner-zero-capability")),
        ("isolation", string("process")),
        ("network", string("denied")),
        ("child_processes", string("denied")),
        ("repository_processes", string("denied")),
        ("credentials", string("absent")),
        ("secrets", string("absent")),
        ("shared_cache", string("denied")),
        ("workspace", string("read-only")),
        ("environment", string("scanner-process-env")),
        (
            "physical_memory",
            object(vec![(
                "maximum_bytes",
                Value::Integer(i64::try_from(EVALUATOR_MANAGED_MEMORY_BYTES).unwrap_or(i64::MAX)),
            )]),
        ),
        (
            "temporary_storage",
            object(vec![
                ("kind", string("private-bounded")),
                (
                    "maximum_bytes",
                    Value::Integer(
                        i64::try_from(PRIVATE_TEMPORARY_STORAGE_BYTES).unwrap_or(i64::MAX),
                    ),
                ),
            ]),
        ),
        (
            "watchdog",
            object(vec![(
                "maximum_milliseconds",
                Value::Integer(i64::try_from(WATCHDOG_MILLISECONDS).unwrap_or(i64::MAX)),
            )]),
        ),
    ]);
    let digest = hj(SANDBOX_SCHEMA, &descriptor);
    (descriptor, digest)
}

impl FindingKind {
    #[must_use]
    pub const fn evidence_class(self) -> &'static str {
        match self {
            Self::ExplicitTargetMissing
            | Self::ExplicitTargetTypeMismatch
            | Self::InvalidReference => "deterministic-structural",
            Self::UnsupportedCapability
            | Self::UnsupportedReferenceSemantics
            | Self::UnsupportedDocumentFormat
            | Self::UnsupportedTargetKind
            | Self::UnsupportedVersionScope => "unsupported",
            Self::DependencyChangedSubjectUnchanged
            | Self::DependencyAndSubjectCochanged
            | Self::SubjectChanged => "impact-observation",
            Self::ExplicitReferenceRemoved
            | Self::DocumentRemoved
            | Self::ExternalOutOfScope
            | Self::OpaqueMdxRegion
            | Self::OpaqueHtmlRegion
            | Self::ObservationCorrelationAmbiguous
            | Self::UnlinkedDocument => "coverage-boundary",
            Self::PolicyWeakened
            | Self::CoverageReduced
            | Self::ControlPlaneChanged
            | Self::DebtWorsened
            | Self::DebtExpired
            | Self::WaiverInvalid => "control-plane",
        }
    }

    #[must_use]
    pub const fn invariant_class(self) -> &'static str {
        match self {
            Self::ExplicitTargetMissing
            | Self::ExplicitTargetTypeMismatch
            | Self::InvalidReference => "ratcheted",
            Self::UnsupportedCapability => "analysis-integrity",
            Self::UnsupportedReferenceSemantics
            | Self::UnsupportedDocumentFormat
            | Self::UnsupportedTargetKind
            | Self::UnsupportedVersionScope
            | Self::DependencyChangedSubjectUnchanged
            | Self::DependencyAndSubjectCochanged
            | Self::SubjectChanged
            | Self::ExplicitReferenceRemoved
            | Self::DocumentRemoved
            | Self::ExternalOutOfScope
            | Self::OpaqueMdxRegion
            | Self::OpaqueHtmlRegion
            | Self::ObservationCorrelationAmbiguous
            | Self::UnlinkedDocument => "advisory",
            Self::PolicyWeakened
            | Self::CoverageReduced
            | Self::ControlPlaneChanged
            | Self::DebtWorsened
            | Self::DebtExpired
            | Self::WaiverInvalid => "absolute",
        }
    }
}

/// One typed analysis error's reportable detail: the code, the exact path
/// where the partition names one, the raw bytes of a name the report cannot
/// hold as text, and the crossing triple for a resource error. Field order
/// is the canonical error key, so the derived ordering is the wire's.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ErrorDetail {
    pub code: AnalysisErrorCode,
    pub path: Option<crate::model::RepoPath>,
    pub path_bytes: Option<Vec<u8>>,
    pub resource: Option<(crate::controls::ResourceName, u64, u64)>,
}

impl ErrorDetail {
    #[must_use]
    pub fn phase(&self) -> &'static str {
        self.resource.map_or_else(
            || self.code.fixed_phase().unwrap_or("internal"),
            |(name, _limit, _observed)| name.phase(),
        )
    }
}

/// One wire error row with its partition phase.
#[must_use]
pub fn error_row_value(detail: &ErrorDetail) -> Value {
    error_row(detail, detail.phase())
}

fn error_row(detail: &ErrorDetail, phase: &str) -> Value {
    let (resource, limit, observed) = detail.resource.map_or(
        (Value::Null, Value::Null, Value::Null),
        |(name, limit, observed)| {
            (
                string(name.as_str()),
                Value::Integer(i64::try_from(limit).unwrap_or(i64::MAX)),
                Value::Integer(i64::try_from(observed).unwrap_or(i64::MAX)),
            )
        },
    );
    object(vec![
        ("phase", string(phase)),
        ("code", string(detail.code.as_str())),
        (
            "path",
            detail
                .path
                .as_ref()
                .map_or(Value::Null, crate::model::RepoPath::to_value),
        ),
        (
            "path_bytes_hex",
            detail.path_bytes.as_deref().map_or(Value::Null, |bytes| {
                Value::String(crate::model::hex_lower(bytes))
            }),
        ),
        ("resource", resource),
        ("configured_limit", limit),
        ("observed_lower_bound", observed),
    ])
}
