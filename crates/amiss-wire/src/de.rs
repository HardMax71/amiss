mod profile;

pub use profile::JsonProfile;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{kind} at {path}")]
pub struct Error {
    pub path: String,
    pub kind: ErrorKind,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ErrorKind {
    #[error("{0}")]
    Json(String),
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

pub(crate) fn deserialize_error<E: std::fmt::Display>(
    base: &str,
    defect: &serde_path_to_error::Error<E>,
) -> Error {
    let message = defect.inner().to_string();
    let (kind, member) = if let Some(member) = message
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
