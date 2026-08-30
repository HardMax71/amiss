use std::cmp::Ordering;
use std::collections::BTreeMap;

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

/// Decodes one nonempty bounded string without control characters.
///
/// # Errors
///
/// Fails with `InvalidValue` when the text is empty, exceeds `limit` bytes, or contains a control
/// character, and with `WrongType` when the value is not a string.
pub(crate) fn bounded_text(path: &str, value: Value, limit: usize) -> Result<String, Error> {
    let value = string(path, value)?;
    if !value.is_empty() && value.len() <= limit && !value.chars().any(char::is_control) {
        Ok(value)
    } else {
        fail(path, ErrorKind::InvalidValue)
    }
}

pub(crate) fn sorted_map<T>(
    path: &str,
    value: Value,
    limit: usize,
    mut decode: impl FnMut(&str, Value) -> Result<(String, T), Error>,
) -> Result<BTreeMap<String, T>, Error> {
    Ok(sorted_items(
        path,
        value,
        limit,
        |path, value| decode(path, value),
        |row| &row.0,
    )?
    .into_iter()
    .collect())
}

pub(crate) fn sorted_items<T, K: Ord + ?Sized>(
    path: &str,
    value: Value,
    limit: usize,
    mut decode: impl FnMut(&str, Value) -> Result<T, Error>,
    key: impl Fn(&T) -> &K,
) -> Result<Vec<T>, Error> {
    let values = array(path, value)?;
    if values.len() > limit {
        return fail(path, ErrorKind::LimitExceeded);
    }
    let mut items: Vec<T> = Vec::with_capacity(values.len());
    for (index, value) in values.into_iter().enumerate() {
        let item = decode(&format!("{path}[{index}]"), value)?;
        match items.last().map(|previous| key(previous).cmp(key(&item))) {
            Some(Ordering::Equal) => return fail(path, ErrorKind::DuplicateMember),
            Some(Ordering::Greater) => return fail(path, ErrorKind::UnsortedSet),
            None | Some(Ordering::Less) => items.push(item),
        }
    }
    Ok(items)
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
