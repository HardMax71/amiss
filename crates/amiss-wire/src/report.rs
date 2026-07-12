use std::collections::BTreeSet;

use crate::digest::{Digest, hj};
use crate::json::{Value, canonical};

pub const ENGINE_CONTRACT: &str = "amiss/scanner-v0";
pub const ENGINE_DOMAIN: &str = "amiss/scanner-engine/v1";
pub const ENVELOPE_SCHEMA: &str = "amiss/scanner-report-envelope/v1";
pub const PAYLOAD_SCHEMA: &str = "amiss/scanner-report-payload/v1";
pub const ADAPTER_CONTRACT_SCHEMA: &str = "amiss/scanner-adapter-contract/v1";
pub const BUILT_IN_POLICY_VERSION: &str = "scanner-policy-defaults-v1";

/// The closed analysis-error codes in schema declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnalysisErrorCode {
    InvalidInvocation,
    UnsupportedProviderHost,
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
            Self::UnsupportedProviderHost => "UNSUPPORTED_PROVIDER_HOST",
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
            | Self::UnsupportedProviderHost
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
            Self::UnsupportedProviderHost => Some("unsupported-provider"),
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
            object(vec![
                ("phase", string(phase)),
                ("code", string(code.as_str())),
                ("path", Value::Null),
                ("path_bytes_hex", Value::Null),
                ("resource", Value::Null),
                ("configured_limit", Value::Null),
                ("observed_lower_bound", Value::Null),
            ])
        })
        .collect();
    let error_count = i64::try_from(error_rows.len()).ok()?;

    let payload = object(vec![
        ("schema", string(PAYLOAD_SCHEMA)),
        ("compatibility", string("experimental")),
        ("engine", engine_value(engine)),
        (
            "evaluation",
            object(vec![
                ("status", string("unavailable")),
                ("request_digest", Value::Null),
                ("reasons", Value::Array(reasons)),
            ]),
        ),
        (
            "controls",
            object(vec![
                ("status", string("unavailable")),
                ("request_digest", Value::Null),
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
    let envelope = object(vec![
        ("schema", string(ENVELOPE_SCHEMA)),
        ("payload", payload),
        ("payload_digest", string(&payload_digest.to_string())),
    ]);
    let mut wire = canonical(&envelope);
    wire.push(b'\n');
    Some(wire)
}

fn engine_value(engine: &EngineProvenance) -> Value {
    let adapters = [
        (
            "markdown-v1",
            "amiss-markdown-adapter",
            "commonmark-gfm-v1",
            "frontmatter-v1",
            "source-projection-v1",
            "markdown-ast-node-path",
        ),
        (
            "mdx-v1",
            "amiss-mdx-adapter",
            "mdx-source-v1",
            "frontmatter-v1",
            "source-projection-v1",
            "mdx-ast-node-path",
        ),
        (
            "plain-advisory-v1",
            "amiss-plain-advisory",
            "plain-zero-lexer-v1",
            "none",
            "none",
            "none",
        ),
    ];
    let adapter_rows: Vec<Value> = adapters
        .iter()
        .map(|(id, parser, grammar, frontmatter, projection, address)| {
            let descriptor = object(vec![
                ("schema", string(ADAPTER_CONTRACT_SCHEMA)),
                ("adapter_id", string(id)),
                ("parser_name", string(parser)),
                ("parser_version", string(&engine.version)),
                ("grammar_profile", string(grammar)),
                ("frontmatter_contract", string(frontmatter)),
                ("source_projection", string(projection)),
                ("structural_address", string(address)),
            ]);
            let digest = hj(ADAPTER_CONTRACT_SCHEMA, &descriptor);
            object(vec![
                ("adapter_id", string(id)),
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
        ("built_in_policy_version", string(BUILT_IN_POLICY_VERSION)),
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
        "same_repository_github",
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
