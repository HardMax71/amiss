use amiss_wire::controls::{
    DebtSnapshot, ExecutionConstraintDescriptor, FloorDefect, OrganizationFloor, ResourceName,
    TrustedTimeStatement, WaiverBundle,
};
use amiss_wire::de::{Error, ErrorKind};
use amiss_wire::digest::Digest;
use amiss_wire::json::{ErrorKind as JsonErrorKind, canonical};
use amiss_wire::report::{AnalysisErrorCode, ErrorDetail};
use amiss_wire::requests::{ControlsRequest, RequestTrust, SuppliedControl};

use crate::policy::{ConstraintInput, DebtInput, FloorInput, TimeInput, TrustSource, WaiverInput};

/// Typed external inputs after the request's embedded values and independent
/// expected digests have both been verified.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ControlInputs {
    pub floor: Option<FloorInput>,
    pub debt: Option<DebtInput>,
    pub waiver: Option<WaiverInput>,
    pub time: Option<TimeInput>,
    pub constraint: Option<ConstraintInput>,
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
            let bytes = canonical(&supplied.value);
            let floor = OrganizationFloor::parse(&bytes).map_err(floor_detail)?;
            if floor.digest != supplied.expected_digest {
                return Err(code(AnalysisErrorCode::DigestMismatch));
            }
            Ok(FloorInput {
                floor,
                trust_source: trust(supplied.trust_source),
            })
        })
        .transpose()?;
    let debt = request
        .debt_snapshot
        .as_ref()
        .map(|supplied| {
            typed(supplied, DebtSnapshot::parse, |value| value.digest).map(
                |(snapshot, trust_source)| DebtInput {
                    snapshot,
                    trust_source,
                },
            )
        })
        .transpose()?;
    let waiver = request
        .waiver_bundle
        .as_ref()
        .map(|supplied| {
            typed(supplied, WaiverBundle::parse, |value| value.digest).map(
                |(bundle, trust_source)| WaiverInput {
                    bundle,
                    trust_source,
                },
            )
        })
        .transpose()?;
    let time = request
        .trusted_time
        .as_ref()
        .map(|supplied| {
            let bytes = canonical(&supplied.value);
            let statement = TrustedTimeStatement::parse(&bytes).map_err(|error| detail(&error))?;
            if statement.digest != supplied.expected_digest {
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
            typed(supplied, ExecutionConstraintDescriptor::parse, |value| {
                value.digest
            })
            .map(|(descriptor, trust_source)| ConstraintInput {
                descriptor,
                trust_source,
            })
        })
        .transpose()?;
    Ok(ControlInputs {
        floor,
        debt,
        waiver,
        time,
        constraint,
    })
}

fn floor_detail(error: FloorDefect) -> ErrorDetail {
    match error {
        FloorDefect::Schema(error) => detail(&error),
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
    digest: impl FnOnce(&T) -> Digest,
) -> Result<(T, TrustSource), ErrorDetail> {
    let bytes = canonical(&supplied.value);
    let value = parse(&bytes).map_err(|error| detail(&error))?;
    if digest(&value) != supplied.expected_digest {
        return Err(code(AnalysisErrorCode::DigestMismatch));
    }
    Ok((value, trust(supplied.trust_source)))
}

const fn trust(source: RequestTrust) -> TrustSource {
    match source {
        RequestTrust::ExternalRequiredCheck => TrustSource::ExternalRequiredCheck,
        RequestTrust::OrganizationPolicy => TrustSource::OrganizationPolicy,
    }
}

fn detail(error: &Error) -> ErrorDetail {
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
