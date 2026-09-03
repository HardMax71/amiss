use amiss_wire::json::Value;

/// A minimal complete scanner report whose candidate side introduces the
/// given external destinations, digest-true, for producer and lane tests.
#[must_use]
pub fn external_report(destinations: &[&str]) -> Vec<u8> {
    let rows: Vec<String> = destinations
        .iter()
        .map(|destination| {
            let scheme = destination.split(':').next().unwrap_or("https");
            format!(
                r#"{{"base":null,"candidate":{{"document":"docs/a.md","external_destination":"{destination}","intent":{{"external_scheme":"{scheme}"}},"resolution":{{"kind":"external","reason":"url"}}}}}}"#
            )
        })
        .collect();
    let payload = format!(
        r#"{{"compatibility":"1","engine":{{"engine_digest":"{digest}","engine_version":"0.0.0"}},"evaluation":{{"base":{{"commit_oid":"a"}},"candidate":{{"commit_oid":"b"}},"mode":"commit-pair"}},"observations":[{rows}],"result":{{"complete":true,"exit_code":0,"status":"pass"}},"schema":"amiss/scanner-report-payload"}}"#,
        digest = amiss_wire::digest::hb("amiss/fixture-engine", b"fixture"),
        rows = rows.join(","),
    );
    // The spelling above is already canonical, so the byte hash is the
    // payload digest.
    let payload_digest = amiss_wire::digest::hb("amiss/scanner-report-payload", payload.as_bytes());
    format!(
        r#"{{"payload":{payload},"payload_digest":"{payload_digest}","schema":"amiss/scanner-report-envelope"}}"#
    )
    .into_bytes()
}

/// Derives the external plan from the shared digest-true report fixture.
#[must_use]
pub fn external_plan(destinations: &[&str]) -> Option<Value> {
    let parsed = amiss_wire::json::parse(&external_report(destinations)).ok()?;
    let engine = parsed.member("payload")?.member("engine")?;
    amiss_wire::external::plan(
        &parsed,
        engine.text("engine_version")?,
        amiss_wire::digest::Digest::from_wire(engine.text("engine_digest")?)?,
    )
    .ok()
}

/// Flattens forge evidence rows into the facts provider tests compare.
#[must_use]
pub fn external_facts(evidence: &Value) -> Option<Vec<String>> {
    let Value::Array(rows) = evidence.member("rows")? else {
        return None;
    };
    rows.iter()
        .map(|row| {
            let destination = row.text("destination")?;
            let repository = row.text("repository")?;
            Some(match row.text("tail") {
                Some(tail) => format!("{destination} {repository} {tail}"),
                None => format!("{destination} {repository}"),
            })
        })
        .collect()
}
