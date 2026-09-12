use sha2::Digest as _;
mod model;
mod publication;
mod record;

use amiss_wire::model::Digest;
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
    let mut writer =
        digest_io::IoWrapper(sha2::Sha256::new_with_prefix(KEY_DOMAIN).chain_update([0_u8]));
    serde_json::to_writer(&mut writer, &StoredDeliveryKey::new(identity))
        .map_err(|_defect| FileLedgerError::Corrupt)?;
    Ok(hex::encode(writer.0.finalize()))
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
) -> Result<Digest, FileLedgerError> {
    let value = StagedDigest {
        evaluation_id,
        fence,
        publication,
    };
    let mut writer =
        digest_io::IoWrapper(sha2::Sha256::new_with_prefix(STAGED_DOMAIN).chain_update([0_u8]));
    serde_json::to_writer(&mut writer, &value).map_err(|_defect| FileLedgerError::Corrupt)?;
    Ok(Digest::from(writer.0.finalize().0))
}

#[derive(Serialize)]
struct StagedDigest<'a> {
    evaluation_id: &'a ControllerEvaluationId,
    fence: u64,
    publication: &'a StoredPublication,
}
