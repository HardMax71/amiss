pub mod action;
pub mod assessment;
mod bounded_envelope;
pub mod controls;
pub mod de;
pub mod digest;
pub mod external;
pub mod extraction;
pub mod human;
pub mod json;
pub mod locale;
pub mod manifest;
pub mod model;
pub mod publication;
pub mod relation;
pub mod report;
pub mod requests;
pub mod resolution;
pub mod semantic;
pub mod uri;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitClass {
    /// Complete evaluation, no effective blocking finding.
    Success,
    /// Complete evaluation, at least one effective blocking finding.
    BlockingFindings,
    /// Anything that prevented a trustworthy complete result.
    Failure,
}

impl ExitClass {
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::BlockingFindings => 1,
            Self::Failure => 2,
        }
    }
}
