mod tests;

use std::cmp::Ordering;
use std::fmt;
use std::marker::PhantomData;

use garde::Validate;
use serde::de::{DeserializeOwned, Unexpected};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::error::Category;
use serde_path_to_error::Segment;

use crate::de::{Error, ErrorKind, fail};
use crate::digest::{Digest, hb};
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
        validate(&payload, PAYLOAD)?;
        let payload_digest = digest(T::PAYLOAD_SCHEMA, &payload)?;
        let envelope = Self {
            schema: T::ENVELOPE_SCHEMA.to_owned(),
            payload,
            payload_digest,
        };
        if u64::try_from(canonical(&envelope)?.len()).unwrap_or(u64::MAX) > T::LIMIT {
            return fail("$", ErrorKind::LimitExceeded);
        }
        Ok(envelope)
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
        let envelope: Self = decode(bytes)?;
        if envelope.schema != T::ENVELOPE_SCHEMA {
            return fail("$.schema", ErrorKind::InvalidValue);
        }
        validate(&envelope.payload, PAYLOAD)?;
        if digest(T::PAYLOAD_SCHEMA, &envelope.payload)? != envelope.payload_digest {
            return fail("$.payload_digest", ErrorKind::DigestMismatch);
        }
        Ok(envelope)
    }
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

/// Reads one subtree of a hand-parsed value into its closed shape, rooting
/// error paths at `path`.
///
/// # Errors
///
/// The subtree is not the closed shape or violates a field constraint.
pub fn from_value<T: DeserializeOwned + Validate<Context = ()>>(
    path: &str,
    value: &json::Value,
) -> Result<T, Error> {
    let decoded: T = decode(&json::canonical(value)).map_err(|mut error| {
        if let Some(rerooted) = error
            .path
            .strip_prefix('$')
            .map(|rest| format!("{path}{rest}"))
        {
            error.path = rerooted;
        }
        error
    })?;
    constrained(&decoded, path)?;
    Ok(decoded)
}

/// The hand-codec value of any wire value, for the writers not yet moved.
///
/// # Errors
///
/// The value cannot be canonicalized.
pub fn to_value<T: Serialize + ?Sized>(value: &T) -> Result<json::Value, Error> {
    json::parse(&canonical(value)?).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))
}

/// The RFC 8785 bytes of any wire value.
///
/// # Errors
///
/// The value holds a map keyed by something other than strings.
pub fn canonical<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, Error> {
    let sorted = serde_json::to_value(value).map_err(|defect| unrepresentable(&defect))?;
    serde_json::to_vec(&sorted).map_err(|defect| unrepresentable(&defect))
}

/// The domain-separated digest over the canonical bytes of any wire value.
///
/// # Errors
///
/// The value cannot be canonicalized.
pub fn digest<T: Serialize + ?Sized>(domain: &str, value: &T) -> Result<Digest, Error> {
    Ok(hb(domain, &canonical(value)?))
}

/// Reads one complete strict JSON value into its closed shape.
///
/// # Errors
///
/// The bytes are not UTF-8, not one JSON value, or not the closed shape.
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    if let Err(defect) = std::str::from_utf8(bytes) {
        return Err(json_error(
            json::ErrorKind::InvalidUtf8,
            defect.valid_up_to(),
        ));
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let value = serde_path_to_error::deserialize(&mut deserializer)
        .map_err(|defect| shape_error(bytes, defect))?;
    deserializer
        .end()
        .map_err(|defect| syntax_error(bytes, &defect))?;
    Ok(value)
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

fn json_error(kind: json::ErrorKind, offset: usize) -> Error {
    Error::new("$", ErrorKind::Json(json::Error { kind, offset }))
}

fn unrepresentable(defect: &serde_json::Error) -> Error {
    Error::described(
        "$".to_owned(),
        ErrorKind::InvalidValue,
        bare_message(defect),
    )
}

fn shape_error(bytes: &[u8], defect: serde_path_to_error::Error<serde_json::Error>) -> Error {
    let path = render_path(defect.path());
    let inner = defect.into_inner();
    match inner.classify() {
        Category::Data => data_error(bytes, path, &inner),
        Category::Io | Category::Syntax | Category::Eof => syntax_error(bytes, &inner),
    }
}

fn data_error(bytes: &[u8], path: String, defect: &serde_json::Error) -> Error {
    let message = bare_message(defect);
    let (path, kind) = if let Some(field) = quoted_field(&message, "missing field `") {
        (member(&path, field), ErrorKind::MissingField)
    } else if message.starts_with("unknown field `") {
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
        (path, ErrorKind::InvalidValue)
    };
    Error::described(path, kind, message)
}

fn syntax_error(bytes: &[u8], defect: &serde_json::Error) -> Error {
    let message = bare_message(defect);
    let syntax = json::Error {
        kind: syntax_kind(&message),
        offset: offset(bytes, defect.line(), defect.column()),
    };
    Error::described("$".to_owned(), ErrorKind::Json(syntax), message)
}

fn syntax_kind(message: &str) -> json::ErrorKind {
    [
        ("EOF while parsing", json::ErrorKind::UnexpectedEnd),
        ("trailing characters", json::ErrorKind::TrailingContent),
        ("recursion limit exceeded", json::ErrorKind::DepthLimit),
        ("control character", json::ErrorKind::ControlCharacter),
        ("invalid escape", json::ErrorKind::InvalidEscape),
        (
            "unexpected end of hex escape",
            json::ErrorKind::InvalidEscape,
        ),
        ("lone leading surrogate", json::ErrorKind::LoneSurrogate),
        ("invalid unicode code point", json::ErrorKind::LoneSurrogate),
    ]
    .into_iter()
    .find(|(prefix, _)| message.starts_with(prefix))
    .map_or(json::ErrorKind::UnexpectedByte, |(_, kind)| kind)
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
    message.strip_prefix(prefix)?.strip_suffix('`')
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
