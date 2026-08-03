use amiss_wire::json::Value;

/// The wire's lowercase hex back to raw bytes; a malformed digit renders as
/// zero rather than failing the human projection, which is not the wire.
pub(crate) fn decode_hex(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|text| u8::from_str_radix(text, 16).ok())
                .unwrap_or(0)
        })
        .collect()
}

pub(crate) struct View(Vec<(String, Value)>);

impl View {
    pub(crate) fn of(value: Option<&Value>) -> Self {
        if let Some(Value::Object(members)) = value {
            Self(members.clone())
        } else {
            Self(Vec::new())
        }
    }

    pub(crate) fn field(&self, name: &str) -> Option<&Value> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    pub(crate) fn view(&self, name: &str) -> Self {
        Self::of(self.field(name))
    }

    pub(crate) fn text(&self, name: &str) -> String {
        if let Some(Value::String(value)) = self.field(name) {
            value.clone()
        } else {
            String::new()
        }
    }

    pub(crate) fn flag(&self, name: &str) -> bool {
        matches!(self.field(name), Some(Value::Bool(true)))
    }

    pub(crate) fn atom_or_dash(&self, name: &str) -> String {
        match self.field(name) {
            Some(Value::String(value)) => amiss_wire::human::atom(value),
            Some(Value::Object(members)) => match members.as_slice() {
                [(key, Value::String(hex))] if key == "bytes_hex" => {
                    amiss_wire::human::atom_bytes(&decode_hex(hex))
                }
                _ => "-".to_owned(),
            },
            Some(Value::Null | Value::Bool(_) | Value::Integer(_) | Value::Array(_)) | None => {
                "-".to_owned()
            }
        }
    }

    pub(crate) fn number(&self, name: &str) -> i64 {
        if let Some(Value::Integer(value)) = self.field(name) {
            *value
        } else {
            0
        }
    }

    pub(crate) fn rows(&self, name: &str) -> Vec<Self> {
        if let Some(Value::Array(rows)) = self.field(name) {
            rows.iter().map(|row| Self::of(Some(row))).collect()
        } else {
            Vec::new()
        }
    }
}

#[path = "../tests/internal/view.rs"]
mod tests;
