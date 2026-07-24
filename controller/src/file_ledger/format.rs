mod model;
mod publication;
mod record;

use amiss_wire::digest::{Digest, hb};
use serde::Serialize;

use crate::{ControllerEvaluationId, DeliveryIdentity};

use self::model::StoredDeliveryKey;
pub(super) use self::publication::{ReportRef, StoredPublication};
pub(super) use self::record::{Record, State};
use super::{FileLedgerError, frame};

const RECORD_FRAME: frame::FrameFormat = frame::define(
    b"AMISS-DELIVERY-RECORD",
    "amiss/controller-file-record-v1",
    MAX_RECORD_BYTES,
);

const KEY_DOMAIN: &str = "amiss/controller-delivery-key-v1";
const STAGED_DOMAIN: &str = "amiss/controller-staged-publication-v2";

pub(super) const MAX_RECORD_BYTES: u64 = 131_072;

pub(super) fn encode(record: &Record) -> Result<Vec<u8>, FileLedgerError> {
    frame::encode(RECORD_FRAME, record, Record::validate)
}

pub(super) fn decode(bytes: &[u8]) -> Result<Record, FileLedgerError> {
    frame::decode(RECORD_FRAME, bytes, Record::validate)
}

pub(super) fn delivery_key(identity: &DeliveryIdentity) -> Result<String, FileLedgerError> {
    let bytes = serde_json::to_vec(&StoredDeliveryKey::new(identity))
        .map_err(|_| FileLedgerError::Corrupt)?;
    digest_hex(&hb(KEY_DOMAIN, &bytes).to_string())
}

pub(super) fn evaluation_id(
    identity: &DeliveryIdentity,
    nonce: &[u8; 16],
) -> Result<ControllerEvaluationId, FileLedgerError> {
    ControllerEvaluationId::new(format!(
        "eval:{}:{}",
        delivery_key(identity)?,
        hex::encode(nonce)
    ))
    .ok_or(FileLedgerError::Corrupt)
}

pub(super) fn staged_digest(
    evaluation_id: &ControllerEvaluationId,
    fence: u64,
    publication: &StoredPublication,
) -> Result<String, FileLedgerError> {
    let value = StagedDigest {
        evaluation_id: evaluation_id.as_str(),
        fence,
        publication,
    };
    let bytes = serde_json::to_vec(&value).map_err(|_| FileLedgerError::Corrupt)?;
    Ok(hb(STAGED_DOMAIN, &bytes).to_string())
}

pub(super) fn digest_hex(wire: &str) -> Result<String, FileLedgerError> {
    Digest::from_wire(wire).ok_or(FileLedgerError::Corrupt)?;
    wire.strip_prefix("sha256:")
        .map(str::to_owned)
        .ok_or(FileLedgerError::Corrupt)
}

#[derive(Serialize)]
struct StagedDigest<'a> {
    evaluation_id: &'a str,
    fence: u64,
    publication: &'a StoredPublication,
}
