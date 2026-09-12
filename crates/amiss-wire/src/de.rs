use crate::{digest::Digest, json};

#[derive(Debug, thiserror::Error)]
#[error("{kind} at {path}")]
pub struct Error {
    pub path: String,
    #[source]
    pub kind: ErrorKind,
}

#[derive(Debug, thiserror::Error)]
pub enum ErrorKind {
    #[error("{0}")]
    Utf8(#[source] std::str::Utf8Error),
    #[error("{0}")]
    Json(#[source] json::Error),
    #[error("{0}")]
    Deserialize(#[from] serde_json::Error),
    #[error("{0}")]
    Canonical(#[from] crate::digest::CanonicalJsonError),
    #[error("value is invalid")]
    InvalidValue,
    #[error("set is not sorted")]
    UnsortedSet,
    #[error("set member is duplicated")]
    DuplicateMember,
    #[error("limit is exceeded")]
    LimitExceeded,
    #[error("digest does not match")]
    DigestMismatch,
    #[error("values are inconsistent")]
    Inconsistent,
}

impl Error {
    #[must_use]
    pub fn new(path: &str, kind: ErrorKind) -> Self {
        Self {
            path: path.to_owned(),
            kind,
        }
    }
}

/// # Errors
///
/// Always fails with the given kind at the given path.
pub fn fail<T>(path: &str, kind: ErrorKind) -> Result<T, Error> {
    Err(Error::new(path, kind))
}

/// Reads one complete JSON object and its domain-separated canonical digest.
///
/// Callers enforce byte ceilings and domain validation. Use closed Serde records;
/// canonical equality alone does not reject duplicate entries in maps.
///
/// # Errors
///
/// Fails on typed decoding or trailing content, or when serialization changes
/// the canonical input.
pub fn deserialize_json<T: serde::de::DeserializeOwned + serde::Serialize>(
    bytes: &[u8],
    domain: &str,
) -> Result<(T, Digest), Error> {
    let text =
        std::str::from_utf8(bytes).map_err(|error| Error::new("$", ErrorKind::Utf8(error)))?;
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let mut track = serde_path_to_error::Track::new();
    let document = crate::requests::object::deserialize(serde_path_to_error::Deserializer::new(
        &mut deserializer,
        &mut track,
    ))
    .and_then(|document| {
        deserializer.end()?;
        Ok(document)
    })
    .map_err(|source| {
        let path = track.path().to_string();
        Error {
            path: if path == "." {
                "$".to_owned()
            } else {
                format!("$.{path}")
            },
            kind: ErrorKind::Deserialize(source),
        }
    })?;
    let digest = crate::digest::verified_json_digest(domain, bytes, &document)
        .map_err(|source| Error::new("$", ErrorKind::Canonical(source)))?;
    Ok((document, digest))
}
