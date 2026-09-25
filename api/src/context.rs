use amiss_wire::de::{self, Document, ErrorKind};
use amiss_wire::envelope::document_digest;
use amiss_wire::model::ArtifactId;
use amiss_wire::model::Digest;
use serde::{Deserialize, Serialize};

pub(crate) const BYTES: u64 = 65_536;
const DIGEST_DOMAIN: &str = "amiss/rust-public-api-context-v1";
const TEXT_BYTES: usize = 4_096;
const SET_MEMBERS: usize = 1_024;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, wary::Wary)]
#[serde(deny_unknown_fields)]
#[validate(func = |_, context: &Context| {
    let sets = [&context.cfg, &context.features];
    sets.iter()
        .all(|values| values.iter().is_sorted_by(|left, right| left < right))
        .then_some(())
        .ok_or_else(|| wary::Error::new("context_sets_not_sorted_unique"))?;
    sets.into_iter()
        .flatten()
        .chain([&context.compiler, &context.package, &context.target, &context.target_triple])
        .all(|value| !value.chars().any(char::is_control))
        .then_some(())
        .ok_or_else(|| wary::Error::new("context_text_has_controls"))?;
    context.name.as_str().ends_with("/local-function-declarations")
        .then_some(())
        .ok_or_else(|| wary::Error::new("unscoped_context_name"))
})]
pub(crate) struct Context {
    #[validate(length(..=SET_MEMBERS), inner(length(bytes, 1..=TEXT_BYTES)))]
    pub cfg: Vec<String>,
    #[validate(length(bytes, 1..=TEXT_BYTES))]
    pub compiler: String,
    pub dependencies_digest: Digest,
    #[validate(length(..=SET_MEMBERS), inner(length(bytes, 1..=TEXT_BYTES)))]
    pub features: Vec<String>,
    pub name: ArtifactId,
    #[validate(length(bytes, 1..=TEXT_BYTES))]
    pub package: String,
    pub rustdoc_format: u32,
    pub schema: ContextSchema,
    #[validate(length(bytes, 1..=TEXT_BYTES))]
    pub target: String,
    #[validate(length(bytes, 1..=TEXT_BYTES))]
    pub target_triple: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ContextSchema {
    #[serde(rename = "amiss/rust-public-api-context")]
    Current,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("the producer context exceeds its byte ceiling")]
    Bytes,
    #[error("the producer context is invalid")]
    Shape(#[source] de::Error),
    #[error("the producer context is invalid")]
    Contract(#[source] wary::Report),
}

impl From<de::Error> for Error {
    fn from(error: de::Error) -> Self {
        match error.kind {
            ErrorKind::LimitExceeded => Self::Bytes,
            ErrorKind::Json(_)
            | ErrorKind::MissingField
            | ErrorKind::UnknownField
            | ErrorKind::DuplicateKey
            | ErrorKind::WrongType
            | ErrorKind::InvalidValue
            | ErrorKind::UnsortedSet
            | ErrorKind::DuplicateMember
            | ErrorKind::DigestMismatch
            | ErrorKind::Inconsistent
            | ErrorKind::Noncanonical => Self::Shape(error),
        }
    }
}

impl Document for Context {
    type Defect = Error;
    const BYTES: u64 = BYTES;

    fn validate(&self) -> Result<(), Error> {
        wary::Validate::validate(self, &()).map_err(Error::Contract)
    }
}

pub(crate) fn digest(context: &Context) -> Result<Digest, Error> {
    document_digest(DIGEST_DOMAIN, context)
        .ok_or_else(|| Error::Shape(de::Error::new("$", ErrorKind::InvalidValue)))
}

mod tests;
