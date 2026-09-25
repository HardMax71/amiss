use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};
use wary::Validate;

use crate::de::{Document, Error, ErrorKind};
use crate::model::Digest;

use super::EXTERNAL_DOCUMENT_BYTES;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[validate(func = |_, evidence: &ExternalEvidence| {
    (evidence.rows.iter().collect::<BTreeSet<_>>().len() == evidence.rows.len())
        .then_some(())
        .ok_or_else(|| wary::Error::new("duplicate_evidence_row"))
})]
pub struct ExternalEvidence {
    pub schema: ExternalEvidenceSchema,
    pub plan_payload_digest: Digest,
    #[validate(dive)]
    pub producer: ExternalEvidenceProducer,
    #[validate(inner(dive))]
    pub rows: Vec<ExternalEvidenceRow>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum ExternalEvidenceSchema {
    #[strum(serialize = "amiss/external-evidence")]
    Current,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
pub struct ExternalEvidenceProducer {
    #[validate(length(chars, 1..))]
    pub name: String,
    #[validate(length(chars, 1..))]
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, wary::Wary)]
#[serde(tag = "kind")]
#[validate(func = |_, row: &ExternalEvidenceRow| {
    match row {
        ExternalEvidenceRow::HttpProbe {
            status,
            failure,
            final_destination,
            redirect_chain_permanent,
            ..
        } => (status.is_some() != failure.is_some()
            && redirect_chain_permanent
                .is_none_or(|permanent| permanent && final_destination.is_some()))
        .then_some(())
        .ok_or_else(|| wary::Error::new("invalid_http_probe_shape")),
        ExternalEvidenceRow::ForgeApi {
            repository, tail, ..
        } => (tail.is_none() || *repository == ForgeRepository::Readable)
            .then_some(())
            .ok_or_else(|| wary::Error::new("invalid_forge_evidence_shape")),
    }
})]
pub enum ExternalEvidenceRow {
    #[serde(rename = "http-probe")]
    HttpProbe {
        #[validate(length(chars, 1..=16_384))]
        destination: String,
        method: ProbeMethod,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[validate(range(100..=999))]
        status: Option<u16>,
        #[serde(skip_serializing_if = "Option::is_none")]
        failure: Option<ProbeFailure>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[validate(length(chars, 1..=16_384))]
        final_destination: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        redirect_chain_permanent: Option<bool>,
        #[validate(length(chars, 1..))]
        checked_at: String,
    },
    #[serde(rename = "forge-api")]
    ForgeApi {
        #[validate(length(chars, 1..=16_384))]
        destination: String,
        repository: ForgeRepository,
        #[serde(skip_serializing_if = "Option::is_none")]
        tail: Option<ForgeTail>,
        #[validate(length(chars, 1..))]
        checked_at: String,
    },
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "lowercase")]
pub enum ProbeMethod {
    Head,
    Get,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "lowercase")]
pub enum ProbeFailure {
    Dns,
    Tls,
    Timeout,
    Refused,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "lowercase")]
pub enum ForgeRepository {
    Readable,
    Missing,
    Denied,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Display,
    EnumString,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum ForgeTail {
    Resolved,
    PathMissing,
    RevisionMissing,
}

#[derive(Debug, thiserror::Error)]
pub enum EvidenceDefect {
    #[error(transparent)]
    Wire(Error),
    #[error("external evidence violates its contract: {0:?}")]
    Contract(wary::Report),
}

impl From<Error> for EvidenceDefect {
    fn from(defect: Error) -> Self {
        Self::Wire(defect)
    }
}

impl Document for ExternalEvidence {
    type Defect = EvidenceDefect;
    const BYTES: u64 = EXTERNAL_DOCUMENT_BYTES;

    fn validate(&self) -> Result<(), EvidenceDefect> {
        Validate::validate(self, &()).map_err(EvidenceDefect::Contract)
    }
}

/// Builds one validated external evidence document from its typed source.
///
/// # Errors
///
/// Fails when a public field violates the same grammar [`parse_evidence`]
/// enforces or the encoded document exceeds its byte ceiling.
pub fn evidence(input: &ExternalEvidence) -> Result<Vec<u8>, EvidenceDefect> {
    Document::validate(input)?;
    let canonical = serde_json_canonicalizer::to_vec(input)
        .map_err(|_defect| EvidenceDefect::Wire(Error::new("$", ErrorKind::InvalidValue)))?;
    if u64::try_from(canonical.len()).unwrap_or(u64::MAX) > EXTERNAL_DOCUMENT_BYTES {
        return Err(EvidenceDefect::Wire(Error::new(
            "$",
            ErrorKind::LimitExceeded,
        )));
    }
    Ok(canonical)
}
