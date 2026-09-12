mod string;
pub use string::string;

mod object;
pub use object::object;

mod tests;
mod tree;

use std::cmp::Ordering;
use std::fmt;
use std::marker::PhantomData;

use garde::Validate;
use serde::de::{DeserializeOwned, Unexpected};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_path_to_error::Segment;

use crate::de::{Error, ErrorKind, fail};
use crate::digest::{Digest, hj};
use crate::json;

pub const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;
pub const PAYLOAD: &str = "$.payload";

/// One closed wire document: its schema spellings, its byte ceiling, and the
/// laws between fields that no field can state alone.
pub trait Document {
    const PAYLOAD_SCHEMA: &'static str;
    const ENVELOPE_SCHEMA: &'static str;
    const LIMIT: u64;

    /// # Errors
    ///
    /// A law between fields is violated; the path is rooted at `root`.
    fn check(&self, _root: &str) -> Result<(), Error> {
        Ok(())
    }
}

/// The payload's fixed `schema` member, spelled by the document type itself.
pub struct Schema<T>(PhantomData<T>);

impl<T> Default for Schema<T> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<T> Clone for Schema<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Schema<T> {}

impl<T> PartialEq for Schema<T> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl<T> Eq for Schema<T> {}

impl<T: Document> fmt::Debug for Schema<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(T::PAYLOAD_SCHEMA)
    }
}

impl<T: Document> Serialize for Schema<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(T::PAYLOAD_SCHEMA)
    }
}

impl<'de, T: Document> Deserialize<'de> for Schema<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let spelling = String::deserialize(deserializer)?;
        if spelling == T::PAYLOAD_SCHEMA {
            Ok(Self(PhantomData))
        } else {
            Err(serde::de::Error::invalid_value(
                Unexpected::Str(&spelling),
                &T::PAYLOAD_SCHEMA,
            ))
        }
    }
}

/// The `schema`, `payload`, `payload_digest` form every sidecar document uses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope<T> {
    schema: String,
    pub payload: T,
    pub payload_digest: Digest,
}

impl<T: Document + Serialize + DeserializeOwned + Validate<Context = ()>> Envelope<T> {
    /// Seals one payload under its digest once every constraint and law holds.
    ///
    /// # Errors
    ///
    /// A field constraint or a law between fields is violated, or the
    /// canonical document exceeds the byte ceiling.
    pub fn seal(payload: T) -> Result<Self, Error> {
        let (_, payload_digest) = sealed_projection(&payload)?;
        Ok(Self {
            schema: T::ENVELOPE_SCHEMA.to_owned(),
            payload,
            payload_digest,
        })
    }

    /// Reads one bounded document and checks its constraints, laws, and digest.
    ///
    /// # Errors
    ///
    /// The bytes exceed the ceiling, are not strict JSON of the closed shape,
    /// violate a constraint or law, or carry a payload digest the payload does
    /// not reproduce.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > T::LIMIT {
            return fail("$", ErrorKind::LimitExceeded);
        }
        let value = json::strict_value(bytes).map_err(|(path, defect)| {
            Error::described(path, ErrorKind::Json(defect), defect.to_string())
        })?;
        Self::read_value(&value)
    }

    /// Validates a complete envelope already held as a JSON tree.
    ///
    /// # Errors
    ///
    /// The strict profile, shape, constraints, digest, or byte ceiling is invalid.
    pub fn from_value(value: &json::Value) -> Result<Self, Error> {
        json::check_profile(value).map_err(|defect| unrepresentable(&defect))?;
        if json::canonical_length(value) > T::LIMIT {
            return fail("$", ErrorKind::LimitExceeded);
        }
        Self::read_value(value)
    }

    fn read_value(value: &json::Value) -> Result<Self, Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields<'a> {
            schema: &'a str,
            payload: serde::de::IgnoredAny,
            payload_digest: Digest,
        }
        let received: Fields<'_> = borrow_value("$", value)?;
        let serde::de::IgnoredAny = received.payload;
        if received.schema != T::ENVELOPE_SCHEMA {
            return fail("$.schema", ErrorKind::InvalidValue);
        }
        let raw_payload = value
            .get("payload")
            .ok_or_else(|| Error::new(PAYLOAD, ErrorKind::MissingField))?;
        let received_digest = hj(T::PAYLOAD_SCHEMA, raw_payload);
        let payload: T = from_value(PAYLOAD, raw_payload)?;
        validate(&payload, PAYLOAD)?;
        if received_digest != received.payload_digest {
            return fail("$.payload_digest", ErrorKind::DigestMismatch);
        }
        Ok(Self {
            schema: received.schema.to_owned(),
            payload,
            payload_digest: received.payload_digest,
        })
    }

    /// Checks a constructed or changed envelope without parsing it again.
    ///
    /// # Errors
    ///
    /// The schema, constraints, digest, or byte ceiling is invalid.
    pub fn verify(&self, root: &str) -> Result<(), Error> {
        if self.schema != T::ENVELOPE_SCHEMA {
            return fail(&member(root, "schema"), ErrorKind::InvalidValue);
        }
        validate(&self.payload, &member(root, "payload"))?;
        let payload = to_value(&self.payload)?;
        if hj(T::PAYLOAD_SCHEMA, &payload) != self.payload_digest {
            return fail(&member(root, "payload_digest"), ErrorKind::DigestMismatch);
        }
        let envelope = envelope_value(&self.schema, payload, self.payload_digest)?;
        if json::canonical_length(&envelope) > T::LIMIT {
            return fail(root, ErrorKind::LimitExceeded);
        }
        Ok(())
    }
}

/// Moves an existing payload into a derived envelope without cloning the tree.
///
/// # Errors
///
/// The envelope cannot be represented as JSON.
pub fn envelope_value(
    schema: &str,
    payload: json::Value,
    payload_digest: Digest,
) -> Result<json::Value, Error> {
    let mut envelope = to_value(&Envelope {
        schema: schema.to_owned(),
        payload: (),
        payload_digest,
    })?;
    let slot = envelope
        .get_mut("payload")
        .ok_or_else(|| Error::new("$.payload", ErrorKind::Inconsistent))?;
    *slot = payload;
    json::check_profile(&envelope).map_err(|defect| unrepresentable(&defect))?;
    Ok(envelope)
}

/// Seals a borrowed payload directly into its wire tree.
///
/// # Errors
///
/// The payload violates its constraints, laws, or document ceiling.
pub fn seal_value<T: Document + Serialize + Validate<Context = ()>>(
    payload: &T,
) -> Result<json::Value, Error> {
    sealed_projection(payload).map(|(value, _digest)| value)
}

fn sealed_projection<T: Document + Serialize + Validate<Context = ()>>(
    payload: &T,
) -> Result<(json::Value, Digest), Error> {
    validate(payload, PAYLOAD)?;
    let payload = to_value(payload)?;
    let payload_digest = hj(T::PAYLOAD_SCHEMA, &payload);
    let envelope = envelope_value(T::ENVELOPE_SCHEMA, payload, payload_digest)?;
    if json::canonical_length(&envelope) > T::LIMIT {
        return fail("$", ErrorKind::LimitExceeded);
    }
    Ok((envelope, payload_digest))
}

fn validate<T: Document + Validate<Context = ()>>(payload: &T, root: &str) -> Result<(), Error> {
    constrained(payload, root)?;
    payload.check(root)
}

/// Checks every field constraint of one value, rooting paths at `root`.
///
/// # Errors
///
/// A field constraint is violated.
pub fn constrained<T: Validate<Context = ()>>(value: &T, root: &str) -> Result<(), Error> {
    value
        .validate()
        .map_err(|report| constraint_error(root, &report))
}

/// Deserializes one strict JSON subtree, rooting shape errors at `path`.
///
/// # Errors
///
/// The subtree violates the Amiss profile or does not match the requested shape.
pub fn from_value<T: DeserializeOwned>(path: &str, value: &json::Value) -> Result<T, Error> {
    check_value(path, value)?;
    borrow_value(path, value)
}

pub(crate) fn check_value(path: &str, value: &json::Value) -> Result<(), Error> {
    json::check_profile(value).map_err(|defect| {
        Error::described(
            path.to_owned(),
            ErrorKind::InvalidValue,
            bare_message(&defect),
        )
    })
}

/// Deserializes borrowed fields from a JSON tree without cloning it.
///
/// # Errors
///
/// The tree does not match the requested shape.
pub fn borrow_value<'de, T: Deserialize<'de>>(
    path: &str,
    value: &'de json::Value,
) -> Result<T, Error> {
    deserialize_tree(path, value)
}

/// Deserializes a JSON tree by consuming its fields without cloning them.
///
/// # Errors
///
/// The tree does not match the requested shape.
pub fn owned_value<T: DeserializeOwned>(path: &str, value: serde_json::Value) -> Result<T, Error> {
    deserialize_tree(path, value)
}

fn deserialize_tree<'de, T, D>(path: &str, value: D) -> Result<T, Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de, Error = serde_json::Error>,
{
    serde_path_to_error::deserialize(tree::Tree(value)).map_err(|defect| {
        let rendered = render_path(defect.path());
        let rooted = rendered
            .strip_prefix('$')
            .map_or_else(|| path.to_owned(), |rest| format!("{path}{rest}"));
        data_error(&[], rooted, &defect.into_inner())
    })
}

/// Projects a serializable wire value into a JSON tree.
///
/// # Errors
///
/// Serialization or the strict numeric profile is invalid.
pub fn to_value<T: Serialize + ?Sized>(value: &T) -> Result<json::Value, Error> {
    json::check_profile(value).map_err(|defect| unrepresentable(&defect))?;
    let value = serde_json::to_value(value).map_err(|defect| unrepresentable(&defect))?;
    Ok(value)
}

/// The RFC 8785 bytes of any wire value.
///
/// # Errors
///
/// The value violates the integer-only profile, nesting bound, or string-key requirement.
pub fn canonical<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, Error> {
    Ok(json::canonical(&to_value(value)?))
}

/// The domain-separated digest over the canonical bytes of any wire value.
///
/// # Errors
///
/// The value cannot be canonicalized.
pub fn digest<T: Serialize + ?Sized>(domain: &str, value: &T) -> Result<Digest, Error> {
    Ok(hj(domain, &to_value(value)?))
}

/// Reads one complete strict JSON value into its closed shape.
///
/// # Errors
///
/// The bytes are not UTF-8, not one JSON value, or not the closed shape.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    let value = json::strict_value(bytes).map_err(|(path, defect)| {
        Error::described(path, ErrorKind::Json(defect), defect.to_string())
    })?;
    owned_value("$", value)
}

/// One garde rule outcome: a refusal carrying `message`, or nothing.
///
/// # Errors
///
/// `valid` is false.
pub fn rule(valid: bool, message: &'static str) -> garde::Result {
    if valid {
        Ok(())
    } else {
        Err(garde::Error::new(message))
    }
}

/// A required member that may be null, never absent.
///
/// # Errors
///
/// The present value is neither null nor a `T`.
pub fn nullable<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(deserializer)
}

/// A present optional member must contain a value, never null.
///
/// # Errors
///
/// The member is null or does not match `T`.
pub fn non_null<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

fn unrepresentable(defect: &serde_json::Error) -> Error {
    Error::described(
        "$".to_owned(),
        ErrorKind::InvalidValue,
        bare_message(defect),
    )
}

fn data_error(bytes: &[u8], path: String, defect: &serde_json::Error) -> Error {
    let message = bare_message(defect);
    let (path, kind) = if let Some(field) = quoted_field(&message, "missing field `") {
        (member(&path, field), ErrorKind::MissingField)
    } else if let Some(field) = quoted_field(&message, "unknown field `") {
        let suffix = format!(".{field}");
        let path = if path.ends_with(&suffix) {
            path
        } else {
            member(&path, field)
        };
        (path, ErrorKind::UnknownField)
    } else if let Some(field) = quoted_field(&message, "duplicate field `") {
        let duplicate = json::Error {
            kind: json::ErrorKind::DuplicateKey,
            offset: offset(bytes, defect.line(), defect.column()),
        };
        (member(&path, field), ErrorKind::Json(duplicate))
    } else if message.starts_with("invalid type") {
        (path, ErrorKind::WrongType)
    } else {
        let kind = [
            ErrorKind::UnsortedSet,
            ErrorKind::DuplicateMember,
            ErrorKind::LimitExceeded,
            ErrorKind::Inconsistent,
        ]
        .into_iter()
        .find(|kind| kind.to_string() == message)
        .unwrap_or(ErrorKind::InvalidValue);
        (path, kind)
    };
    Error::described(path, kind, message)
}

fn constraint_error(root: &str, report: &garde::Report) -> Error {
    let Some((path, defect)) = report.iter().next() else {
        return Error::new(root, ErrorKind::InvalidValue);
    };
    let message = defect.message().to_owned();
    let kind = [
        ErrorKind::UnsortedSet,
        ErrorKind::DuplicateMember,
        ErrorKind::LimitExceeded,
        ErrorKind::Inconsistent,
    ]
    .into_iter()
    .find(|kind| kind.to_string() == message)
    .unwrap_or(ErrorKind::InvalidValue);
    let rendered = path.to_string();
    let path = if rendered.is_empty() {
        root.to_owned()
    } else {
        member(root, &rendered)
    };
    Error::described(path, kind, message)
}

fn render_path(path: &serde_path_to_error::Path) -> String {
    let mut rendered = String::from("$");
    for segment in path {
        match segment {
            Segment::Seq { index } => {
                rendered.push('[');
                rendered.push_str(&index.to_string());
                rendered.push(']');
            }
            Segment::Map { key } => {
                rendered.push('.');
                rendered.push_str(key);
            }
            Segment::Enum { .. } | Segment::Unknown => {}
        }
    }
    rendered
}

fn quoted_field<'a>(message: &'a str, prefix: &str) -> Option<&'a str> {
    message.strip_prefix(prefix)?.split('`').next()
}

fn member(path: &str, name: &str) -> String {
    format!("{path}.{name}")
}

fn bare_message(defect: &serde_json::Error) -> String {
    let mut text = defect.to_string();
    if let Some(cut) = text.rfind(" at line ") {
        text.truncate(cut);
    }
    text
}

fn offset(bytes: &[u8], line: usize, column: usize) -> usize {
    let line_start = bytes
        .split(|&byte| byte == b'\n')
        .take(line.saturating_sub(1))
        .fold(0_usize, |start, row| {
            start.saturating_add(row.len()).saturating_add(1)
        });
    line_start
        .saturating_add(column.saturating_sub(1))
        .min(bytes.len())
}

/// Orders two role identities the way every two-subject document requires.
///
/// # Errors
///
/// The roles are equal or reversed; the path names the subjects array.
pub fn sorted_roles<T: Ord>(root: &str, left: &T, right: &T) -> Result<(), Error> {
    match left.cmp(right) {
        Ordering::Less => Ok(()),
        Ordering::Equal => fail(&member(root, "subjects"), ErrorKind::DuplicateMember),
        Ordering::Greater => fail(&member(root, "subjects"), ErrorKind::UnsortedSet),
    }
}
