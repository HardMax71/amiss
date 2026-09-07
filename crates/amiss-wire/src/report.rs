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
mod read;
mod sandbox;

pub use error::{AnalysisErrorCode, ErrorDetail, error_row};
pub use failure::{
    EngineProvenance, adapter_contract, engine_block, invocation_failure_envelope,
    invocation_failure_wire, unavailable_evaluation_envelope, unavailable_evaluation_wire,
};
pub use finding::{Disposition, FindingKind, FindingMetadata, FindingScope, FixKind, IntentKind};
pub use output::emit_report;
pub use read::{result_verdict, validate_envelope};
pub use sandbox::sandbox_descriptor;

pub const ENGINE_CONTRACT: &str = "amiss/scanner";

/// The exact `machine-json-bytes` reservation: the report wire, canonical
/// envelope plus the trailing newline, never exceeds this.
pub const MACHINE_JSON_BYTES: u64 = 268_435_456;

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
pub const COMPATIBILITY: &str = "1";
pub const ADAPTER_CONTRACT_SCHEMA: &str = "amiss/scanner-adapter-contract";
pub const BUILT_IN_POLICY: &str = "scanner-policy-defaults";
pub const SANDBOX_SCHEMA: &str = "amiss/scanner-sandbox-profile";

/// Why a report-bound operation cannot consume a scanner report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReportDefect {
    #[error("the input is not a scanner report envelope")]
    NotAReport,
    #[error("the report uses an unsupported wire compatibility")]
    UnsupportedCompatibility,
    #[error("the report payload does not match its recorded digest")]
    DigestMismatch,
    #[error("the report carries an invalid result tuple")]
    InvalidResult,
    #[error("the report is incomplete, so its sides cannot be compared")]
    Incomplete,
    #[error("a delegated occurrence is missing its destination, document, or required scheme")]
    MalformedExternal,
}
