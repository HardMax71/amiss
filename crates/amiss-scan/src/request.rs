use amiss_wire::controls::{FloorDefect, ResourceName};
use amiss_wire::de::{Error, ErrorKind};
use amiss_wire::envelope::document_digest;
use amiss_wire::report::{AnalysisErrorCode, ErrorDetail};
use amiss_wire::requests::ControlsRequest;

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

/// Validates and consumes the typed controls, requiring their semantic digests
/// to equal the independently supplied expected digests.
///
/// # Errors
///
/// The first malformed embedded control or digest mismatch, as one typed
/// configuration detail suitable for the pipeline's unavailable projection.
pub fn controls(request: ControlsRequest) -> Result<ControlInputs, ErrorDetail> {
    let floor = request
        .organization_floor
        .map(organization_floor)
        .transpose()?;
    let debt = request
        .debt_snapshot
        .map(|supplied| {
            supplied
                .value
                .validate()
                .map_err(|error| configuration_detail(&error))?;
            let digest = admitted(
                &supplied.value,
                amiss_wire::controls::DEBT_SNAPSHOT_SCHEMA,
                supplied.expected_digest,
            )?;
            Ok(DebtInput {
                snapshot: supplied.value,
                digest,
                trust_source: supplied.trust_source,
            })
        })
        .transpose()?;
    let waiver = request
        .waiver_bundle
        .map(|supplied| {
            supplied
                .value
                .validate()
                .map_err(|error| configuration_detail(&error))?;
            let digest = admitted(
                &supplied.value,
                amiss_wire::controls::WAIVER_BUNDLE_SCHEMA,
                supplied.expected_digest,
            )?;
            Ok(WaiverInput {
                bundle: supplied.value,
                digest,
                trust_source: supplied.trust_source,
            })
        })
        .transpose()?;
    let time = request.trusted_time.map(trusted_time).transpose()?;
    let constraint = request
        .execution_constraint
        .map(|supplied| {
            supplied
                .value
                .validate()
                .map_err(|error| configuration_detail(&error))?;
            admitted(
                &supplied.value,
                amiss_wire::controls::EXECUTION_CONSTRAINT_SCHEMA,
                supplied.expected_digest,
            )?;
            Ok(ConstraintInput {
                descriptor: supplied.value,
                trust_source: supplied.trust_source,
            })
        })
        .transpose()?;
    let semantic = crate::semantic::parse(request.semantic_evidence.into_iter().enumerate().map(
        |(index, supplied)| {
            crate::semantic::validated_envelope(supplied, &format!("$.semantic_evidence[{index}]"))
        },
    ))
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

/// The admission law: a supplied control is accepted only when its own
/// canonical bytes recompute the digest the caller acquired independently.
fn admitted<T: serde::Serialize>(
    value: &T,
    domain: &str,
    expected: amiss_wire::model::Digest,
) -> Result<amiss_wire::model::Digest, ErrorDetail> {
    let digest = document_digest(domain, value)
        .ok_or_else(|| code(AnalysisErrorCode::ConfigurationInvalid))?;
    (digest == expected)
        .then_some(digest)
        .ok_or_else(|| code(AnalysisErrorCode::DigestMismatch))
}

fn organization_floor(
    supplied: amiss_wire::requests::SuppliedControl<amiss_wire::controls::OrganizationFloor>,
) -> Result<FloorInput, ErrorDetail> {
    supplied.value.validate().map_err(floor_detail)?;
    let digest = admitted(
        &supplied.value,
        amiss_wire::controls::ORGANIZATION_FLOOR_SCHEMA,
        supplied.expected_digest,
    )?;
    Ok(FloorInput {
        floor: supplied.value,
        digest,
        trust_source: supplied.trust_source,
    })
}

fn trusted_time(supplied: amiss_wire::requests::SuppliedTime) -> Result<TimeInput, ErrorDetail> {
    supplied
        .value
        .validate()
        .map_err(|error| configuration_detail(&error))?;
    admitted(
        &supplied.value,
        amiss_wire::controls::TRUSTED_TIME_STATEMENT_SCHEMA,
        supplied.expected_digest,
    )?;
    Ok(TimeInput {
        statement: supplied.value,
        provider: supplied.provider,
        provider_run_id: supplied.provider_run_id,
        provider_run_attempt: supplied.provider_run_attempt,
    })
}

fn floor_detail(error: FloorDefect) -> ErrorDetail {
    match error {
        FloorDefect::Schema(error) => configuration_detail(&error),
        FloorDefect::Entries {
            configured_limit,
            observed_lower_bound,
        } => ErrorDetail {
            resource: Some((
                ResourceName::OrganizationPolicyEntries,
                configured_limit,
                observed_lower_bound,
            )),
            ..code(AnalysisErrorCode::ResourceLimitExceeded)
        },
    }
}

/// Maps one strict external-input defect into the scanner's public analysis taxonomy.
#[must_use]
pub fn configuration_detail(error: &Error) -> ErrorDetail {
    let analysis = match &error.kind {
        ErrorKind::Json(message)
            if message.starts_with("invalid utf-8") || message.starts_with("incomplete utf-8") =>
        {
            AnalysisErrorCode::InvalidUtf8
        }
        ErrorKind::Json(message) if message.starts_with("duplicate JSON key") => {
            AnalysisErrorCode::DuplicateJsonKey
        }
        ErrorKind::Json(_) => AnalysisErrorCode::InvalidJson,
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
