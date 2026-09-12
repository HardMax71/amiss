use std::cell::Cell;
use std::fmt;

use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};

const MAX_DEPTH: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ErrorKind {
    #[error("invalid UTF-8")]
    InvalidUtf8,
    #[error("byte-order mark is forbidden")]
    ByteOrderMark,
    #[error("unexpected end of input")]
    UnexpectedEnd,
    #[error("unexpected byte")]
    UnexpectedByte,
    #[error("trailing content")]
    TrailingContent,
    #[error("nesting limit exceeded")]
    DepthLimit,
    #[error("duplicate object key")]
    DuplicateKey,
    #[error("unescaped control character")]
    ControlCharacter,
    #[error("invalid escape")]
    InvalidEscape,
    #[error("lone UTF-16 surrogate")]
    LoneSurrogate,
    #[error("negative zero is forbidden")]
    NegativeZero,
    #[error("fractions and exponents are forbidden")]
    FractionOrExponent,
    #[error("integer is outside the safe range")]
    IntegerOutOfRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{kind} at byte {offset}")]
pub struct Error {
    pub kind: ErrorKind,
    pub offset: usize,
}

/// Reads exactly one JSON value under the integer-only Amiss profile.
///
/// # Errors
///
/// JSON syntax, duplicate keys, unsafe numbers, or excess nesting are refused.
pub fn parse(bytes: &[u8]) -> Result<Value, Error> {
    decode(bytes).map_err(|(_, error)| error)
}

/// Reads an upstream JSON document with duplicate and nesting checks.
///
/// # Errors
///
/// Invalid syntax, duplicate keys, or more than 512 containers are refused.
pub fn parse_upstream(bytes: &[u8]) -> Result<Value, Error> {
    decode_profile(bytes, false).map_err(|(_, error)| error)
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Value, (String, Error)> {
    decode_profile(bytes, true)
}

fn decode_profile(bytes: &[u8], integers_only: bool) -> Result<Value, (String, Error)> {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Err((
            "$".to_owned(),
            Error {
                kind: ErrorKind::ByteOrderMark,
                offset: 0,
            },
        ));
    }
    std::str::from_utf8(bytes).map_err(|defect| {
        (
            "$".to_owned(),
            Error {
                kind: ErrorKind::InvalidUtf8,
                offset: defect.valid_up_to(),
            },
        )
    })?;
    let violation = Cell::new(None);
    let root = Location::Root;
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    deserializer.disable_recursion_limit();
    let parsed = Seed {
        depth: 0,
        location: &root,
        violation: &violation,
        integers_only,
    }
    .deserialize(&mut deserializer)
    .and_then(|value| deserializer.end().map(|()| value));
    parsed.map_err(|defect| {
        let (path, kind) = violation
            .take()
            .unwrap_or_else(|| ("$".to_owned(), syntax_kind(&defect)));
        (
            path,
            Error {
                kind,
                offset: offset(bytes, defect.line(), defect.column()),
            },
        )
    })
}

enum Location<'a> {
    Root,
    Member(&'a Self, &'a str),
    Index(&'a Self, usize),
}

impl Location<'_> {
    fn render(&self) -> String {
        match self {
            Self::Root => "$".to_owned(),
            Self::Member(parent, key) => format!("{}.{key}", parent.render()),
            Self::Index(parent, index) => format!("{}[{index}]", parent.render()),
        }
    }
}

struct Seed<'a> {
    depth: usize,
    integers_only: bool,
    location: &'a Location<'a>,
    violation: &'a Cell<Option<(String, ErrorKind)>>,
}

impl Seed<'_> {
    fn refuse<E: serde::de::Error>(&self, kind: ErrorKind) -> E {
        self.violation.set(Some((self.location.render(), kind)));
        E::custom(kind)
    }

    fn child<'a>(&'a self, location: &'a Location<'a>) -> Seed<'a> {
        Seed {
            depth: self.depth.saturating_add(1),
            location,
            violation: self.violation,
            integers_only: self.integers_only,
        }
    }
}

impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = Value;

    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Seed<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an Amiss JSON value")
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }
    fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Value, E> {
        if self.integers_only && value.unsigned_abs() > super::MAX_SAFE_INTEGER.unsigned_abs() {
            return Err(self.refuse(ErrorKind::IntegerOutOfRange));
        }
        Ok(Value::from(value))
    }

    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Value, E> {
        if self.integers_only && value > super::MAX_SAFE_INTEGER.unsigned_abs() {
            return Err(self.refuse(ErrorKind::IntegerOutOfRange));
        }
        Ok(Value::from(value))
    }

    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Value, E> {
        if !self.integers_only {
            return serde_json::Number::from_f64(value)
                .map(Value::Number)
                .ok_or_else(|| self.refuse(ErrorKind::IntegerOutOfRange));
        }
        Err(self.refuse(if value == 0.0 && value.is_sign_negative() {
            ErrorKind::NegativeZero
        } else if value.abs() > 9_007_199_254_740_991.0 {
            ErrorKind::IntegerOutOfRange
        } else {
            ErrorKind::FractionOrExponent
        }))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Value, A::Error> {
        if self.depth >= MAX_DEPTH {
            return Err(self.refuse(ErrorKind::DepthLimit));
        }
        let mut values = Vec::new();
        loop {
            let location = Location::Index(self.location, values.len());
            let Some(value) = sequence.next_element_seed(self.child(&location))? else {
                break;
            };
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut object: A) -> Result<Value, A::Error> {
        if self.depth >= MAX_DEPTH {
            return Err(self.refuse(ErrorKind::DepthLimit));
        }
        let mut values = Map::new();
        while let Some(key) = object.next_key::<String>()? {
            let location = Location::Member(self.location, &key);
            let child = self.child(&location);
            if values.contains_key(&key) {
                return Err(child.refuse(ErrorKind::DuplicateKey));
            }
            let value = object.next_value_seed(child)?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

fn syntax_kind(defect: &serde_json::Error) -> ErrorKind {
    if defect.is_eof() {
        return ErrorKind::UnexpectedEnd;
    }
    let message = defect.to_string();
    [
        ("trailing characters", ErrorKind::TrailingContent),
        ("recursion limit exceeded", ErrorKind::DepthLimit),
        ("control character", ErrorKind::ControlCharacter),
        ("invalid escape", ErrorKind::InvalidEscape),
        ("unexpected end of hex escape", ErrorKind::LoneSurrogate),
        ("lone leading surrogate", ErrorKind::LoneSurrogate),
        ("invalid unicode code point", ErrorKind::LoneSurrogate),
        ("number out of range", ErrorKind::IntegerOutOfRange),
    ]
    .into_iter()
    .find(|(prefix, _)| message.starts_with(prefix))
    .map_or(ErrorKind::UnexpectedByte, |(_, kind)| kind)
}

fn offset(bytes: &[u8], line: usize, column: usize) -> usize {
    bytes
        .split(|&byte| byte == b'\n')
        .take(line.saturating_sub(1))
        .fold(0_usize, |start, row| {
            start.saturating_add(row.len()).saturating_add(1)
        })
        .saturating_add(column.saturating_sub(1))
        .min(bytes.len())
}
