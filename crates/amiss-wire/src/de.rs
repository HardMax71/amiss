use crate::digest::Digest;
use crate::json::{self, Value};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{kind} at {path}")]
pub struct Error {
    pub path: String,
    pub kind: ErrorKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ErrorKind {
    #[error("{0}")]
    Json(json::Error),
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

pub(crate) fn deserialize_json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    serde_path_to_error::deserialize(&mut deserializer)
        .map_err(|defect| deserialize_error("$", &defect))
}

/// Deserializes one already strict JSON value while retaining the caller's document path.
///
/// # Errors
///
/// Fails with the structural error and its exact nested path.
pub fn deserialize_value<T: serde::de::DeserializeOwned>(
    path: &str,
    value: serde_json::Value,
) -> Result<T, Error> {
    serde_path_to_error::deserialize(value).map_err(|defect| deserialize_error(path, &defect))
}

fn deserialize_error<E: std::fmt::Display>(
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

pub struct Obj {
    path: String,
    members: Vec<(String, Value)>,
}

impl Obj {
    /// # Errors
    ///
    /// Fails with `WrongType` when the value is not a JSON object.
    pub fn new(path: &str, value: Value) -> Result<Self, Error> {
        let Value::Object(members) = value else {
            return fail(path, ErrorKind::WrongType);
        };
        Ok(Self {
            path: path.to_owned(),
            members: members.into_vec(),
        })
    }

    #[must_use]
    pub fn field(&self, name: &str) -> String {
        format!("{}.{name}", self.path)
    }

    /// An optional member: present or absent, never defaulted from null.
    pub fn take_optional(&mut self, name: &str) -> Option<Value> {
        self.members
            .iter()
            .position(|(key, _)| key == name)
            .map(|index| self.members.remove(index).1)
    }

    /// # Errors
    ///
    /// Fails with `MissingField` when the member is absent.
    pub fn take(&mut self, name: &str) -> Result<Value, Error> {
        let index = self
            .members
            .iter()
            .position(|(key, _)| key == name)
            .ok_or_else(|| Error::new(&self.field(name), ErrorKind::MissingField))?;
        Ok(self.members.remove(index).1)
    }

    /// Removes and decodes one required member.
    ///
    /// # Errors
    ///
    /// Fails with `MissingField` when the member is absent, or returns the
    /// decoder's error at the member path.
    pub fn required<T>(
        &mut self,
        name: &str,
        decode: impl FnOnce(&str, Value) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let value = self.take(name)?;
        self.decode_member(name, value, decode)
    }

    /// # Errors
    ///
    /// Fails with `UnknownField` at the first leftover member.
    pub fn finish(self) -> Result<(), Error> {
        let Some((name, _)) = self.members.into_iter().next() else {
            return Ok(());
        };
        Err(Error {
            kind: ErrorKind::UnknownField,
            path: format!("{}.{name}", self.path),
        })
    }

    fn decode_member<T>(
        &mut self,
        name: &str,
        value: Value,
        decode: impl FnOnce(&str, Value) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let parent_length = self.path.len();
        self.path.push('.');
        self.path.push_str(name);
        let decoded = decode(&self.path, value);
        self.path.truncate(parent_length);
        decoded
    }
}

/// # Errors
///
/// Fails with `WrongType` when the value is not a string.
pub fn string(path: &str, value: Value) -> Result<String, Error> {
    let Value::String(string) = value else {
        return fail(path, ErrorKind::WrongType);
    };
    Ok(string.into())
}

/// # Errors
///
/// Fails with `WrongType` when the value is not a boolean.
#[expect(
    clippy::needless_pass_by_value,
    reason = "uniform consuming decoder signature"
)]
pub fn boolean(path: &str, value: Value) -> Result<bool, Error> {
    let Value::Bool(boolean) = value else {
        return fail(path, ErrorKind::WrongType);
    };
    Ok(boolean)
}

/// # Errors
///
/// Fails with `InvalidValue` when the value is not the canonical SHA-256 wire form.
pub fn digest(path: &str, value: Value) -> Result<Digest, Error> {
    Digest::from_wire(&string(path, value)?)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}

/// # Errors
///
/// Fails with `WrongType` when the value is not an integer.
#[expect(
    clippy::needless_pass_by_value,
    reason = "uniform consuming decoder signature"
)]
pub fn integer(path: &str, value: Value) -> Result<i64, Error> {
    let Value::Integer(integer) = value else {
        return fail(path, ErrorKind::WrongType);
    };
    Ok(integer)
}

/// # Errors
///
/// Fails with `WrongType` when the value is not an array.
pub fn array(path: &str, value: Value) -> Result<Vec<Value>, Error> {
    let Value::Array(items) = value else {
        return fail(path, ErrorKind::WrongType);
    };
    Ok(items.into_vec())
}

/// # Errors
///
/// Fails with `InvalidValue` when the string differs from `expected`.
pub fn const_str(path: &str, value: Value, expected: &str) -> Result<(), Error> {
    if string(path, value)? == expected {
        Ok(())
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}

#[must_use]
pub fn nullable(value: Value) -> Option<Value> {
    (!matches!(&value, Value::Null)).then_some(value)
}
