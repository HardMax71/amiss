use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::Digest as _;

const VERSION: u8 = 1;
const LENGTH_BYTES: usize = 8;
const DIGEST_BYTES: usize = 32;

/// Bytes that are not one frame of the format: the magic, version, length or
/// digest does not hold, the payload does not read back as written, or the
/// frame is over its ceiling. The store owning the format names the defect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Corrupt;

#[derive(Clone, Copy)]
pub struct FrameFormat {
    magic: &'static [u8],
    domain: &'static str,
    maximum: u64,
}

#[must_use]
pub const fn define(magic: &'static [u8], domain: &'static str, maximum: u64) -> FrameFormat {
    FrameFormat {
        magic,
        domain,
        maximum,
    }
}

/// Frames one validated value: magic, version, payload length, the payload's
/// domain-separated digest, then the payload.
///
/// # Errors
///
/// The value fails `validate`, does not serialize, or the frame is over its ceiling.
pub fn encode<T: Serialize, E: From<Corrupt>>(
    format: FrameFormat,
    value: &T,
    validate: impl FnOnce(&T) -> Result<(), E>,
) -> Result<Vec<u8>, E> {
    validate(value)?;
    let payload = serde_json::to_vec(value).map_err(|_defect| E::from(Corrupt))?;
    let payload_length = u64::try_from(payload.len()).map_err(|_defect| E::from(Corrupt))?;
    let frame_length = format
        .magic
        .len()
        .checked_add(1 + LENGTH_BYTES + DIGEST_BYTES)
        .and_then(|length| length.checked_add(payload.len()))
        .ok_or(Corrupt)?;
    if u64::try_from(frame_length).map_err(|_defect| E::from(Corrupt))? > format.maximum {
        return Err(E::from(Corrupt));
    }
    let mut frame = Vec::with_capacity(frame_length);
    frame.extend_from_slice(format.magic);
    frame.push(VERSION);
    frame.extend_from_slice(&payload_length.to_be_bytes());
    frame.extend_from_slice(
        &sha2::Sha256::new_with_prefix(format.domain)
            .chain_update([0_u8])
            .chain_update(&payload)
            .finalize(),
    );
    frame.extend_from_slice(&payload);
    Ok(frame)
}

/// Reads one frame back: the header must hold, the payload must be the exact
/// bytes its value writes, and the value must pass `validate`.
///
/// # Errors
///
/// Any of those refusals, as the owning store's defect.
pub fn decode<T: DeserializeOwned + Serialize, E: From<Corrupt>>(
    format: FrameFormat,
    frame: &[u8],
    validate: impl FnOnce(&T) -> Result<(), E>,
) -> Result<T, E> {
    if u64::try_from(frame.len()).unwrap_or(u64::MAX) > format.maximum {
        return Err(E::from(Corrupt));
    }
    let (version, header) = frame
        .strip_prefix(format.magic)
        .and_then(<[u8]>::split_first)
        .ok_or(Corrupt)?;
    if *version != VERSION {
        return Err(E::from(Corrupt));
    }
    let (length, header) = header.split_first_chunk::<LENGTH_BYTES>().ok_or(Corrupt)?;
    let (expected_digest, payload) = header.split_first_chunk::<DIGEST_BYTES>().ok_or(Corrupt)?;
    if u64::try_from(payload.len()).ok() != Some(u64::from_be_bytes(*length))
        || sha2::Sha256::new_with_prefix(format.domain)
            .chain_update([0_u8])
            .chain_update(payload)
            .finalize()
            .as_slice()
            != expected_digest
    {
        return Err(E::from(Corrupt));
    }
    let value = serde_json::from_slice(payload).map_err(|_defect| E::from(Corrupt))?;
    validate(&value)?;
    if serde_json::to_vec(&value).map_err(|_defect| E::from(Corrupt))? != payload {
        return Err(E::from(Corrupt));
    }
    Ok(value)
}
