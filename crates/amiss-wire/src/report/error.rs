use strum::{AsRefStr, EnumIter, IntoEnumIterator, IntoStaticStr};

use crate::json::Value;

use super::{object, string};

/// The closed analysis-error codes in schema declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, AsRefStr, EnumIter, IntoStaticStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
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
    /// Every analysis-error code in schema declaration order.
    #[must_use]
    pub fn all() -> impl ExactSizeIterator<Item = Self> {
        Self::iter()
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

pub(super) fn error_row(detail: &ErrorDetail, phase: &str) -> Value {
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
        ("code", string(detail.code.as_ref())),
        ("description", string(detail.code.meaning())),
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
                Value::String(crate::model::hex_lower(bytes).into())
            }),
        ),
        ("resource", resource),
        ("configured_limit", limit),
        ("observed_lower_bound", observed),
    ])
}
