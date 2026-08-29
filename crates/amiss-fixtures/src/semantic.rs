use amiss_wire::json::Value;

/// Builds a digest-true passing report over the supplied semantic payloads.
#[must_use]
pub fn semantic_report(payload_digests: &[amiss_wire::digest::Digest]) -> Vec<u8> {
    let semantic_evidence = payload_digests
        .iter()
        .map(|digest| {
            Value::object(vec![(
                "payload_digest".to_owned(),
                Value::string(digest.to_string()),
            )])
        })
        .collect();
    let payload = Value::object(vec![
        (
            "compatibility".to_owned(),
            Value::string(amiss_wire::report::COMPATIBILITY.to_owned()),
        ),
        (
            "controls".to_owned(),
            Value::object(vec![(
                "semantic_evidence".to_owned(),
                Value::array(semantic_evidence),
            )]),
        ),
        (
            "result".to_owned(),
            Value::object(vec![
                ("complete".to_owned(), Value::Bool(true)),
                ("exit_code".to_owned(), Value::Integer(0)),
                ("status".to_owned(), Value::string("pass".to_owned())),
            ]),
        ),
        (
            "schema".to_owned(),
            Value::string(amiss_wire::report::PAYLOAD_SCHEMA.to_owned()),
        ),
    ]);
    amiss_wire::json::canonical(&Value::object(vec![
        ("payload".to_owned(), payload.clone()),
        (
            "payload_digest".to_owned(),
            Value::string(
                amiss_wire::digest::hj(amiss_wire::report::PAYLOAD_SCHEMA, &payload).to_string(),
            ),
        ),
        (
            "schema".to_owned(),
            Value::string(amiss_wire::report::ENVELOPE_SCHEMA.to_owned()),
        ),
    ]))
}

/// Builds one record-set semantic observation.
#[must_use]
pub fn record_set(name: &str, records: &[(&str, &str)]) -> Value {
    Value::object(vec![
        ("kind".to_owned(), Value::string("record-set".to_owned())),
        ("name".to_owned(), Value::string(name.to_owned())),
        (
            "records".to_owned(),
            Value::array(
                records
                    .iter()
                    .map(|(key, value)| {
                        Value::object(vec![
                            ("key".to_owned(), Value::string((*key).to_owned())),
                            ("value".to_owned(), Value::string((*value).to_owned())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

/// A typed site-build observation fixture.
#[derive(Clone, Copy)]
pub enum SiteObservation<'a> {
    Page(&'a str, &'a [&'a str]),
    Generated(Option<&'a str>, &'a [&'a str]),
    Redirect(&'a str, &'a str),
}

/// Builds one site-build semantic observation.
#[must_use]
pub fn site_observation(route: &str, observation: SiteObservation<'_>) -> Value {
    let mut members = vec![("route".to_owned(), Value::string(route.to_owned()))];
    match observation {
        SiteObservation::Page(source, anchors) => {
            members.push(("kind".to_owned(), Value::string("site-route".to_owned())));
            members.push(("source".to_owned(), Value::string(source.to_owned())));
            members.push((
                "anchors".to_owned(),
                Value::array(
                    anchors
                        .iter()
                        .map(|anchor| Value::string((*anchor).to_owned()))
                        .collect(),
                ),
            ));
        }
        SiteObservation::Generated(source, anchors) => {
            members.push((
                "kind".to_owned(),
                Value::string("site-generated-route".to_owned()),
            ));
            members.push((
                "source".to_owned(),
                source.map_or(Value::Null, |source| Value::string(source.to_owned())),
            ));
            members.push((
                "anchors".to_owned(),
                Value::array(
                    anchors
                        .iter()
                        .map(|anchor| Value::string((*anchor).to_owned()))
                        .collect(),
                ),
            ));
        }
        SiteObservation::Redirect(source, destination) => {
            members.push(("kind".to_owned(), Value::string("site-redirect".to_owned())));
            members.push(("source".to_owned(), Value::string(source.to_owned())));
            members.push((
                "destination".to_owned(),
                Value::string(destination.to_owned()),
            ));
        }
    }
    Value::object(members)
}

/// Builds one canonical site navigation observation.
#[must_use]
pub fn site_navigation(
    root: Option<&str>,
    manifest: &str,
    entrypoints: &[&str],
    reachable: &[&str],
) -> Value {
    Value::object(vec![
        (
            "entrypoints".to_owned(),
            Value::array(
                entrypoints
                    .iter()
                    .map(|route| Value::string((*route).to_owned()))
                    .collect(),
            ),
        ),
        (
            "kind".to_owned(),
            Value::string("site-navigation".to_owned()),
        ),
        ("manifest".to_owned(), Value::string(manifest.to_owned())),
        (
            "reachable".to_owned(),
            Value::array(
                reachable
                    .iter()
                    .map(|source| Value::string((*source).to_owned()))
                    .collect(),
            ),
        ),
        (
            "root".to_owned(),
            root.map_or(Value::Null, |root| Value::string(root.to_owned())),
        ),
    ])
}
