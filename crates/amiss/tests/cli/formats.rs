use std::fs;
use std::path::Path;

use crate::support::{amiss, fixture};

/// Four suites validate a report against the frozen schema, and every one of them
/// builds that report in process. Nothing had ever read the bytes the binary
/// prints, which is the only artifact a caller ever sees. Those bytes are exactly
/// `JCS(envelope)` and one LF: canonical JSON puts the whole envelope on a single
/// line, so the trailing newline is the only newline in the stream. The serializer
/// is shared, so this passes the day it is written. What it buys is that it cannot
/// quietly stop passing.
#[test]
fn the_bytes_the_binary_prints_are_a_schema_clean_report() {
    let fx = fixture();
    let (code, stdout, stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.base,
        "--candidate",
        &fx.candidate,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!(
        (code, stderr.as_str()),
        (0, ""),
        "a complete accepted projection leaves stderr empty"
    );
    let (last, rest) = stdout.split_last().expect("the report is not empty");
    assert_eq!(*last, b'\n', "the report ends in an LF");
    assert!(
        !rest.contains(&b'\n'),
        "the canonical envelope is one line, so its LF is the only one"
    );

    let schema_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/scanner-report.schema.json"),
    )
    .unwrap();
    let schema: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let defects: Vec<String> = validator
        .iter_errors(&envelope)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert_eq!(
        defects,
        Vec::<String>::new(),
        "the bytes the binary printed are a schema-clean report"
    );
}

#[test]
fn an_asciidoc_report_is_schema_clean() {
    let fx = amiss_fixtures::commit_pair(
        &[(
            "docs/manual.adoc",
            "= Manual\n\n[[here]]\n== Part\n\nSee xref:manual.adoc[Self], <<here>>, \
             xref:other-page#part[Page], link:{base}/x.adoc[Attr].\n\n\
             image::img/logo.png[Logo]\n\ninclude::shared.adoc[]\n",
        )],
        &[(
            "docs/manual.adoc",
            "= Manual\n\n[[here]]\n== Part\n\nSee xref:manual.adoc[Self], <<here>>, \
             xref:other-page#part[Page], link:{base}/x.adoc[Attr], xref:gone.adoc[Gone].\n\n\
             image::img/logo.png[Logo]\n\ninclude::shared.adoc[]\n",
        )],
    )
    .unwrap();
    let (code, stdout, stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.base,
        "--candidate",
        &fx.candidate,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));

    let schema_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/scanner-report.schema.json"),
    )
    .unwrap();
    let schema: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let defects: Vec<String> = validator
        .iter_errors(&envelope)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert_eq!(
        defects,
        Vec::<String>::new(),
        "an AsciiDoc observation is a schema-clean report"
    );

    let kinds: Vec<&str> = envelope["payload"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|finding| finding["kind"].as_str())
        .collect();
    assert!(
        kinds.contains(&"unsupported-reference-semantics"),
        "an attribute, an image, and a page identity are declared: {kinds:?}"
    );
    assert!(
        kinds.contains(&"explicit-target-missing"),
        "a named .adoc file the tree does not hold is still missing: {kinds:?}"
    );
}

#[test]
fn a_restructuredtext_report_is_schema_clean() {
    let fx = amiss_fixtures::commit_pair(
        &[(
            "docs/manual.rst",
            "Manual\n======\n\n.. _here:\n\nSee `guide <guide.rst>`_ and `gone <missing.rst>`_.\n\n\
             .. _named: other.rst\n\n.. image:: img/logo.png\n",
        )],
        &[(
            "docs/manual.rst",
            "Manual\n======\n\n.. _here:\n\nSee `guide <guide.rst>`_ and `gone <missing.rst>`_.\n\n\
             .. _named: other.rst\n\n.. image:: img/logo.png\n\n.. include:: shared.rst\n",
        )],
    )
    .unwrap();
    let (code, stdout, stderr) = amiss(&[
        "check",
        "--repo",
        &fx.repo,
        "--object-format",
        "sha1",
        "--base",
        &fx.base,
        "--candidate",
        &fx.candidate,
        "--profile",
        "observe",
        "--format",
        "json",
    ]);
    assert_eq!((code, stderr.as_str()), (0, ""));

    let schema_text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/scanner-report.schema.json"),
    )
    .unwrap();
    let schema: serde_json::Value = serde_json::from_str(&schema_text).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
    let defects: Vec<String> = validator
        .iter_errors(&envelope)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert_eq!(
        defects,
        Vec::<String>::new(),
        "a reStructuredText observation is a schema-clean report"
    );
}
