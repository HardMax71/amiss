use crate::{digest::Digest, json};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{kind} at {path}")]
pub struct Error {
    pub path: String,
    #[source]
    pub kind: ErrorKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ErrorKind {
    #[error("{0}")]
    Utf8(#[source] std::str::Utf8Error),
    #[error("{0}")]
    Json(json::Error),
    #[error("{category:?} JSON error at line {line} column {column}")]
    Deserialize {
        category: serde_json::error::Category,
        line: usize,
        column: usize,
    },
    #[error("required field is missing")]
    MissingField,
    #[error("field is unknown")]
    UnknownField,
    #[error("value has the wrong type")]
    WrongType,
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
    .map_err(|error| {
        deserialize_error("$", &serde_path_to_error::Error::new(track.path(), error))
    })?;
    let digest = crate::digest::verified_json_digest(domain, bytes, &document)
        .map_err(|_defect| Error::new("$", ErrorKind::InvalidValue))?;
    Ok((document, digest))
}

pub(crate) fn deserialize_error(
    base: &str,
    defect: &serde_path_to_error::Error<serde_json::Error>,
) -> Error {
    let error = defect.inner();
    let message = error.to_string();
    let (kind, member) = if !error.is_data() {
        (
            ErrorKind::Deserialize {
                category: error.classify(),
                line: error.line(),
                column: error.column(),
            },
            None,
        )
    } else if let Some(member) = message
        .strip_prefix("missing field `")
        .and_then(|rest| rest.split_once('`').map(|(member, _rest)| member))
    {
        (ErrorKind::MissingField, Some(member))
    } else if let Some(member) = message
        .strip_prefix("unknown field `")
        .and_then(|rest| rest.split_once('`').map(|(member, _rest)| member))
    {
        (ErrorKind::UnknownField, Some(member))
    } else if message.starts_with("invalid type:") {
        (ErrorKind::WrongType, None)
    } else {
        (ErrorKind::InvalidValue, None)
    };
    let raw_path = defect.path().to_string();
    let mut path = if raw_path == "." {
        base.to_owned()
    } else {
        format!("{base}.{raw_path}")
    };
    if let Some(member) = member
        && defect
            .path()
            .iter()
            .next_back()
            .map(ToString::to_string)
            .as_deref()
            != Some(member)
    {
        path.push('.');
        path.push_str(member);
    }
    Error { path, kind }
}
