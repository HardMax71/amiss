use amiss_wire::{
    assessment::Nullable,
    de::{Error, ErrorKind},
    digest::sha256,
    locale::{
        self, EVIDENCE_DOCUMENT_BYTES, LocaleSourcePage, LocaleTargetPage, PAGE_ITEMS_LIMIT,
        PAGE_KEY_BYTES,
    },
};

use super::input::assert_object_required;

const EVIDENCE: &[u8] = include_bytes!("../../../../spec/examples/locale-coverage-evidence.json");

#[test]
fn owned_locale_evidence_preserves_inventories_through_assessment_and_output() {
    let plan = locale::parse_plan(include_bytes!(
        "../../../../spec/examples/locale-coverage-plan.json"
    ))
    .unwrap();
    let mut input = locale::parse_evidence(EVIDENCE).unwrap().payload;
    input.source.pages[0].key = "k".repeat(PAGE_KEY_BYTES);
    input.target.pages[0].key = "k".repeat(PAGE_KEY_BYTES);
    let source_vector = input.source.pages.as_ptr();
    let target_vector = input.target.pages.as_ptr();
    let source_key = input.source.pages[0].key.as_ptr();
    let target_key = input.target.pages[0].key.as_ptr();
    let document = locale::evidence(input).unwrap();
    assert_eq!(document.payload.source.pages.as_ptr(), source_vector);
    assert_eq!(document.payload.target.pages.as_ptr(), target_vector);
    assert_eq!(document.payload.source.pages[0].key.as_ptr(), source_key);
    assert_eq!(document.payload.target.pages[0].key.as_ptr(), target_key);
    let assessment = locale::assess(&plan, Some(&document), "1", sha256(b"engine")).unwrap();
    assert_eq!(
        assessment.payload.subject.evidence_payload_digest,
        Nullable::Value(document.payload_digest)
    );
    let mut bytes = Vec::new();
    amiss_wire::write_json(&document, &mut bytes, EVIDENCE_DOCUMENT_BYTES).unwrap();
    assert_eq!(locale::parse_evidence(&bytes).unwrap(), document);
    let exact = u64::try_from(bytes.len()).unwrap();
    amiss_wire::write_json(&document, std::io::sink(), exact).unwrap();
    assert!(matches!(
        amiss_wire::write_json(&document, std::io::sink(), exact - 1)
            .unwrap_err()
            .kind,
        ErrorKind::LimitExceeded
    ));
    let mut invalid_source = document.payload.clone();
    invalid_source.source.pages[0].key.push('k');
    let mut invalid_target = document.payload;
    invalid_target.target.pages[0].key.push('k');
    for (input, path) in [
        (invalid_source, "$.payload.source.pages[0].key"),
        (invalid_target, "$.payload.target.pages[0].key"),
    ] {
        let error = locale::evidence(input).unwrap_err();
        assert_eq!(error.path, path);
        assert!(matches!(error.kind, ErrorKind::InvalidValue));
    }
}

#[test]
fn locale_evidence_keeps_individual_and_combined_inventory_limits() {
    let mut input = locale::parse_evidence(EVIDENCE).unwrap().payload;
    let resource_digest = sha256(b"page");
    let origin = input.target.pages[0].origin.clone();
    input.source.pages = (0..PAGE_ITEMS_LIMIT)
        .map(|index| LocaleSourcePage {
            key: format!("page/{index:06}"),
            resource_digest,
        })
        .collect();
    input.target.pages.clear();
    let mut input = locale::evidence(input).unwrap().payload;
    assert_eq!(input.source.pages.len(), PAGE_ITEMS_LIMIT);
    input.target.pages.push(LocaleTargetPage {
        key: "target".to_owned(),
        resource_digest,
        origin: origin.clone(),
    });
    assert!(matches!(
        locale::evidence(input.clone()).err(),
        Some(Error {
            path,
            kind: ErrorKind::LimitExceeded,
        }) if path == "$.payload.target.pages"
    ));
    input.source.pages.push(LocaleSourcePage {
        key: "z".to_owned(),
        resource_digest,
    });
    let error = locale::evidence(input.clone()).unwrap_err();
    assert_eq!(error.path, "$.payload.source.pages");
    assert!(matches!(error.kind, ErrorKind::LimitExceeded));
    input.target.pages = input
        .source
        .pages
        .drain(..)
        .map(|page| LocaleTargetPage {
            key: page.key,
            resource_digest: page.resource_digest,
            origin: origin.clone(),
        })
        .collect();
    let error = locale::evidence(input).unwrap_err();
    assert_eq!(error.path, "$.payload.target.pages");
    assert!(matches!(error.kind, ErrorKind::LimitExceeded));
    let oversized = vec![b' '; usize::try_from(EVIDENCE_DOCUMENT_BYTES).unwrap() + 1];
    assert!(matches!(
        locale::parse_evidence(&oversized).unwrap_err().kind,
        ErrorKind::LimitExceeded
    ));
}

#[test]
fn locale_evidence_requires_objects_for_envelope_and_shared_bindings()
-> Result<(), Box<dyn std::error::Error>> {
    let document = locale::parse_evidence(EVIDENCE)?;
    let input = (&document, locale::parse_evidence);
    let payload = &document.payload;
    let docs = &payload.docs;
    let repository = &docs.repository;
    let scope = &payload.scope;
    let producer = &payload.producer;
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
            payload.plan_payload_digest,
            docs,
            scope,
            producer,
            &payload.source,
            &payload.target,
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
    assert_object_required(
        input,
        producer,
        (
            &producer.identity,
            &producer.version,
            producer.context_digest,
        ),
    )?;
    Ok(())
}

#[test]
fn locale_evidence_requires_objects_for_both_inventories_pages_and_products()
-> Result<(), Box<dyn std::error::Error>> {
    let mut payload = locale::parse_evidence(EVIDENCE)?.payload;
    let Nullable::Value(product) = &mut payload.target.product else {
        panic!("the committed evidence includes its target product");
    };
    product.digest = sha256(b"distinct target product");
    let document = locale::evidence(payload)?;
    let input = (&document, locale::parse_evidence);
    let source = &document.payload.source;
    let target = &document.payload.target;
    assert_object_required(
        input,
        source,
        (
            source.input_digest,
            &source.product,
            source.complete,
            &source.pages,
        ),
    )?;
    assert_object_required(
        input,
        target,
        (
            target.input_digest,
            &target.product,
            target.complete,
            &target.pages,
        ),
    )?;
    for page in &source.pages {
        assert_object_required(input, page, (&page.key, page.resource_digest))?;
    }
    for page in &target.pages {
        assert_object_required(input, page, (&page.key, page.resource_digest, &page.origin))?;
    }
    for product in [&source.product, &target.product] {
        let Nullable::Value(product) = product else {
            panic!("the committed evidence includes both products");
        };
        assert_object_required(input, product, (&product.uri, product.digest))?;
    }
    Ok(())
}

#[test]
fn locale_evidence_nullable_fields_require_explicit_presence()
-> Result<(), Box<dyn std::error::Error>> {
    let mut payload = locale::parse_evidence(EVIDENCE)?.payload;
    payload.source.product = Nullable::Null;
    payload.target.product = Nullable::Null;
    payload.scope.version = Nullable::Null;
    payload.target.pages[0].origin = locale::LocaleTargetOrigin::TargetResource {
        based_on_source_digest: Nullable::Null,
    };
    let document = locale::evidence(payload)?;
    let text = serde_json::to_string(&document)?;
    assert_eq!(locale::parse_evidence(text.as_bytes())?, document);
    for (object, member, path) in [
        (
            serde_json::to_string(&document.payload.source)?,
            r#""product":null,"#,
            "$.payload.source.product",
        ),
        (
            serde_json::to_string(&document.payload.target)?,
            r#""product":null,"#,
            "$.payload.target.product",
        ),
        (
            serde_json::to_string(&document.payload.scope)?,
            r#","version":null"#,
            "$.payload.scope.version",
        ),
        (
            serde_json::to_string(&document.payload.target.pages[0].origin)?,
            r#","based_on_source_digest":null"#,
            "$.payload.target.pages[0].origin.based_on_source_digest",
        ),
    ] {
        assert_eq!(text.matches(&object).count(), 1);
        assert_eq!(object.matches(member).count(), 1);
        let missing = text.replacen(&object, &object.replacen(member, "", 1), 1);
        assert_ne!(missing, text);
        assert!(serde_json::from_str::<locale::LocaleCoverageEvidenceEnvelope>(&missing).is_err());
        let error = locale::parse_evidence(missing.as_bytes()).unwrap_err();
        assert_eq!(error.path, path.rsplit_once('.').unwrap().0);
        assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));
    }
    Ok(())
}

#[test]
fn locale_evidence_schema_tags_remain_required_and_closed() -> Result<(), Box<dyn std::error::Error>>
{
    let document = locale::parse_evidence(EVIDENCE)?;
    let text = serde_json::to_string(&document)?;
    for (tag, path) in [
        (serde_json::to_string(&document.schema)?, "$.schema"),
        (
            serde_json::to_string(&document.payload.schema)?,
            "$.payload.schema",
        ),
    ] {
        for invalid in ["null", "false", r#""unknown""#] {
            let changed = text.replacen(&tag, invalid, 1);
            assert_ne!(changed, text);
            let error = locale::parse_evidence(changed.as_bytes()).unwrap_err();
            assert_eq!(error.path, path);
            assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));
        }
        let missing = text.replacen(&format!("\"schema\":{tag},"), "", 1);
        assert_ne!(missing, text);
        let error = locale::parse_evidence(missing.as_bytes()).unwrap_err();
        assert_eq!(error.path, path.rsplit_once('.').unwrap().0);
        assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));
    }
    Ok(())
}
