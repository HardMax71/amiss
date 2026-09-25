macro_rules! declare_taxonomy {
    (
        $(#[$attribute:meta])*
        $visibility:vis enum $name:ident {
            $(
                $variant:ident => {
                    meaning: $meaning:literal,
                    metadata: $metadata:expr,
                }
            ),+ $(,)?
        }
        metadata $metadata_visibility:vis const fn $metadata_method:ident(self) -> $metadata_type:ty;
    ) => {
        $(#[$attribute])*
        $visibility enum $name {
            $($variant),+
        }

        impl $name {
            /// Every taxonomy value in declaration order.
            #[must_use]
            pub fn all() -> impl ExactSizeIterator<Item = Self> {
                Self::iter()
            }

            /// The fixed engine-owned description for this taxonomy value.
            #[must_use]
            pub const fn meaning(self) -> &'static str {
                match self {
                    $(Self::$variant => $meaning),+
                }
            }

            /// The typed immutable metadata owned by this taxonomy value.
            #[must_use]
            $metadata_visibility const fn $metadata_method(self) -> $metadata_type {
                match self {
                    $(Self::$variant => $metadata),+
                }
            }
        }
    };
}

macro_rules! declare_meaningful_enum {
    (
        $(#[$attribute:meta])*
        $visibility:vis enum $name:ident {
            $($variant:ident => $meaning:literal),+ $(,)?
        }
    ) => {
        $(#[$attribute])*
        $visibility enum $name {
            $($variant),+
        }

        impl $name {
            /// The fixed engine-owned description for this taxonomy value.
            #[must_use]
            pub const fn meaning(self) -> &'static str {
                match self {
                    $(Self::$variant => $meaning),+
                }
            }
        }
    };
}

mod error;
mod failure;
mod finding;
pub mod model;
mod output;
mod sandbox;

use crate::ExitClass;

pub use error::{ErrorDetail, error_row, fault_code};
pub use failure::{
    EngineProvenance, adapter_contract, engine_block, invocation_failure_envelope,
    invocation_failure_wire, unavailable_evaluation_envelope, unavailable_evaluation_wire,
};
pub use finding::{
    Disposition, EvidenceClass, FindingKind, FindingMetadata, FindingScope, FixKind, IntentKind,
    InvariantClass,
};
pub use output::{emit_report, emit_sealed};
pub use sandbox::sandbox_descriptor;

pub const ENGINE_CONTRACT: &str = "amiss/scanner";

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

pub const ENGINE_DOMAIN: &str = "amiss/scanner-engine";
pub const ENVELOPE_SCHEMA: &str = "amiss/scanner-report-envelope";
pub const PAYLOAD_SCHEMA: &str = "amiss/scanner-report-payload";
/// The wire's own version: a reshape mints the next major, as a major release.
pub const COMPATIBILITY: &str = "3";
pub const ADAPTER_CONTRACT_SCHEMA: &str = "amiss/scanner-adapter-contract";
pub const BUILT_IN_POLICY: &str = "scanner-policy-defaults";
pub const SANDBOX_SCHEMA: &str = "amiss/scanner-sandbox-profile";

/// Why a report-bound operation cannot consume a scanner report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReportDefect {
    #[error("the input is not a scanner report envelope")]
    NotAReport,
    #[error("the report is not the canonical spelling of its model")]
    Noncanonical,
    #[error("the report payload does not match its recorded digest")]
    DigestMismatch,
    #[error("the report carries an invalid result tuple")]
    InvalidResult,
    #[error("the report is incomplete, so its sides cannot be compared")]
    Incomplete,
    #[error("a delegated occurrence is missing its destination, document, or required scheme")]
    MalformedExternal,
}

impl From<crate::de::Error> for ReportDefect {
    fn from(error: crate::de::Error) -> Self {
        match error.kind {
            crate::de::ErrorKind::Noncanonical => Self::Noncanonical,
            crate::de::ErrorKind::DigestMismatch => Self::DigestMismatch,
            crate::de::ErrorKind::Json(_)
            | crate::de::ErrorKind::MissingField
            | crate::de::ErrorKind::UnknownField
            | crate::de::ErrorKind::DuplicateKey
            | crate::de::ErrorKind::WrongType
            | crate::de::ErrorKind::InvalidValue
            | crate::de::ErrorKind::UnsortedSet
            | crate::de::ErrorKind::DuplicateMember
            | crate::de::ErrorKind::LimitExceeded
            | crate::de::ErrorKind::Inconsistent => Self::NotAReport,
        }
    }
}

/// Checks the recorded completeness, status and exit code as one verdict.
///
/// # Errors
/// Refuses inconsistent or unsupported result tuples.
pub fn result_verdict(result: &model::ReportResult) -> Result<ExitClass, ReportDefect> {
    match (result.complete, result.status, result.exit_code) {
        (true, model::ReportStatus::Pass, 0) => Ok(ExitClass::Success),
        (true, model::ReportStatus::Fail, 1) => Ok(ExitClass::BlockingFindings),
        (false, model::ReportStatus::Incomplete, 2) => Ok(ExitClass::Failure),
        (_, _, _) => Err(ReportDefect::InvalidResult),
    }
}
