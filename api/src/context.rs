use std::cmp::Ordering;

use amiss_wire::codec;
use amiss_wire::de::{self, ErrorKind, fail};
use amiss_wire::digest::Digest;
use amiss_wire::model::ArtifactId;

pub(crate) const BYTES: u64 = 65_536;
const SCHEMA: &str = "amiss/rust-public-api-context";
const DIGEST_DOMAIN: &str = "amiss/rust-public-api-context-v1";
const TEXT_BYTES: usize = 4_096;
const SET_MEMBERS: usize = 1_024;

pub(crate) struct Context {
    pub digest: Digest,
    pub name: ArtifactId,
    pub rustdoc_format: u32,
    pub target: String,
    pub target_triple: String,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("the producer context exceeds its byte ceiling")]
    Bytes,
    #[error("the producer context is not strict JSON")]
    Json(#[source] amiss_wire::json::Error),
    #[error("the producer context is invalid")]
    Shape(#[source] de::Error),
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Context, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > BYTES {
        return Err(Error::Bytes);
    }
    let value = amiss_wire::json::parse(bytes).map_err(Error::Json)?;
    let digest = codec::digest(DIGEST_DOMAIN, &value).map_err(Error::Shape)?;
    let context: Document = codec::from_value("$", &value).map_err(Error::Shape)?;
    context.validate().map_err(Error::Shape)?;
    Ok(Context {
        digest,
        name: context.name,
        rustdoc_format: context.rustdoc_format,
        target: context.target,
        target_triple: context.target_triple,
    })
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema: String,
    name: ArtifactId,
    compiler: String,
    package: String,
    target: String,
    target_triple: String,
    rustdoc_format: u32,
    features: Vec<String>,
    cfg: Vec<String>,
    #[serde(rename = "dependencies_digest")]
    _dependencies_digest: Digest,
}

impl Document {
    fn validate(&self) -> Result<(), de::Error> {
        if self.schema != SCHEMA {
            return fail("$.schema", ErrorKind::InvalidValue);
        }
        if !self.name.as_str().ends_with("/local-function-declarations") {
            return fail("$.name", ErrorKind::InvalidValue);
        }
        for (name, value) in [
            ("name", self.name.as_str()),
            ("compiler", &self.compiler),
            ("package", &self.package),
            ("target", &self.target),
            ("target_triple", &self.target_triple),
        ] {
            bounded_text(&format!("$.{name}"), value)?;
        }
        validate_sorted_texts("$.features", &self.features)?;
        validate_sorted_texts("$.cfg", &self.cfg)?;
        Ok(())
    }
}

mod tests;

fn bounded_text(path: &str, value: &str) -> Result<(), de::Error> {
    if !value.is_empty() && value.len() <= TEXT_BYTES && !value.chars().any(char::is_control) {
        Ok(())
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}

fn validate_sorted_texts(path: &str, values: &[String]) -> Result<(), de::Error> {
    if values.len() > SET_MEMBERS {
        return fail(path, ErrorKind::LimitExceeded);
    }
    let mut previous: Option<&String> = None;
    for (index, current) in values.iter().enumerate() {
        bounded_text(&format!("{path}[{index}]"), current)?;
        match previous.map(|value| value.cmp(current)) {
            Some(Ordering::Equal) => return fail(path, ErrorKind::DuplicateMember),
            Some(Ordering::Greater) => return fail(path, ErrorKind::UnsortedSet),
            None | Some(Ordering::Less) => previous = Some(current),
        }
    }
    Ok(())
}
