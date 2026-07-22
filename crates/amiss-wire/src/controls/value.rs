use crate::de::{Error, ErrorKind};
use crate::json::{MAX_SAFE_INTEGER, Value};
use crate::model::RepositoryIdentity;

pub(crate) fn text(value: &str) -> Value {
    Value::String(value.to_owned())
}

pub(crate) fn object(rows: Vec<(&str, Value)>) -> Value {
    Value::Object(
        rows.into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

pub(crate) fn repository(identity: &RepositoryIdentity) -> Value {
    object(vec![
        ("host", text(&identity.host)),
        ("owner", text(&identity.owner)),
        ("name", text(&identity.name)),
    ])
}

pub(crate) fn positive_safe_integer(path: &str, raw: u64) -> Result<Value, Error> {
    i64::try_from(raw)
        .ok()
        .filter(|value| (1..=MAX_SAFE_INTEGER).contains(value))
        .map(Value::Integer)
        .ok_or_else(|| Error::new(path, ErrorKind::InvalidValue))
}
