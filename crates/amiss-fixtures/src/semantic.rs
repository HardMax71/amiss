use amiss_wire::json::Value as WireValue;

/// Builds a digest-true passing report over the supplied semantic payloads.
#[must_use]
pub fn semantic_report(payload_digests: &[amiss_wire::digest::Digest]) -> Vec<u8> {
    let semantic_evidence = payload_digests
        .iter()
        .map(|digest| {
            WireValue::object(vec![(
                "payload_digest".to_owned(),
                WireValue::string(digest.to_string()),
            )])
        })
        .collect();
    let payload = WireValue::object(vec![
        (
            "compatibility".to_owned(),
            WireValue::string(amiss_wire::report::COMPATIBILITY.to_owned()),
        ),
        (
            "controls".to_owned(),
            WireValue::object(vec![(
                "semantic_evidence".to_owned(),
                WireValue::array(semantic_evidence),
            )]),
        ),
        (
            "result".to_owned(),
            WireValue::object(vec![
                ("complete".to_owned(), WireValue::Bool(true)),
                ("exit_code".to_owned(), WireValue::Integer(0)),
                ("status".to_owned(), WireValue::string("pass".to_owned())),
            ]),
        ),
        (
            "schema".to_owned(),
            WireValue::string(amiss_wire::report::PAYLOAD_SCHEMA.to_owned()),
        ),
    ]);
    amiss_wire::json::canonical(&WireValue::object(vec![
        ("payload".to_owned(), payload.clone()),
        (
            "payload_digest".to_owned(),
            WireValue::string(
                amiss_wire::digest::hj(amiss_wire::report::PAYLOAD_SCHEMA, &payload).to_string(),
            ),
        ),
        (
            "schema".to_owned(),
            WireValue::string(amiss_wire::report::ENVELOPE_SCHEMA.to_owned()),
        ),
    ]))
}

/// Builds one record-set semantic observation.
#[must_use]
pub fn record_set(name: &str, records: &[(&str, &str)]) -> serde_json::Value {
    serde_json::json!({
        "kind": "record-set",
        "name": name,
        "records": records
            .iter()
            .map(|(key, value)| serde_json::json!({"key": key, "value": value}))
            .collect::<Vec<_>>()
    })
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
pub fn site_observation(route: &str, observation: SiteObservation<'_>) -> serde_json::Value {
    match observation {
        SiteObservation::Page(source, anchors) => serde_json::json!({
            "kind": "site-route",
            "route": route,
            "source": source,
            "anchors": anchors,
        }),
        SiteObservation::Generated(source, anchors) => serde_json::json!({
            "kind": "site-generated-route",
            "route": route,
            "source": source,
            "anchors": anchors,
        }),
        SiteObservation::Redirect(source, destination) => serde_json::json!({
            "kind": "site-redirect",
            "route": route,
            "source": source,
            "destination": destination,
        }),
    }
}

/// Builds one canonical site navigation observation.
#[must_use]
pub fn site_navigation(
    root: Option<&str>,
    manifest: &str,
    entrypoints: &[&str],
    reachable: &[&str],
) -> serde_json::Value {
    serde_json::json!({
        "kind": "site-navigation",
        "root": root,
        "manifest": manifest,
        "entrypoints": entrypoints,
        "reachable": reachable,
    })
}
