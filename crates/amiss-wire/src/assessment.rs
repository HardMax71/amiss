use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

use crate::controls::value::{object, text};
use crate::de::{self, Error, Obj};
use crate::digest::Digest;
use crate::json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumString, Serialize, Deserialize)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AssessmentVerdict {
    Matched,
    Refuted,
    Unproven,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Nullable<T> {
    Value(T),
    Null,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentEngine {
    pub engine_version: String,
    pub engine_digest: Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssessmentSubject {
    pub report_payload_digest: Digest,
    pub plan_payload_digest: Digest,
    pub evidence_payload_digest: Nullable<Digest>,
}

pub(crate) struct AssessmentBindings {
    pub engine_version: String,
    pub engine_digest: Digest,
    pub report_payload_digest: Digest,
    pub plan_payload_digest: Digest,
    pub evidence_payload_digest: Option<Digest>,
}

pub(crate) fn decode_bindings(parent: &mut Obj) -> Result<AssessmentBindings, Error> {
    let (engine_version, engine_digest) = parent.required("engine", |path, value| {
        let mut engine = Obj::new(path, value)?;
        let version = engine.required("engine_version", crate::semantic::decode_open_identity)?;
        let digest = engine.required("engine_digest", de::digest)?;
        engine.finish()?;
        Ok((version, digest))
    })?;
    let (report_payload_digest, plan_payload_digest, evidence_payload_digest) =
        parent.required("subject", |path, value| {
            let mut subject = Obj::new(path, value)?;
            let report = subject.required("report_payload_digest", de::digest)?;
            let plan = subject.required("plan_payload_digest", de::digest)?;
            let evidence_path = subject.field("evidence_payload_digest");
            let evidence = de::nullable(subject.take("evidence_payload_digest")?)
                .map(|value| de::digest(&evidence_path, value))
                .transpose()?;
            subject.finish()?;
            Ok((report, plan, evidence))
        })?;
    Ok(AssessmentBindings {
        engine_version,
        engine_digest,
        report_payload_digest,
        plan_payload_digest,
        evidence_payload_digest,
    })
}

pub(crate) fn bindings_value(
    engine_version: &str,
    engine_digest: Digest,
    report_payload_digest: Digest,
    plan_payload_digest: Digest,
    evidence_payload_digest: Option<Digest>,
) -> (Value, Value) {
    let engine = object(vec![
        ("engine_version", text(engine_version)),
        ("engine_digest", text(&engine_digest.to_string())),
    ]);
    let subject = object(vec![
        (
            "report_payload_digest",
            text(&report_payload_digest.to_string()),
        ),
        (
            "plan_payload_digest",
            text(&plan_payload_digest.to_string()),
        ),
        (
            "evidence_payload_digest",
            evidence_payload_digest.map_or(Value::Null, |digest| text(&digest.to_string())),
        ),
    ]);
    (engine, subject)
}
