mod format;
mod semantic;
mod store;

use std::time::Duration;

use amiss_wire::digest::Digest;
use url::Url;

use crate::{
    ControllerEvaluationId, ExternalTally, PublicationAuditBundle, PublicationAuditDigests,
    RelationAuditBundle, RelationAuditDigests,
};

pub use store::FileArtifactStore;

pub const MAX_ARTIFACT_RECORDS: u64 = 100_000;
pub const MAX_ARTIFACT_BYTES: u64 = 68_719_476_736;
pub const MAX_ARTIFACT_RECORD_BYTES: u64 = 1_073_741_824;
pub const MAX_ARTIFACT_RETENTION: Duration = Duration::from_hours(8_760);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactStoreConfig {
    pub base_url: String,
    pub retention: Duration,
    pub max_records: u64,
    pub max_bytes: u64,
    pub max_record_bytes: u64,
}

#[derive(Clone, Copy)]
pub struct ArtifactBundle<'a> {
    pub report: &'a [u8],
    pub semantic: Option<&'a [u8]>,
    pub plan: Option<&'a [u8]>,
    pub evidence: Option<&'a [u8]>,
    pub assessment: Option<&'a [u8]>,
    pub external_tally: Option<ExternalTally>,
    pub external_incomplete: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactReference {
    pub id: String,
    pub locator: String,
    pub expires_at_unix_millis: i64,
    pub report_digest: Digest,
    pub semantic_digest: Option<Digest>,
    pub assessment_digest: Option<Digest>,
    pub external_tally: Option<ExternalTally>,
    pub external_incomplete: bool,
}

#[derive(Clone, Copy)]
pub enum ArtifactAuditBundle<'a> {
    Publication(PublicationAuditBundle<'a>),
    Relation(RelationAuditBundle<'a>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactAuditDigests {
    Publication(PublicationAuditDigests),
    Relation(RelationAuditDigests),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactAuditReference {
    pub artifact: ArtifactReference,
    pub audit: ArtifactAuditDigests,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    strum::AsRefStr,
    strum::EnumIter,
    strum::EnumString,
)]
#[strum(serialize_all = "kebab-case")]
pub enum ArtifactComponent {
    Report,
    Semantic,
    Plan,
    Evidence,
    Assessment,
    PublicationPlan,
    PublicationEvidence,
    PublicationAssessment,
    RelationPlan,
    RelationEvidence,
    RelationAssessment,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArtifactCleanup {
    pub removed_records: u64,
    pub removed_bytes: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("artifact store is already open")]
    AlreadyOpen,
    #[error("artifact store configuration changed")]
    Configuration,
    #[error("artifact store is corrupt")]
    Corrupt,
    #[error("artifact store capacity is exhausted")]
    Full,
    #[error("artifact exceeds its configured size limit")]
    TooLarge,
    #[error("artifact identity was rebound to different bytes")]
    Conflict,
    #[error("artifact is absent or expired")]
    NotFound,
    #[error("artifact clock is unavailable")]
    Clock,
    #[error("artifact storage failed")]
    Io(#[from] std::io::Error),
}

#[must_use]
pub fn artifact_route(base_url: &str) -> Option<String> {
    if base_url.len() > 2_048 || base_url.ends_with('/') {
        return None;
    }
    let parsed = Url::parse(base_url).ok()?;
    let path = parsed.path();
    let valid_path = path != "/"
        && path
            .strip_prefix('/')?
            .split('/')
            .all(|segment| !segment.is_empty() && segment.bytes().all(route_byte));
    (parsed.scheme() == "https"
        && parsed.host_str().is_some()
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.query().is_none()
        && parsed.fragment().is_none()
        && parsed.as_str() == base_url
        && valid_path)
        .then(|| path.to_owned())
}

const fn route_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

pub(crate) fn checked_reference(reference: ArtifactReference) -> Option<ArtifactReference> {
    let suffix = format!("/{}/report", reference.id);
    (format::valid_id(&reference.id)
        && reference
            .locator
            .strip_suffix(&suffix)
            .is_some_and(|base| artifact_route(base).is_some())
        && reference.expires_at_unix_millis >= 0
        && !(reference.external_incomplete && reference.external_tally.is_some())
        && reference.assessment_digest.is_some() == reference.external_tally.is_some())
    .then_some(reference)
}

pub(crate) fn reference_matches_report(
    reference: &ArtifactReference,
    report: Option<&[u8]>,
) -> bool {
    report.is_some_and(|bytes| amiss_wire::digest::sha256(bytes) == reference.report_digest)
}

pub(crate) fn evaluation_id(raw: &str) -> Result<ControllerEvaluationId, ArtifactError> {
    ControllerEvaluationId::new(raw.to_owned()).ok_or(ArtifactError::Corrupt)
}
