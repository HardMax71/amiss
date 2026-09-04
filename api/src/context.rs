use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::ArtifactId;
use serde::{Deserialize, Serialize};
use wary::Validate as _;

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
    #[error("the producer context is not strict JSON")]
    Json(#[source] amiss_wire::json::Error),
    #[error("the producer context is invalid")]
    Shape(#[source] serde_json::Error),
    #[error("the producer context is invalid")]
    Contract(#[source] wary::Report),
}

pub(crate) fn parse(bytes: &[u8]) -> Result<(Context, Digest), Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > BYTES {
        return Err(Error::Bytes);
    }
    amiss_wire::json::parse(bytes).map_err(Error::Json)?;
    let context: Context = serde_json::from_slice(bytes).map_err(Error::Shape)?;
    context.validate(&()).map_err(Error::Contract)?;
    let digest = serde_json::to_vec(&context)
        .map(|canonical| hb(DIGEST_DOMAIN, &canonical))
        .map_err(Error::Shape)?;
    Ok((context, digest))
}

mod tests;
