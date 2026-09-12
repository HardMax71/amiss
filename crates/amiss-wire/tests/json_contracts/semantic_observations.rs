use amiss_wire::{
    assessment::Nullable,
    digest::hb,
    semantic::{
        self, SemanticEvidenceEnvelope, SemanticEvidenceTemplate, TemplateSchema,
        observation::{Observation, SiteBuildObservation, SphinxLabelKind, SphinxLabelObservation},
        record,
    },
};

#[expect(
    clippy::unwrap_used,
    reason = "the fixture constructs known-valid observation models"
)]
fn observations() -> Vec<(Observation, Vec<u8>)> {
    let sites = [
        SiteBuildObservation::Route {
            route: "/guide".to_owned(),
            source: "docs/guide.md".parse().unwrap(),
            anchors: vec!["intro".to_owned()],
        },
        SiteBuildObservation::GeneratedRoute {
            route: "/index".to_owned(),
            source: Nullable::Null,
            anchors: Vec::new(),
        },
        SiteBuildObservation::Redirect {
            route: "/old".to_owned(),
            source: "docs/guide.md".parse().unwrap(),
            destination: "/guide".to_owned(),
        },
        SiteBuildObservation::Navigation {
            root: Nullable::Null,
            manifest: "SUMMARY.md".parse().unwrap(),
            entrypoints: vec!["/guide".to_owned()],
            reachable: vec!["docs/guide.md".parse().unwrap()],
        },
    ];
    let mut cases = sites
        .into_iter()
        .map(|site| {
            let bytes = serde_json_canonicalizer::to_vec(&site).unwrap();
            (Observation::Site(site), bytes)
        })
        .collect::<Vec<_>>();
    let label = SphinxLabelObservation {
        kind: SphinxLabelKind::Current,
        inventory: "python".parse().unwrap(),
        name: "context managers".to_owned(),
        destination: "https://docs.python.org/reference/datamodel.html".to_owned(),
    };
    let expected = serde_json_canonicalizer::to_vec(&label).unwrap();
    cases.push((Observation::Sphinx(label), expected));
    let records = record::Observation {
        kind: record::ObservationKind::Current,
        name: "rust/api".parse().unwrap(),
        records: vec![record::Record {
            key: "amiss::check".to_owned(),
            value: "pub fn check()".to_owned(),
        }],
    };
    let expected = serde_json_canonicalizer::to_vec(&records).unwrap();
    cases.push((Observation::Record(records), expected));
    cases
}

#[test]
fn semantic_observations_reuse_closed_models_without_changing_their_json() {
    #[derive(serde::Serialize)]
    struct ExtendedObservation<'a> {
        #[serde(flatten)]
        observation: &'a Observation,
        unexpected: bool,
    }

    #[derive(serde::Serialize)]
    struct RepeatedObservation<'a> {
        #[serde(flatten)]
        first: &'a Observation,
        #[serde(flatten)]
        second: &'a Observation,
    }

    let original: SemanticEvidenceEnvelope<'static> = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-semantic-evidence.json"
    ))
    .unwrap();

    for (observation, expected) in observations() {
        assert_eq!(
            serde_json_canonicalizer::to_vec(&observation).unwrap(),
            expected
        );
        assert_eq!(
            serde_json::from_slice::<Observation>(&expected).unwrap(),
            observation
        );
        let text = String::from_utf8(expected).unwrap();
        let unknown_member = String::from_utf8(
            serde_json_canonicalizer::to_vec(&ExtendedObservation {
                observation: &observation,
                unexpected: true,
            })
            .unwrap(),
        )
        .unwrap();
        let repeated_members = serde_json::to_string(&RepeatedObservation {
            first: &observation,
            second: &observation,
        })
        .unwrap();
        let template = SemanticEvidenceTemplate {
            schema: TemplateSchema::Current,
            producer: original.payload.producer.clone(),
            complete: true,
            observations: vec![std::borrow::Cow::Owned(observation)].into(),
        };
        let template_bytes = semantic::template(template.clone()).unwrap();
        assert_eq!(semantic::parse_template(&template_bytes).unwrap(), template);
        let document = semantic::bind_template(
            &template,
            original.payload.subject.candidate_identity_digest,
        )
        .unwrap();
        let mut envelope_bytes = Vec::new();
        amiss_wire::write_json(
            &document,
            &mut envelope_bytes,
            semantic::SEMANTIC_EVIDENCE_BYTES,
        )
        .unwrap();
        assert_eq!(
            semantic::parse(&envelope_bytes)
                .unwrap()
                .payload
                .observations,
            template.observations.as_ref()
        );
        let template_text = String::from_utf8(template_bytes).unwrap();
        for invalid in [unknown_member, repeated_members] {
            assert!(
                serde_json::from_str::<Observation>(&invalid)
                    .unwrap_err()
                    .is_data()
            );
            let malformed_template = template_text.replace(&text, &invalid);
            let error = semantic::parse_template(malformed_template.as_bytes()).unwrap_err();
            assert!(matches!(error.kind,
                amiss_wire::de::ErrorKind::Deserialize(source) if source.is_data()));
            let malformed_payload =
                String::from_utf8(serde_json_canonicalizer::to_vec(&document.payload).unwrap())
                    .unwrap()
                    .replace(&text, &invalid);
            let malformed_digest = hb(semantic::PAYLOAD_SCHEMA, malformed_payload.as_bytes());
            let malformed_envelope = format!(
                r#"{{"schema":"amiss/semantic-evidence-envelope","payload":{malformed_payload},"payload_digest":"{malformed_digest}"}}"#
            );
            assert!(matches!(
                semantic::parse(malformed_envelope.as_bytes())
                    .unwrap_err()
                    .kind,
                amiss_wire::de::ErrorKind::Deserialize(source) if source.is_data()
            ));
        }
    }
}

#[test]
fn semantic_observations_refuse_unknown_tags_and_positional_struct_forms() {
    let record_array = serde_json::to_vec(&(
        record::ObservationKind::Current,
        "rust/api",
        Vec::<record::Record>::new(),
    ))
    .unwrap();
    let label_array = serde_json::to_vec(&(
        SphinxLabelKind::Current,
        "python",
        "context managers",
        "https://docs.python.org/reference/datamodel.html",
    ))
    .unwrap();
    for invalid in [
        record_array.as_slice(),
        label_array.as_slice(),
        br#"{"kind":"future-fact","data":{"arbitrary":[true,null,2]}}"#,
        br#"{"kind":{"record-set":null},"name":"rust/api","records":[]}"#,
        br#"{"kind":"record-set","name":"rust/api","records":[{"key":"a","value":"A","extra":true}]}"#,
        br#"{"kind":"site-route","route":"/guide","anchors":[]}"#,
        br#"{"kind":"site-generated-route","route":"/index","anchors":[]}"#,
        b"null",
        b"[]",
    ] {
        assert!(
            serde_json::from_slice::<Observation>(invalid).is_err(),
            "{}",
            String::from_utf8_lossy(invalid)
        );
    }
}
