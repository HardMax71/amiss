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

    let original: SemanticEvidenceEnvelope = serde_json::from_slice(include_bytes!(
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
        assert!(serde_json::from_str::<Observation>(&unknown_member).is_err());
        let template = SemanticEvidenceTemplate {
            schema: TemplateSchema::Current,
            producer: original.payload.producer.clone(),
            complete: true,
            observations: vec![observation].into(),
        };
        let template_bytes = semantic::template(template.clone()).unwrap();
        assert_eq!(semantic::parse_template(&template_bytes).unwrap(), template);
        let (document, envelope_bytes) = semantic::bind_template(
            &template,
            original.payload.subject.candidate_identity_digest,
        )
        .unwrap();
        assert_eq!(
            semantic::parse(&envelope_bytes)
                .unwrap()
                .payload
                .observations,
            template.observations.as_ref()
        );
        let malformed_template = String::from_utf8(template_bytes)
            .unwrap()
            .replace(&text, &unknown_member);
        assert!(semantic::parse_template(malformed_template.as_bytes()).is_err());
        let malformed_payload =
            String::from_utf8(serde_json_canonicalizer::to_vec(&document.payload).unwrap())
                .unwrap()
                .replace(&text, &unknown_member);
        let malformed_digest = hb(semantic::PAYLOAD_SCHEMA, malformed_payload.as_bytes());
        let malformed_envelope = format!(
            r#"{{"schema":"amiss/semantic-evidence-envelope","payload":{malformed_payload},"payload_digest":"{malformed_digest}"}}"#
        );
        assert_eq!(
            semantic::parse(malformed_envelope.as_bytes())
                .unwrap_err()
                .kind,
            amiss_wire::de::ErrorKind::InvalidValue
        );
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
