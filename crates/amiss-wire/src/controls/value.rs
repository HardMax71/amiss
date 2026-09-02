use crate::json::Value;
use crate::model::RepositoryIdentity;

pub(crate) fn text(value: &str) -> Value {
    Value::String(value.into())
}

pub(crate) fn object(rows: Vec<(&str, Value)>) -> Value {
    Value::Object(
        rows.into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
    )
}

pub(crate) fn repository(identity: &RepositoryIdentity) -> Value {
    object(vec![
        ("host", text(identity.host())),
        ("owner", text(identity.owner())),
        ("name", text(identity.name())),
    ])
}
