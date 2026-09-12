use amiss_wire::{controls, de::ErrorKind, digest::CanonicalJsonError, semantic};

#[test]
fn policy_reader_keeps_nested_records_object_shaped() -> Result<(), Box<dyn std::error::Error>> {
    #[derive(serde::Serialize)]
    struct RepeatedPolicy<'a> {
        #[serde(flatten)]
        first: &'a controls::ScannerPolicy,
        #[serde(flatten)]
        second: &'a controls::ScannerPolicy,
    }

    let mut policy = controls::parse_scanner_policy(include_bytes!(
        "../../../../spec/examples/scanner-policy.json"
    ))?;
    policy.document_includes.push(controls::DocumentInclude {
        path: "docs".parse().unwrap(),
        kind: controls::IncludeKind::Tree,
        suffix: Some(".md".to_owned()),
        adapter: None,
    });
    controls::canonical_scanner_policy(&policy)?;
    let include = &policy.document_includes[0];
    super::input::assert_object_required(
        (&policy, controls::parse_scanner_policy),
        include,
        (
            &include.path,
            include.kind,
            &include.suffix,
            include.adapter,
        ),
    )?;

    let text = serde_json::to_string(&policy)?;
    let repeated = serde_json::to_vec(&RepeatedPolicy {
        first: &policy,
        second: &policy,
    })?;
    assert!(
        matches!(controls::parse_scanner_policy(&repeated).unwrap_err().kind,
        ErrorKind::Deserialize(source) if source.is_data())
    );
    for (valid, invalid) in [
        (
            serde_json::to_string(include)?,
            r#"{"path":"docs","kind":"tree","suffix":null}"#.to_owned(),
        ),
        (
            serde_json::to_string(include)?,
            r#"{"path":"docs","kind":"tree","suffix":".md","adapter":null}"#.to_owned(),
        ),
    ] {
        let changed = text.replace(&valid, &invalid);
        assert_ne!(changed, text);
        assert!(matches!(
            controls::parse_scanner_policy(changed.as_bytes())
                .unwrap_err()
                .kind,
            ErrorKind::Canonical(CanonicalJsonError::InputChanged)
        ));
    }
    Ok(())
}

#[test]
fn debt_and_fact_readers_keep_nested_objects_without_changing_identities()
-> Result<(), Box<dyn std::error::Error>> {
    let debt = controls::parse_debt_snapshot(include_bytes!(
        "../../../../spec/examples/debt-snapshot.json"
    ))?;
    let tree = &debt.adoption_tree;
    super::input::assert_object_required(
        (&debt, controls::parse_debt_snapshot),
        tree,
        (tree.object_format, &tree.tree_oid),
    )?;
    let fact = &debt.items[0].accepted_fact;
    let evidence = &fact.evidence;
    for (object, positional) in [
        (
            serde_json::to_string(evidence)?,
            serde_json::to_string(&(
                evidence.kind,
                &evidence.resolution,
                evidence.occurrence_multiplicity,
            ))?,
        ),
        (
            serde_json::to_string(&fact.key_input.scope.occurrence)?,
            serde_json::to_string(&(
                fact.key_input.scope.occurrence.kind,
                fact.key_input.scope.occurrence.source_projection_digest,
            ))?,
        ),
    ] {
        let text = serde_json::to_string(&debt)?;
        let changed = text.replace(&object, &positional);
        assert_ne!(changed, text);
        assert!(matches!(
            controls::parse_debt_snapshot(changed.as_bytes())
                .unwrap_err()
                .kind,
            ErrorKind::Canonical(CanonicalJsonError::InputChanged)
        ));
        let text = serde_json::to_string(fact)?;
        let changed = text.replace(&object, &positional);
        assert_ne!(changed, text);
        assert!(matches!(
            controls::parse_fact(changed.as_bytes()).unwrap_err().kind,
            ErrorKind::Canonical(CanonicalJsonError::InputChanged)
        ));
    }
    let (bytes, digest) = controls::canonical_debt_snapshot(&debt)?;
    let replay = controls::parse_debt_snapshot(&bytes)?;
    assert_eq!(replay, debt);
    assert_eq!(controls::canonical_debt_snapshot(&replay)?.1, digest);
    Ok(())
}

#[test]
fn floor_and_waiver_readers_keep_repository_objects() -> Result<(), Box<dyn std::error::Error>> {
    let floor = controls::parse_organization_floor(include_bytes!(
        "../../../../spec/examples/organization-floor.json"
    ))
    .unwrap();
    let waiver = controls::parse_waiver_bundle(include_bytes!(
        "../../../../spec/examples/waiver-bundle.json"
    ))?;
    let repository = &floor.repository;
    let object = serde_json::to_string(repository)?;
    let positional =
        serde_json::to_string(&(repository.host(), repository.name(), repository.owner()))?;
    let text = serde_json::to_string(&floor)?;
    let changed = text.replace(&object, &positional);
    assert_ne!(changed, text);
    let result = controls::parse_organization_floor(changed.as_bytes());
    assert!(
        matches!(
            result,
            Err(controls::FloorDefect::Schema(amiss_wire::de::Error {
                kind: ErrorKind::Canonical(CanonicalJsonError::InputChanged),
                ..
            }))
        ),
        "{result:?}"
    );
    let repository = &waiver.repository;
    super::input::assert_object_required(
        (&waiver, controls::parse_waiver_bundle),
        repository,
        (repository.host(), repository.name(), repository.owner()),
    )?;
    let tree = &waiver.items[0].candidate_tree;
    super::input::assert_object_required(
        (&waiver, controls::parse_waiver_bundle),
        tree,
        (tree.object_format, &tree.tree_oid),
    )?;
    Ok(())
}

#[test]
fn semantic_templates_reject_positional_records_at_the_shared_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let template = semantic::parse_template(include_bytes!(
        "../../../../spec/examples/scanner-semantic-template.json"
    ))?;
    let producer = &template.producer;
    super::input::assert_object_required(
        (&template, semantic::parse_template),
        producer,
        (
            producer.kind,
            &producer.identity,
            &producer.version,
            producer.context_digest,
            producer.input_digest,
        ),
    )?;
    let text = serde_json::to_string(&template)?;
    let semantic::observation::Observation::Record(observation) = template.observations[0].as_ref()
    else {
        return Err("the published template must exercise record observations".into());
    };
    assert!(!observation.records.is_empty());
    for record in &observation.records {
        let object = serde_json::to_string(record)?;
        let positional = serde_json::to_string(&(&record.key, &record.value))?;
        let changed = text.replace(&object, &positional);
        assert_ne!(changed, text);
        assert!(matches!(
            semantic::parse_template(changed.as_bytes())
                .unwrap_err()
                .kind,
            ErrorKind::Canonical(CanonicalJsonError::InputChanged)
        ));
    }
    Ok(())
}

#[test]
fn semantic_evidence_and_record_inputs_share_complete_typed_ingress()
-> Result<(), Box<dyn std::error::Error>> {
    let evidence = semantic::parse(include_bytes!(
        "../../../../spec/examples/scanner-semantic-evidence.json"
    ))?;
    let producer = &evidence.payload.producer;
    super::input::assert_object_required(
        (&evidence, semantic::parse),
        producer,
        (
            producer.kind,
            &producer.identity,
            &producer.version,
            producer.context_digest,
            producer.input_digest,
        ),
    )?;
    let input = semantic::record::parse_input(include_bytes!(
        "../../../../spec/examples/scanner-record-set-input.json"
    ))?;
    let record = &input.records[0];
    super::input::assert_object_required(
        (&input, semantic::record::parse_input),
        record,
        (&record.key, &record.value),
    )
}
