use amiss_wire::{
    assessment::Nullable,
    de::ErrorKind,
    digest::sha256,
    locale::{
        self, LOCALE_DOCUMENT_BYTES, LocaleCoverageReason, LocalePageRequirement, PAGE_KEY_BYTES,
    },
};

use super::input::assert_object_required;

const PLAN: &[u8] = include_bytes!("../../../../spec/examples/locale-coverage-plan.json");

#[test]
fn owned_locale_plan_preserves_page_allocations_and_bounded_output() {
    let mut input = locale::parse_plan(PLAN).unwrap().payload;
    let key = "k".repeat(PAGE_KEY_BYTES);
    let key_pointer = key.as_ptr();
    let keys = vec![key];
    let vector_pointer = keys.as_ptr();
    let locale_pointer = input.scope.source_locale.as_ptr();
    input.policy.required = LocalePageRequirement::Named { keys };
    let envelope = locale::plan(input).unwrap();
    let LocalePageRequirement::Named { keys } = &envelope.payload.policy.required else {
        panic!("the selected named policy must remain in the owned plan");
    };
    assert_eq!(keys.as_ptr(), vector_pointer);
    assert_eq!(keys[0].as_ptr(), key_pointer);
    assert_eq!(keys[0].len(), PAGE_KEY_BYTES);
    assert_eq!(
        envelope.payload.scope.source_locale.as_ptr(),
        locale_pointer
    );
    let assessment = locale::assess(&envelope, None, "1", sha256(b"engine")).unwrap();
    assert_eq!(
        assessment.payload.reasons,
        vec![LocaleCoverageReason::EvidenceAbsent]
    );
    let mut bytes = Vec::new();
    amiss_wire::write_json(&envelope, &mut bytes, LOCALE_DOCUMENT_BYTES).unwrap();
    assert_eq!(locale::parse_plan(&bytes).unwrap(), envelope);
    let exact = u64::try_from(bytes.len()).unwrap();
    assert!(exact <= LOCALE_DOCUMENT_BYTES);
    amiss_wire::write_json(&envelope, std::io::sink(), exact).unwrap();
    assert_eq!(
        amiss_wire::write_json(&envelope, std::io::sink(), exact - 1)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    );
    let mut invalid = envelope.payload;
    invalid.policy.required = LocalePageRequirement::Named {
        keys: vec!["k".repeat(PAGE_KEY_BYTES + 1)],
    };
    let error = locale::plan(invalid).unwrap_err();
    assert_eq!(error.path, "$.payload.policy.required.keys[0]");
    assert_eq!(error.kind, ErrorKind::InvalidValue);
}

#[test]
fn locale_plan_requires_objects_at_every_struct_position() -> Result<(), Box<dyn std::error::Error>>
{
    let document = locale::parse_plan(PLAN)?;
    let input = (&document, locale::parse_plan);
    let payload = &document.payload;
    let docs = &payload.docs;
    let repository = &docs.repository;
    let scope = &payload.scope;
    let producer = &payload.producer;
    let policy = &payload.policy;
    let fallback = &policy.fallbacks[0];
    let Nullable::Value(product) = &payload.product else {
        panic!("the committed plan includes its selected product");
    };
    assert_object_required(
        input,
        &document,
        (document.schema, payload, document.payload_digest),
    )?;
    assert_object_required(
        input,
        payload,
        (
            payload.schema,
            payload.report_payload_digest,
            docs,
            scope,
            &payload.product,
            producer,
            policy,
        ),
    )?;
    assert_object_required(
        input,
        docs,
        (
            repository,
            docs.object_format,
            &docs.commit,
            &docs.tree,
            docs.candidate_identity_digest,
        ),
    )?;
    assert_object_required(
        input,
        repository,
        (repository.host(), repository.name(), repository.owner()),
    )?;
    assert_object_required(
        input,
        scope,
        (
            &scope.site,
            &scope.source_locale,
            &scope.target_locale,
            &scope.channel,
            &scope.version,
        ),
    )?;
    assert_object_required(input, product, (&product.uri, product.digest))?;
    assert_object_required(
        input,
        producer,
        (
            &producer.identity,
            &producer.version,
            producer.context_digest,
        ),
    )?;
    assert_object_required(
        input,
        policy,
        (
            &policy.identity,
            policy.context_digest,
            &policy.required,
            &policy.fallbacks,
            policy.require_target_lineage,
        ),
    )?;
    assert_object_required(input, fallback, (&fallback.class, &fallback.pages))?;
    Ok(())
}

#[test]
fn locale_plan_schema_tags_and_nullable_fields_remain_required()
-> Result<(), Box<dyn std::error::Error>> {
    let mut payload = locale::parse_plan(PLAN)?.payload;
    payload.product = Nullable::Null;
    payload.scope.version = Nullable::Null;
    let document = locale::plan(payload)?;
    let text = serde_json::to_string(&document)?;
    assert_eq!(locale::parse_plan(text.as_bytes())?, document);
    for (tag, path) in [
        (serde_json::to_string(&document.schema)?, "$.schema"),
        (
            serde_json::to_string(&document.payload.schema)?,
            "$.payload.schema",
        ),
    ] {
        for (invalid, kind) in [
            ("null", ErrorKind::WrongType),
            ("false", ErrorKind::WrongType),
            (r#""unknown""#, ErrorKind::InvalidValue),
        ] {
            let changed = text.replacen(&tag, invalid, 1);
            assert_ne!(changed, text);
            let error = locale::parse_plan(changed.as_bytes()).unwrap_err();
            assert_eq!(error.path, path);
            assert_eq!(error.kind, kind);
        }
        let missing = text.replacen(&format!("\"schema\":{tag},"), "", 1);
        assert_ne!(missing, text);
        let error = locale::parse_plan(missing.as_bytes()).unwrap_err();
        assert_eq!(error.path, path);
        assert_eq!(error.kind, ErrorKind::MissingField);
    }
    for (member, path) in [
        ("\"product\":null,", "$.payload.product"),
        (",\"version\":null", "$.payload.scope.version"),
    ] {
        assert_eq!(text.matches(member).count(), 1);
        let missing = text.replacen(member, "", 1);
        assert_ne!(missing, text);
        assert!(serde_json::from_str::<locale::LocaleCoveragePlanEnvelope>(&missing).is_err());
        let error = locale::parse_plan(missing.as_bytes()).unwrap_err();
        assert_eq!(error.path, path);
        assert_eq!(error.kind, ErrorKind::MissingField);
    }
    Ok(())
}
