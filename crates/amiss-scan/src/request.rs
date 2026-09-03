use amiss_wire::controls::{
    FloorDefect, OrganizationFloor, ResourceName, canonical_debt_snapshot,
    canonical_execution_constraint, canonical_trusted_time, canonical_waiver_bundle,
    parse_debt_snapshot, parse_execution_constraint, parse_trusted_time, parse_waiver_bundle,
};
use amiss_wire::de::{Error, ErrorKind};
use amiss_wire::digest::Digest;
use amiss_wire::json::ErrorKind as JsonErrorKind;
use amiss_wire::report::{AnalysisErrorCode, ErrorDetail};
use amiss_wire::requests::{ControlsRequest, RequestTrust, SuppliedControl};

use crate::policy::{ConstraintInput, DebtInput, FloorInput, TimeInput, WaiverInput};

/// Typed external inputs after the request's embedded values and independent
/// expected digests have both been verified.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ControlInputs {
    pub floor: Option<FloorInput>,
    pub debt: Option<DebtInput>,
    pub waiver: Option<WaiverInput>,
    pub time: Option<TimeInput>,
    pub constraint: Option<ConstraintInput>,
    pub semantic: crate::semantic::Inputs,
}

/// Parses every supplied value under its own schema and requires its semantic
/// digest to equal the independently supplied expected digest.
///
/// # Errors
///
/// The first malformed embedded control or digest mismatch, as one typed
/// configuration detail suitable for the pipeline's unavailable projection.
pub fn controls(request: &ControlsRequest) -> Result<ControlInputs, ErrorDetail> {
    let floor = request
        .organization_floor
        .as_ref()
        .map(|supplied| {
            let bytes = serde_json::to_vec(&supplied.value)
                .map_err(|_defect| code(AnalysisErrorCode::ConfigurationInvalid))?;
            let floor = OrganizationFloor::parse(&bytes).map_err(floor_detail)?;
            if floor.digest() != supplied.expected_digest {
                return Err(code(AnalysisErrorCode::DigestMismatch));
            }
            Ok(FloorInput {
                floor,
                trust_source: supplied.trust_source,
            })
        })
        .transpose()?;
    let debt = request
        .debt_snapshot
        .as_ref()
        .map(|supplied| {
            typed(supplied, parse_debt_snapshot, canonical_debt_snapshot).map(
                |(snapshot, digest, trust_source)| DebtInput {
                    snapshot,
                    digest,
                    trust_source,
                },
            )
        })
        .transpose()?;
    let waiver = request
        .waiver_bundle
        .as_ref()
        .map(|supplied| {
            typed(supplied, parse_waiver_bundle, canonical_waiver_bundle).map(
                |(bundle, digest, trust_source)| WaiverInput {
                    bundle,
                    digest,
                    trust_source,
                },
            )
        })
        .transpose()?;
    let time = request
        .trusted_time
        .as_ref()
        .map(|supplied| {
            let bytes = serde_json::to_vec(&supplied.value)
                .map_err(|_defect| code(AnalysisErrorCode::ConfigurationInvalid))?;
            let statement =
                parse_trusted_time(&bytes).map_err(|error| configuration_detail(&error))?;
            let (_, digest) =
                canonical_trusted_time(&statement).map_err(|error| configuration_detail(&error))?;
            if digest != supplied.expected_digest {
                return Err(code(AnalysisErrorCode::DigestMismatch));
            }
            Ok(TimeInput {
                statement,
                provider: supplied.provider.clone(),
                provider_run_id: supplied.provider_run_id.clone(),
                provider_run_attempt: supplied.provider_run_attempt,
            })
        })
        .transpose()?;
    let constraint = request
        .execution_constraint
        .as_ref()
        .map(|supplied| {
            let bytes = serde_json::to_vec(&supplied.value)
                .map_err(|_defect| code(AnalysisErrorCode::ConfigurationInvalid))?;
            let descriptor =
                parse_execution_constraint(&bytes).map_err(|error| configuration_detail(&error))?;
            let (_, digest) = canonical_execution_constraint(&descriptor)
                .map_err(|error| configuration_detail(&error))?;
            if digest != supplied.expected_digest {
                return Err(code(AnalysisErrorCode::DigestMismatch));
            }
            Ok(ConstraintInput {
                descriptor,
                trust_source: supplied.trust_source,
            })
        })
        .transpose()?;
    let semantic = crate::semantic::parse(&request.semantic_evidence)
        .map_err(|error| configuration_detail(&error))?;
    Ok(ControlInputs {
        floor,
        debt,
        waiver,
        time,
        constraint,
        semantic,
    })
}

fn floor_detail(error: FloorDefect) -> ErrorDetail {
    match error {
        FloorDefect::Schema(error) => configuration_detail(&error),
        FloorDefect::Entries {
            configured_limit,
            observed_lower_bound,
        } => ErrorDetail {
            code: AnalysisErrorCode::ResourceLimitExceeded,
            path: None,
            path_bytes: None,
            resource: Some((
                ResourceName::OrganizationPolicyEntries,
                configured_limit,
                observed_lower_bound,
            )),
        },
    }
}

fn typed<T>(
    supplied: &SuppliedControl,
    parse: impl FnOnce(&[u8]) -> Result<T, Error>,
    canonical: impl FnOnce(&T) -> Result<(Vec<u8>, Digest), Error>,
) -> Result<(T, Digest, RequestTrust), ErrorDetail> {
    let bytes = serde_json::to_vec(&supplied.value)
        .map_err(|_defect| code(AnalysisErrorCode::ConfigurationInvalid))?;
    let value = parse(&bytes).map_err(|error| configuration_detail(&error))?;
    let digest = canonical(&value)
        .map_err(|error| configuration_detail(&error))?
        .1;
    if digest != supplied.expected_digest {
        return Err(code(AnalysisErrorCode::DigestMismatch));
    }
    Ok((value, digest, supplied.trust_source))
}

/// Maps one strict external-input defect into the scanner's public analysis taxonomy.
#[must_use]
pub fn configuration_detail(error: &Error) -> ErrorDetail {
    let analysis = match error.kind {
        ErrorKind::Json(json) => match json.kind {
            JsonErrorKind::InvalidUtf8 => AnalysisErrorCode::InvalidUtf8,
            JsonErrorKind::DuplicateKey => AnalysisErrorCode::DuplicateJsonKey,
            JsonErrorKind::ByteOrderMark
            | JsonErrorKind::UnexpectedEnd
            | JsonErrorKind::UnexpectedByte
            | JsonErrorKind::TrailingContent
            | JsonErrorKind::DepthLimit
            | JsonErrorKind::ControlCharacter
            | JsonErrorKind::InvalidEscape
            | JsonErrorKind::LoneSurrogate
            | JsonErrorKind::NegativeZero
            | JsonErrorKind::FractionOrExponent
            | JsonErrorKind::IntegerOutOfRange => AnalysisErrorCode::InvalidJson,
        },
        ErrorKind::UnknownField => AnalysisErrorCode::UnknownField,
        ErrorKind::DigestMismatch => AnalysisErrorCode::DigestMismatch,
        ErrorKind::UnsortedSet | ErrorKind::DuplicateMember => AnalysisErrorCode::NoncanonicalArray,
        ErrorKind::MissingField
        | ErrorKind::WrongType
        | ErrorKind::InvalidValue
        | ErrorKind::LimitExceeded
        | ErrorKind::Inconsistent => AnalysisErrorCode::ConfigurationInvalid,
    };
    code(analysis)
}

const fn code(code: AnalysisErrorCode) -> ErrorDetail {
    ErrorDetail {
        code,
        path: None,
        path_bytes: None,
        resource: None,
    }
}
