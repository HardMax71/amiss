use amiss_wire::json::Value;

pub(crate) fn member<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.get(key)
}
pub(crate) fn text(value: &Value) -> Option<&str> {
    value.as_str()
}
pub(crate) fn byte_offset(value: &Value) -> Option<usize> {
    usize::try_from(value.as_u64()?).ok()
}
