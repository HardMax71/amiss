use amiss_bootstrap::supervise::{AcceptanceDefect, accept};
use amiss_fixtures::{canonical_json, corrupt};
use amiss_wire::digest::hb;
use amiss_wire::model::UtcInstant;
use amiss_wire::report::{PAYLOAD_SCHEMA, model};
use amiss_wire::requests::{CANDIDATE_IDENTITY_DOMAIN, CandidateIdentitySchema};

use super::{GOLDEN, bind};

#[test]
fn identity_extensions_are_refused_even_with_matching_bindings() {
    let (report, _) = &*GOLDEN;
    let model::Evaluation::Resolved(evaluation) = &report.payload.evaluation else {
        panic!("the fixture has a resolved evaluation");
    };
    let preimage = serde_json_canonicalizer::to_string(&model::IdentityPreimage {
        evaluation,
        schema: CandidateIdentitySchema::Current,
    })
    .unwrap();
    let extension =
        serde_json_canonicalizer::to_string(&(None::<bool>, "\u{1f600}\u{e000}")).unwrap();
    let index = format!(
        r#""index_only_materialized_paths":{}"#,
        serde_json::to_string(&evaluation.index_only_materialized_paths).unwrap()
    );
    let base = serde_json_canonicalizer::to_string(&evaluation.base).unwrap();
    let candidate = serde_json_canonicalizer::to_string(&evaluation.candidate).unwrap();
    let mut captured = Vec::new();
    for (original, bound) in [
        (index.clone(), format!(r#""future":{extension},{index}"#)),
        (
            format!(r#""base":{base}"#),
            format!(
                r#""base":{}"#,
                base.replacen('{', &format!(r#"{{"future":{extension},"#), 1)
            ),
        ),
        (
            format!(r#""candidate":{candidate}"#),
            format!(
                r#""candidate":{}"#,
                candidate.replacen('{', &format!(r#"{{"future":{extension},"#), 1)
            ),
        ),
    ] {
        assert_eq!(preimage.matches(&original).count(), 1);
        assert_eq!(bound.matches(&extension).count(), 1);
        let identity = preimage.replacen(&original, &bound, 1);
        let digest = hb(
            CANDIDATE_IDENTITY_DOMAIN,
            &canonical_json(identity.as_bytes()).unwrap(),
        );
        let (report, expectations) = bind(digest);
        let sealed = expectations.sealed.as_ref().unwrap();
        for replacement in [bound.clone(), bound.replacen(&extension, "true", 1)] {
            let wire = corrupt(&report, &original, &replacement).unwrap();
            assert_eq!(
                accept(&wire, &expectations),
                Err(AcceptanceDefect::Shape),
                "{replacement}"
            );
            captured.extend_from_slice(&wire);
            captured.extend_from_slice(sealed.candidate_identity_digest.to_string().as_bytes());
            captured.extend_from_slice(sealed.trusted_time_digest.to_string().as_bytes());
        }
    }
    assert_eq!(
        hb("amiss/test-sealed-identity-extensions", &captured).to_string(),
        "sha256:42449e8d15adbe3187d99be0b39a603caae5f2d1f42d1522fa4441ff35c6e206"
    );
}

#[test]
fn a_reserved_schema_cannot_join_the_identity_preimage() {
    let (report, _) = &*GOLDEN;
    let model::Evaluation::Resolved(evaluation) = &report.payload.evaluation else {
        panic!("the fixture has a resolved evaluation");
    };
    let object = serde_json_canonicalizer::to_string(evaluation).unwrap();
    let preimage = serde_json_canonicalizer::to_string(&model::IdentityPreimage {
        evaluation,
        schema: CandidateIdentitySchema::Current,
    })
    .unwrap();
    let mut captured = Vec::new();
    for schema in [
        "null".to_owned(),
        "false".to_owned(),
        serde_json::to_string(&CandidateIdentitySchema::Current).unwrap(),
    ] {
        let member = format!(r#"{{"schema":{schema},"#);
        let identity = preimage.replacen('{', &member, 1);
        let digest = hb(
            CANDIDATE_IDENTITY_DOMAIN,
            &canonical_json(identity.as_bytes()).unwrap(),
        );
        let (report, expectations) = bind(digest);
        let wire = corrupt(&report, &object, &object.replacen('{', &member, 1)).unwrap();
        assert_eq!(
            accept(&wire, &expectations),
            Err(AcceptanceDefect::Shape),
            "{schema}"
        );
        captured.extend_from_slice(&wire);
        let sealed = expectations.sealed.as_ref().unwrap();
        captured.extend_from_slice(sealed.candidate_identity_digest.to_string().as_bytes());
        captured.extend_from_slice(sealed.trusted_time_digest.to_string().as_bytes());
    }
    assert_eq!(
        hb("amiss/test-sealed-identity-schema", &captured).to_string(),
        "sha256:85d3a96abc4d70fd4c33949107e8986a2277f1ba124d67cb3fd99be42d0f6c30"
    );
}

#[test]
fn clock_shape_and_binding_defects_remain_distinct() {
    let (report, expectations) = &*GOLDEN;
    let model::Evaluation::Resolved(evaluation) = &report.payload.evaluation else {
        panic!("the fixture has a resolved evaluation");
    };
    let object = serde_json_canonicalizer::to_string(evaluation).unwrap();
    let member = format!(
        r#""evaluation_instant":{}"#,
        serde_json::to_string(&evaluation.evaluation_instant).unwrap()
    );
    assert_eq!(object.matches(&member).count(), 1);
    let late: UtcInstant = "2026-07-12T10:00:01Z".to_owned().try_into().unwrap();
    let mut captured = Vec::new();
    let sealed = expectations.sealed.as_ref().unwrap();
    for (value, defect) in [
        (
            serde_json::to_string(&None::<UtcInstant>).unwrap(),
            AcceptanceDefect::SealedControls,
        ),
        (
            serde_json::to_string(&true).unwrap(),
            AcceptanceDefect::Shape,
        ),
        (
            serde_json::to_string("not-an-instant").unwrap(),
            AcceptanceDefect::Shape,
        ),
        (
            serde_json::to_string("2026-02-30T10:00:00Z").unwrap(),
            AcceptanceDefect::Shape,
        ),
        (
            serde_json::to_string(&late).unwrap(),
            AcceptanceDefect::SealedControls,
        ),
    ] {
        let changed = object.replacen(&member, &format!(r#""evaluation_instant":{value}"#), 1);
        let wire = corrupt(report, &object, &changed).unwrap();
        assert_eq!(accept(&wire, expectations), Err(defect), "{value}");
        captured.extend_from_slice(&wire);
        captured.extend_from_slice(sealed.candidate_identity_digest.to_string().as_bytes());
        captured.extend_from_slice(sealed.trusted_time_digest.to_string().as_bytes());
    }
    let removed = object.replacen(&format!("{member},"), "", 1);
    let wire = corrupt(report, &object, &removed).unwrap();
    assert_eq!(accept(&wire, expectations), Err(AcceptanceDefect::Shape));
    captured.extend_from_slice(&wire);
    captured.extend_from_slice(sealed.candidate_identity_digest.to_string().as_bytes());
    captured.extend_from_slice(sealed.trusted_time_digest.to_string().as_bytes());
    assert_eq!(
        hb("amiss/test-sealed-identity-clock", &captured).to_string(),
        "sha256:b3a8748c0d471f63840c3ecf530dccd94cd2c9d79baa52708668a7e5a70a5f43"
    );
}

#[test]
fn identity_extensions_cannot_bypass_the_closed_shape_or_outer_depth_limit() {
    let (report, _) = &*GOLDEN;
    let model::Evaluation::Resolved(evaluation) = &report.payload.evaluation else {
        panic!("the fixture has a resolved evaluation");
    };
    let model::Snapshot::Available(amiss_wire::requests::CandidateSnapshot::Git(candidate)) =
        &evaluation.candidate
    else {
        panic!("the fixture has a Git candidate");
    };
    let candidate_object = serde_json_canonicalizer::to_string(candidate).unwrap();
    let kind = format!(
        r#""kind":{}"#,
        serde_json::to_string(&candidate.kind).unwrap()
    );
    let member = format!(r#""candidate":{candidate_object}"#);
    let preimage = serde_json_canonicalizer::to_string(&model::IdentityPreimage {
        evaluation,
        schema: CandidateIdentitySchema::Current,
    })
    .unwrap();
    assert_eq!(candidate_object.matches(&kind).count(), 1);
    assert_eq!(preimage.matches(&member).count(), 1);
    let mut captured = Vec::new();
    for depth in [256, 513] {
        let extension = format!(
            r#""future":{}null{},{}"#,
            "[".repeat(depth),
            "]".repeat(depth),
            kind
        );
        let changed = format!(
            r#""candidate":{}"#,
            candidate_object.replacen(&kind, &extension, 1)
        );
        let identity = preimage.replacen(&member, &changed, 1);
        let (report, expectations) = bind(hb(CANDIDATE_IDENTITY_DOMAIN, identity.as_bytes()));
        let payload = serde_json_canonicalizer::to_string(&report.payload).unwrap();
        let wire = serde_json_canonicalizer::to_string(&report).unwrap();
        let digest = report.payload_digest.to_string();
        assert_eq!(payload.matches(&member).count(), 1);
        assert_eq!(wire.matches(&payload).count(), 1);
        assert_eq!(wire.matches(&digest).count(), 1);
        let changed_payload = payload.replacen(&member, &changed, 1);
        let mut altered = wire.replacen(&payload, &changed_payload, 1).replacen(
            &digest,
            &hb(PAYLOAD_SCHEMA, changed_payload.as_bytes()).to_string(),
            1,
        );
        altered.push('\n');
        assert_eq!(
            accept(altered.as_bytes(), &expectations),
            Err(AcceptanceDefect::Shape),
            "{depth}"
        );
        captured.extend_from_slice(altered.as_bytes());
        let sealed = expectations.sealed.as_ref().unwrap();
        captured.extend_from_slice(sealed.candidate_identity_digest.to_string().as_bytes());
        captured.extend_from_slice(sealed.trusted_time_digest.to_string().as_bytes());
    }
    assert_eq!(
        hb("amiss/test-sealed-identity-depth", &captured).to_string(),
        "sha256:323417c20c533b668ccf608a1bdeb1fdb42d162ff28528d2d8fc9c7a6e53eb7f"
    );
}
