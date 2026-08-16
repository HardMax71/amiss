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

pub struct Obj {
    path: String,
    members: Vec<(String, Value)>,
}

impl Obj {
    /// # Errors
    ///
    /// Fails with `WrongType` when the value is not a JSON object.
    pub fn new(path: &str, value: Value) -> Result<Self, Error> {
        match value {
            Value::Object(members) => Ok(Self {
                path: path.to_owned(),
                members,
            }),
            Value::Null
            | Value::Bool(_)
            | Value::Integer(_)
            | Value::String(_)
            | Value::Array(_) => fail(path, ErrorKind::WrongType),
        }
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
        match self.members.iter().position(|(key, _)| key == name) {
            Some(index) => Ok(self.members.remove(index).1),
            None => fail(&self.field(name), ErrorKind::MissingField),
        }
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
        match self.members.into_iter().next() {
            None => Ok(()),
            Some((name, _)) => Err(Error {
                kind: ErrorKind::UnknownField,
                path: format!("{}.{name}", self.path),
            }),
        }
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
    match value {
        Value::String(s) => Ok(s),
        Value::Null | Value::Bool(_) | Value::Integer(_) | Value::Array(_) | Value::Object(_) => {
            fail(path, ErrorKind::WrongType)
        }
    }
}

/// # Errors
///
/// Fails with `WrongType` when the value is not an integer.
#[expect(
    clippy::needless_pass_by_value,
    reason = "uniform consuming decoder signature"
)]
pub fn integer(path: &str, value: Value) -> Result<i64, Error> {
    match value {
        Value::Integer(n) => Ok(n),
        Value::Null | Value::Bool(_) | Value::String(_) | Value::Array(_) | Value::Object(_) => {
            fail(path, ErrorKind::WrongType)
        }
    }
}

/// # Errors
///
/// Fails with `WrongType` when the value is not an array.
pub fn array(path: &str, value: Value) -> Result<Vec<Value>, Error> {
    match value {
        Value::Array(items) => Ok(items),
        Value::Null | Value::Bool(_) | Value::Integer(_) | Value::String(_) | Value::Object(_) => {
            fail(path, ErrorKind::WrongType)
        }
    }
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
    match value {
        Value::Null => None,
        other @ (Value::Bool(_)
        | Value::Integer(_)
        | Value::String(_)
        | Value::Array(_)
        | Value::Object(_)) => Some(other),
    }
}
