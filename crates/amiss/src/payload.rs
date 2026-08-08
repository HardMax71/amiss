use amiss_wire::json::Value;

pub(crate) fn member<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    let Value::Object(members) = value else {
        return None;
    };
    members
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, member)| member)
}

pub(crate) fn text(value: &Value) -> Option<&str> {
    let Value::String(text) = value else {
        return None;
    };
    Some(text)
}

pub(crate) fn byte_offset(value: &Value) -> Option<usize> {
    let Value::Integer(offset) = value else {
        return None;
    };
    usize::try_from(*offset).ok()
}
