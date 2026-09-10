use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_github::webhook::{Installation, Workflow};

const INSTALLATION: &[u8] = include_bytes!("../fixtures/webhook-installation.json");
const WORKFLOW: &[u8] = include_bytes!("../fixtures/webhook-workflow.json");

#[test]
fn webhook_metadata_keeps_every_published_member_before_strict_decoding() {
    let installation: Installation = serde_json::from_slice(INSTALLATION).unwrap();
    let workflow: Workflow = serde_json::from_slice(WORKFLOW).unwrap();
    assert_eq!(installation.id, 1);
    assert_eq!(
        installation.node_id,
        "MDIzOkludGVncmF0aW9uSW5zdGFsbGF0aW9uMQ=="
    );
    assert_eq!(workflow.id, 2_823_525);
    assert_eq!(workflow.name, "test");
    assert_eq!(workflow.path, ".github/workflows/test.yml");
    assert_eq!(workflow.state, "active");
    for (input, output) in [
        (INSTALLATION, serde_json::to_vec(&installation).unwrap()),
        (WORKFLOW, serde_json::to_vec(&workflow).unwrap()),
    ] {
        assert_eq!(
            amiss_fixtures::canonical_json(input).unwrap(),
            amiss_fixtures::canonical_json(&output).unwrap()
        );
    }
    let (decoded, length): (Installation, _) =
        decode_bounded_json(INSTALLATION, None, INSTALLATION.len(), |bytes| {
            amiss_wire::read_json(bytes, u64::MAX)
        })
        .unwrap();
    assert_eq!(decoded, installation);
    assert_eq!(length, INSTALLATION.len());
    let (decoded, length): (Workflow, _) =
        decode_bounded_json(WORKFLOW, None, WORKFLOW.len(), |bytes| {
            amiss_wire::read_json(bytes, u64::MAX)
        })
        .unwrap();
    assert_eq!(decoded, workflow);
    assert_eq!(length, WORKFLOW.len());
    assert_eq!(
        decode_bounded_json::<Workflow, _>(WORKFLOW, None, WORKFLOW.len() - 1, |bytes| {
            amiss_wire::read_json(bytes, u64::MAX)
        }),
        Err(ProviderError::InvalidResponse)
    );
    assert_eq!(
        decode_bounded_json::<Installation, _>(
            INSTALLATION,
            None,
            INSTALLATION.len() - 1,
            |bytes| amiss_wire::read_json(bytes, u64::MAX)
        ),
        Err(ProviderError::InvalidResponse)
    );
}

#[test]
fn every_workflow_metadata_field_is_required_and_nonnull() {
    let workflow: Workflow = serde_json::from_slice(WORKFLOW).unwrap();
    let wire = serde_json::to_string(&workflow).unwrap();
    for (name, value) in [
        ("id", serde_json::to_string(&workflow.id).unwrap()),
        ("node_id", serde_json::to_string(&workflow.node_id).unwrap()),
        ("name", serde_json::to_string(&workflow.name).unwrap()),
        ("path", serde_json::to_string(&workflow.path).unwrap()),
        ("state", serde_json::to_string(&workflow.state).unwrap()),
        (
            "created_at",
            serde_json::to_string(&workflow.created_at).unwrap(),
        ),
        (
            "updated_at",
            serde_json::to_string(&workflow.updated_at).unwrap(),
        ),
        ("url", serde_json::to_string(&workflow.url).unwrap()),
        (
            "html_url",
            serde_json::to_string(&workflow.html_url).unwrap(),
        ),
        (
            "badge_url",
            serde_json::to_string(&workflow.badge_url).unwrap(),
        ),
    ] {
        let field = format!("\"{name}\":{value}");
        let absent = if name == "id" {
            wire.replacen(&format!("{field},"), "", 1)
        } else {
            wire.replacen(&format!(",{field}"), "", 1)
        };
        assert_ne!(absent, wire, "{name}");
        for changed in [
            absent,
            wire.replacen(&field, &format!("\"{name}\":null"), 1),
        ] {
            assert!(
                serde_json::from_str::<Workflow>(&changed).is_err(),
                "{name}"
            );
            assert!(
                amiss_wire::read_json::<Workflow>(changed.as_bytes(), u64::MAX).is_err(),
                "{name}"
            );
        }
    }
    for changed in [br#"{"id":1}"#.as_slice(), br#"{"node_id":"installation"}"#] {
        assert!(serde_json::from_slice::<Installation>(changed).is_err());
        assert!(amiss_wire::read_json::<Installation>(changed, u64::MAX).is_err());
    }
}

#[test]
fn metadata_uses_checked_integers_and_refuses_ambiguous_or_untyped_objects() {
    let installation: Installation = serde_json::from_slice(INSTALLATION).unwrap();
    let workflow: Workflow = serde_json::from_slice(WORKFLOW).unwrap();
    let sources = [
        serde_json::to_string(&installation).unwrap(),
        serde_json::to_string(&workflow).unwrap(),
    ];
    for (wire, id) in sources.iter().zip([installation.id, workflow.id]) {
        let field = format!("\"id\":{id}");
        for replacement in [
            r#""id":9007199254740992"#,
            r#""id":-1"#,
            r#""id":1.5"#,
            r#""id":null"#,
            r#""id":"1""#,
        ] {
            let changed = wire.replacen(&field, replacement, 1);
            assert!(serde_json::from_str::<Installation>(&changed).is_err());
            assert!(serde_json::from_str::<Workflow>(&changed).is_err());
        }
        for changed in [
            wire.replacen('{', r#"{"unknown":true,"#, 1),
            wire.replacen('{', r#"{"\u0069d":1,"#, 1),
            wire.replacen(&field, r#""id":-0"#, 1),
            wire.replacen(&field, r#""id":1e0"#, 1),
            wire.replacen(&field, r#""id":1.0"#, 1),
            format!("{wire} null"),
            format!("[{wire}]"),
            "null".to_owned(),
        ] {
            assert!(amiss_wire::read_json::<Installation>(changed.as_bytes(), u64::MAX).is_err());
            assert!(amiss_wire::read_json::<Workflow>(changed.as_bytes(), u64::MAX).is_err());
        }
    }
    let large = Installation {
        id: 9_007_199_254_740_991,
        ..installation
    };
    let bytes = serde_json::to_vec(&large).unwrap();
    assert_eq!(
        amiss_wire::read_json::<Installation>(&bytes, u64::MAX).unwrap(),
        large
    );
    let large = Workflow {
        id: 9_007_199_254_740_991,
        ..workflow
    };
    let bytes = serde_json::to_vec(&large).unwrap();
    assert_eq!(
        amiss_wire::read_json::<Workflow>(&bytes, u64::MAX).unwrap(),
        large
    );
}
