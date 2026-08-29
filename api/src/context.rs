use std::cmp::Ordering;

use amiss_wire::de::{self, ErrorKind, Obj, fail};
use amiss_wire::digest::{Digest, hj};
use amiss_wire::json::Value;
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
    let digest = hj(DIGEST_DOMAIN, &value);
    let mut context = Obj::new("$", value).map_err(Error::Shape)?;
    context
        .required("schema", |path, value| de::const_str(path, value, SCHEMA))
        .map_err(Error::Shape)?;
    let name = context
        .required("name", |path, value| {
            let raw = bounded_text(path, value)?;
            ArtifactId::new(raw)
                .filter(|identity| identity.as_str().ends_with("/local-free-functions"))
                .ok_or_else(|| de::Error::new(path, ErrorKind::InvalidValue))
        })
        .map_err(Error::Shape)?;
    context
        .required("compiler", bounded_text)
        .map_err(Error::Shape)?;
    context
        .required("package", bounded_text)
        .map_err(Error::Shape)?;
    let target = context
        .required("target", bounded_text)
        .map_err(Error::Shape)?;
    let target_triple = context
        .required("target_triple", bounded_text)
        .map_err(Error::Shape)?;
    let rustdoc_format = context
        .required("rustdoc_format", |path, value| {
            u32::try_from(de::integer(path, value)?)
                .map_err(|_error| de::Error::new(path, ErrorKind::InvalidValue))
        })
        .map_err(Error::Shape)?;
    context
        .required("features", validate_sorted_texts)
        .map_err(Error::Shape)?;
    context
        .required("cfg", validate_sorted_texts)
        .map_err(Error::Shape)?;
    context
        .required("dependencies_digest", de::digest)
        .map_err(Error::Shape)?;
    context.finish().map_err(Error::Shape)?;
    Ok(Context {
        digest,
        name,
        rustdoc_format,
        target,
        target_triple,
    })
}

mod tests;

fn bounded_text(path: &str, value: Value) -> Result<String, de::Error> {
    let value = de::string(path, value)?;
    if !value.is_empty() && value.len() <= TEXT_BYTES && !value.chars().any(char::is_control) {
        Ok(value)
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}

fn validate_sorted_texts(path: &str, value: Value) -> Result<(), de::Error> {
    let values = de::array(path, value)?;
    if values.len() > SET_MEMBERS {
        return fail(path, ErrorKind::LimitExceeded);
    }
    let mut previous: Option<String> = None;
    for (index, value) in values.into_iter().enumerate() {
        let item_path = format!("{path}[{index}]");
        let current = bounded_text(&item_path, value)?;
        match previous.as_ref().map(|value| value.cmp(&current)) {
            Some(Ordering::Equal) => return fail(path, ErrorKind::DuplicateMember),
            Some(Ordering::Greater) => return fail(path, ErrorKind::UnsortedSet),
            None | Some(Ordering::Less) => previous = Some(current),
        }
    }
    Ok(())
}
