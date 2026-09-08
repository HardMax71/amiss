use amiss_wire::{
    de::{Error, ErrorKind},
    publication,
};
use serde::Serialize;

fn assert_object_required<T>(
    (document, read): (&T, impl Fn(&[u8]) -> Result<T, Error>),
    object: &impl Serialize,
    positional: impl Serialize,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: Serialize + PartialEq + std::fmt::Debug,
{
    let text = serde_json::to_string(document)?;
    for valid in [
        serde_json::to_string_pretty(document)?,
        String::from_utf8(serde_json_canonicalizer::to_vec(document)?)?,
        format!(" \n\t{}\r ", text.replace("schema", r"\u0073chema")),
        text.replace('/', r"\/"),
    ] {
        assert_eq!(&read(valid.as_bytes())?, document);
    }
    let object = serde_json::to_string(object)?;
    assert_eq!(text.matches(&object).count(), 1);
    let changed = text.replacen(&object, &serde_json::to_string(&positional)?, 1);
    assert_ne!(changed, text);
    assert_eq!(
        read(changed.as_bytes()).err(),
        Some(Error {
            path: "$".to_owned(),
            kind: ErrorKind::InvalidValue,
        })
    );
    Ok(())
}

#[test]
fn publication_plan_requires_objects_without_normalizing_positional_input()
-> Result<(), Box<dyn std::error::Error>> {
    let document = publication::parse_plan(include_bytes!(
        "../../../../spec/examples/publication-plan.json"
    ))?;
    let input = (&document, publication::parse_plan);
    let payload = &document.payload;
    let docs = &payload.docs;
    let repository = &docs.repository;
    let target = &payload.target;
    let site = &payload.site;
    let product = &payload.product;
    let producer = &payload.producer;
    let relation = &payload.relation;
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
            target,
            site,
            product,
            producer,
            relation,
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
        target,
        (
            &target.provider,
            &target.instance,
            &target.environment,
            &target.channel,
            &target.canonical_url,
        ),
    )?;
    assert_object_required(input, site, (&site.artifact, site.input_digest))?;
    assert_object_required(
        input,
        &site.artifact,
        (&site.artifact.uri, site.artifact.digest),
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
        relation,
        (&relation.identity, relation.context_digest),
    )?;
    Ok(())
}

#[test]
fn publication_receipts_require_objects_without_normalizing_positional_input()
-> Result<(), Box<dyn std::error::Error>> {
    let document = publication::parse_evidence(include_bytes!(
        "../../../../spec/examples/publication-evidence.json"
    ))?;
    let input = (&document, publication::parse_evidence);
    let payload = &document.payload;
    let deployment = &payload.deployment;
    let record = &deployment.record;
    let workflow = &deployment.workflow;
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
            &payload.producer,
            deployment,
            &payload.docs,
            &payload.target,
            &payload.site,
            &payload.product,
        ),
    )?;
    assert_object_required(
        input,
        deployment,
        (
            deployment.outcome,
            record,
            workflow,
            deployment.provider_run_attempt,
        ),
    )?;
    assert_object_required(input, record, (&record.uri, record.digest))?;
    assert_object_required(input, workflow, (&workflow.uri, workflow.digest))?;

    let document = publication::parse_assessment(include_bytes!(
        "../../../../spec/examples/publication-assessment.json"
    ))?;
    let input = (&document, publication::parse_assessment);
    let payload = &document.payload;
    let engine = &payload.engine;
    let subject = &payload.subject;
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
            engine,
            subject,
            payload.verdict,
            &payload.reasons,
        ),
    )?;
    assert_object_required(
        input,
        engine,
        (&engine.engine_version, engine.engine_digest),
    )?;
    assert_object_required(
        input,
        subject,
        (
            subject.report_payload_digest,
            subject.plan_payload_digest,
            subject.evidence_payload_digest,
        ),
    )?;
    Ok(())
}
