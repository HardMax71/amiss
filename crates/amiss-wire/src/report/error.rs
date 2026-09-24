use crate::controls::AnalysisPhase;
use crate::extraction::Fault;

use super::model::{AnalysisError, AnalysisErrorCode};

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

/// One wire error row with its partition phase.
#[must_use]
pub fn error_row(detail: &ErrorDetail) -> AnalysisError<crate::model::RepoPath> {
    let phase = detail.resource.map_or_else(
        || {
            detail
                .code
                .route()
                .map_or(AnalysisPhase::Internal, |route| route.phase)
        },
        |(name, _limit, _observed)| name.phase(),
    );
    AnalysisError {
        phase,
        code: detail.code,
        description: detail.code.meaning().to_owned(),
        path: detail.path.clone(),
        path_bytes_hex: detail.path_bytes.as_ref().map(hex::encode),
        resource: detail.resource.map(|(name, _, _)| name),
        configured_limit: detail
            .resource
            .map(|(_, limit, _)| limit.min(i64::MAX.unsigned_abs())),
        observed_lower_bound: detail
            .resource
            .map(|(_, _, observed)| observed.min(i64::MAX.unsigned_abs())),
    }
}

/// The analysis error a parse fault is reported as.
#[must_use]
pub const fn fault_code(fault: Fault) -> AnalysisErrorCode {
    match fault {
        Fault::DocumentInvalid | Fault::DocumentUnparsable => AnalysisErrorCode::DocumentInvalid,
        Fault::ParserError => AnalysisErrorCode::ParserError,
        Fault::ParserPanic => AnalysisErrorCode::ParserPanic,
        Fault::InvalidSourceSpan => AnalysisErrorCode::InvalidSourceSpan,
    }
}
