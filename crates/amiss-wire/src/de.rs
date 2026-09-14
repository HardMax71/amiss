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
    #[error("spelling is not canonical")]
    Noncanonical,
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

pub(crate) fn deserialize_error(
    base: &str,
    defect: &serde_path_to_error::Error<serde_json::Error>,
) -> Error {
    if defect.inner().is_syntax() || defect.inner().is_eof() {
        return Error::new(base, ErrorKind::Json(defect.to_string()));
    }
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

/// One bounded JSON document read under the closed-shape law: the byte
/// ceiling, then the typed shape with the failing path, then nothing may
/// follow the value.
///
/// # Errors
///
/// The document is over its ceiling, is not the shape, or carries trailing
/// bytes.
pub(crate) fn read<'de, T: serde::Deserialize<'de>>(
    bytes: &'de [u8],
    limit: u64,
) -> Result<T, Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
        return fail("$", ErrorKind::LimitExceeded);
    }
    let mut input = serde_json::Deserializer::from_slice(bytes);
    let document = serde_path_to_error::deserialize(&mut input)
        .map_err(|defect| deserialize_error("$", &defect))?;
    input
        .end()
        .map_err(|defect| Error::new("$", ErrorKind::Json(defect.to_string())))?;
    Ok(document)
}

/// A closed document without an envelope: the shape is its Serde derive, the
/// grammar is `validate`, and `parse` reads one under both.
pub trait Document: serde::de::DeserializeOwned + Sized {
    /// What a rejected document reports; `Error` unless a document carries
    /// more than a path and a kind.
    type Defect: From<Error>;
    /// The byte ceiling `parse` enforces; the caller's when unbounded here.
    const BYTES: u64 = u64::MAX;

    /// The document's own grammar beyond its shape.
    ///
    /// # Errors
    ///
    /// A field or set breaks the grammar.
    fn validate(&self) -> Result<(), Self::Defect>;

    /// Reads one document: ceiling, shape, trailing bytes, grammar.
    ///
    /// # Errors
    ///
    /// Any of those four refusals.
    fn parse(bytes: &[u8]) -> Result<Self, Self::Defect> {
        let document: Self = read(bytes, Self::BYTES)?;
        document.validate()?;
        Ok(document)
    }
}
