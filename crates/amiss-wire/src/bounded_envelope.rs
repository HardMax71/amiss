use crate::controls::value::{object, text};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::digest::{Digest, hj};
use crate::json::{self, Value, canonical_length};

pub(crate) fn parse<T>(
    bytes: &[u8],
    envelope_schema: &str,
    payload_schema: &str,
    maximum_bytes: u64,
    decode: impl FnOnce(&str, Value) -> Result<T, Error>,
) -> Result<(T, Digest), Error> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > maximum_bytes {
        return fail("$", ErrorKind::LimitExceeded);
    }
    let value = json::parse(bytes).map_err(|defect| Error::new("$", ErrorKind::Json(defect)))?;
    let mut envelope = Obj::new("$", value)?;
    envelope.required("schema", |path, value| {
        de::const_str(path, value, envelope_schema)
    })?;
    let payload = envelope.take("payload")?;
    let payload_digest = envelope.required("payload_digest", de::digest)?;
    envelope.finish()?;
    if hj(payload_schema, &payload) != payload_digest {
        return fail("$.payload_digest", ErrorKind::DigestMismatch);
    }
    Ok((decode("$.payload", payload)?, payload_digest))
}

pub(crate) fn build(
    payload: Value,
    envelope_schema: &str,
    payload_schema: &str,
    maximum_bytes: u64,
) -> Result<Value, Error> {
    let payload_digest = hj(payload_schema, &payload);
    let value = object(vec![
        ("schema", text(envelope_schema)),
        ("payload", payload),
        ("payload_digest", text(&payload_digest.to_string())),
    ]);
    if canonical_length(&value) > maximum_bytes {
        fail("$", ErrorKind::LimitExceeded)
    } else {
        Ok(value)
    }
}
