mod tests;

use amiss_wire::json::Value;

#[derive(Clone, Copy)]
pub(crate) struct View<'value>(&'value [(String, Value)]);

impl<'value> View<'value> {
    pub(crate) fn of(value: &'value Value) -> Self {
        match value {
            Value::Object(members) => Self(members),
            Value::Null
            | Value::Bool(_)
            | Value::Integer(_)
            | Value::String(_)
            | Value::Array(_) => Self(&[]),
        }
    }

    pub(crate) fn field(self, name: &str) -> Option<&'value Value> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    pub(crate) fn view(self, name: &str) -> Self {
        if let Some(value) = self.field(name) {
            Self::of(value)
        } else {
            Self(&[])
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
            Some(Value::Object(members)) => match members.as_ref() {
                [(key, Value::String(hex))] if key == "bytes_hex" => {
                    amiss_wire::human::atom_bytes(&amiss_wire::human::decode_hex(hex))
                }
                _ => "-".to_owned(),
            },
            Some(Value::Null | Value::Bool(_) | Value::Integer(_) | Value::Array(_)) | None => {
                "-".to_owned()
            }
        }
    }

    pub(crate) fn number(self, name: &str) -> i64 {
        if let Some(Value::Integer(value)) = self.field(name) {
            *value
        } else {
            0
        }
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
