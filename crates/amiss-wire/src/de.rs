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
    Ok(string.into_string())
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
