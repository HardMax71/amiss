mod tests;

use amiss_wire::json::Value;
use amiss_wire::json::ValueExt as _;

#[derive(Clone, Copy)]
pub(crate) struct View<'value>(Option<&'value Value>);

impl<'value> View<'value> {
    pub(crate) fn of(value: &'value Value) -> Self {
        Self(Some(value))
    }

    pub(crate) fn field(self, name: &str) -> Option<&'value Value> {
        self.0.and_then(|value| value.get(name))
    }

    pub(crate) fn view(self, name: &str) -> Self {
        if let Some(value) = self.field(name) {
            Self::of(value)
        } else {
            Self(None)
        }
    }

    pub(crate) fn text(self, name: &str) -> &'value str {
        if let Some(Value::String(value)) = self.field(name) {
            value
        } else {
            ""
        }
    }

    pub(crate) fn flag(self, name: &str) -> bool {
        matches!(self.field(name), Some(Value::Bool(true)))
    }

    pub(crate) fn atom_or_dash(self, name: &str) -> String {
        match self.field(name) {
            Some(Value::String(value)) => amiss_wire::human::atom(value),
            Some(Value::Object(members)) => match members.get("bytes_hex") {
                Some(Value::String(hex)) if members.len() == 1 => {
                    amiss_wire::human::atom_bytes(&amiss_wire::human::decode_hex(hex))
                }
                _ => "-".to_owned(),
            },
            Some(Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_)) | None => {
                "-".to_owned()
            }
        }
    }

    pub(crate) fn number(self, name: &str) -> i64 {
        self.field(name).and_then(Value::as_i64).unwrap_or(0)
    }

    pub(crate) fn rows(self, name: &str) -> impl ExactSizeIterator<Item = Self> + Clone + 'value {
        let rows: &'value [Value] = if let Some(Value::Array(rows)) = self.field(name) {
            rows
        } else {
            &[]
        };
        rows.iter().map(Self::of)
    }
}

/// A projection object in canonical member order, shared by the lanes that
/// build non-wire JSON.
pub(crate) fn object(members: Vec<(&str, Value)>) -> Value {
    let mut members: Vec<(String, Value)> = members
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect();
    members.sort_by(|left, right| left.0.cmp(&right.0));
    Value::object(members)
}

pub(crate) fn string(value: &str) -> Value {
    Value::string(value)
}
