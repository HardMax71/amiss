#![expect(
    clippy::unwrap_used,
    reason = "the fixture constructs known-valid bounded inventories"
)]

use std::io::Write as _;

use amiss_controller::{
    INTERSPHINX_INVENTORY_BYTES, IntersphinxError, IntersphinxInventory, bind_semantic_evidence,
    intersphinx_evidence,
};
use amiss_wire::digest::hb;
use flate2::Compression;
use flate2::write::ZlibEncoder;

fn inventory(body: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(body).unwrap();
    let compressed = encoder.finish().unwrap();
    let mut inventory = b"# Sphinx inventory version 2\n# Project: Test\n# Version: 1\n# The remainder of this file is compressed using zlib.\n".to_vec();
    inventory.extend_from_slice(&compressed);
    inventory
}

fn input(identity: &str, base_url: &str, body: &[u8]) -> IntersphinxInventory {
    IntersphinxInventory {
        identity: identity.to_owned(),
        base_url: base_url.to_owned(),
        bytes: inventory(body),
    }
}

pub(super) fn evidence() -> Vec<amiss_controller::SemanticEvidenceTemplate> {
    intersphinx_evidence(vec![input(
        "python",
        "https://docs.python.org/3",
        b"except_star std:label -1 reference/compound_stmts.html#except-star -\nignored py:function 1 library/functions.html#ignored -\nforeign django:setting 1 ref/settings.html#foreign -\ntestcase-objects std:label -1 library/unittest.html#testcase-objects -\n",
    )])
    .unwrap()
}

#[test]
fn a_bounded_inventory_becomes_only_complete_label_evidence() {
    let evidence = evidence();
    let candidate = hb("amiss/test-candidate", b"candidate");
    let bound = bind_semantic_evidence(&evidence, &[], &[], candidate).unwrap();
    let parsed = amiss_wire::semantic::parse(
        &serde_json::to_vec(&bound.supplied.first().unwrap().value).unwrap(),
    )
    .unwrap();

    assert_eq!(parsed.payload.subject.candidate_identity_digest, candidate);
    assert!(parsed.payload.complete);
    assert_eq!(parsed.payload.observations.len(), 2);
    assert!(parsed.payload.observations.iter().any(|row| {
        row.get("name").and_then(serde_json::Value::as_str) == Some("except_star")
            && row.get("destination").and_then(serde_json::Value::as_str)
                == Some("https://docs.python.org/3/reference/compound_stmts.html#except-star")
    }));
    assert!(parsed.payload.observations.iter().any(|row| {
        row.get("name").and_then(serde_json::Value::as_str) == Some("testcase-objects")
            && row.get("destination").and_then(serde_json::Value::as_str)
                == Some("https://docs.python.org/3/library/unittest.html#testcase-objects")
    }));
}

#[test]
fn invalid_or_partial_inventory_input_never_produces_evidence() {
    assert!(intersphinx_evidence(Vec::new()).unwrap().is_empty());
    let duplicate = vec![
        input(
            "python",
            "https://docs.python.org/3/",
            b"one std:label -1 one.html -\n",
        ),
        input(
            "python",
            "https://docs.python.org/3/",
            b"two std:label -1 two.html -\n",
        ),
    ];
    assert!(intersphinx_evidence(duplicate).is_err());
    assert!(
        intersphinx_evidence(vec![input(
            "python",
            "https://docs.python.org/3/",
            b"one std:label -1 one.html -\none std:label -1 one.html -\n",
        )])
        .is_err()
    );
    assert!(
        intersphinx_evidence(vec![input(
            "python",
            "file:///tmp/inventory/",
            b"one std:label -1 one.html -\n",
        )])
        .is_err()
    );

    let mut trailing = input(
        "python",
        "https://docs.python.org/3/",
        b"one std:label -1 one.html -\n",
    );
    trailing.bytes.extend_from_slice(b"unbound trailing bytes");
    assert!(intersphinx_evidence(vec![trailing]).is_err());

    let mut truncated = input(
        "python",
        "https://docs.python.org/3/",
        b"alpha std:label -1 a.html -\nbeta std:label -1 b.html -\ngamma std:label -1 c.html -\n",
    );
    truncated.bytes.pop();
    assert!(intersphinx_evidence(vec![truncated]).is_err());

    let bomb = vec![b'a'; 16_777_217];
    assert!(
        intersphinx_evidence(vec![input("python", "https://docs.python.org/3/", &bomb,)]).is_err()
    );

    let half_ceiling = usize::try_from(INTERSPHINX_INVENTORY_BYTES / 2).unwrap();
    let oversized_set = ["one", "two"].map(|identity| IntersphinxInventory {
        identity: identity.to_owned(),
        base_url: "https://docs.python.org/3/".to_owned(),
        bytes: vec![0; half_ceiling + 1],
    });
    assert!(matches!(
        intersphinx_evidence(Vec::from(oversized_set)),
        Err(IntersphinxError::InventoryBytes)
    ));
}

#[test]
fn destinations_share_the_consumer_grammar_and_stay_beneath_the_base() {
    for location in [
        "page.html#100%",
        "//evil.example/page.html",
        "https://evil.example/page.html",
        "../page.html",
    ] {
        let body = format!("bad-label std:label -1 {location} -\n");
        assert!(matches!(
            intersphinx_evidence(vec![input(
                "python",
                "https://docs.python.org/3/",
                body.as_bytes(),
            )]),
            Err(IntersphinxError::Destination)
        ));
    }
    assert!(matches!(
        intersphinx_evidence(vec![input(
            "python",
            "https://docs.python.org/100%",
            b"label std:label -1 page.html -\n",
        )]),
        Err(IntersphinxError::BaseUrl)
    ));
}
