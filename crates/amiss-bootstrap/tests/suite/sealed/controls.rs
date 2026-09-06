use amiss_bootstrap::supervise::{AcceptanceDefect, accept};
use amiss_wire::json::{Value, parse};

use super::{Deviation, FLOOR_DIGEST, FOREIGN_DIGEST, Patch, entry, golden, refused, set, string};

#[test]
fn sealed_controls_require_objects_not_positional_arrays() {
    let paths: &[&[&str]] = &[
        &["controls"],
        &["controls", "organization_floor"],
        &["controls", "debt_snapshot"],
        &["controls", "waiver_bundle"],
        &["controls", "execution_constraint"],
        &["controls", "execution_constraint", "descriptor"],
        &["controls", "trusted_time_source"],
        &["controls", "trusted_time_source", "statement"],
        &["controls", "sandbox"],
    ];
    for &path in paths {
        let deviation = Deviation::post(move |payload| {
            let value = path.iter().fold(payload, |value, key| entry(value, key));
            let Value::Object(members) = std::mem::replace(value, Value::Null) else {
                panic!("an object fixture at {path:?}");
            };
            *value = Value::array(
                members
                    .into_vec()
                    .into_iter()
                    .map(|(_, value)| value)
                    .collect(),
            );
        });
        assert_eq!(
            refused(deviation),
            AcceptanceDefect::SealedControls,
            "{path:?}"
        );
    }
}

#[test]
fn additive_control_members_remain_accepted() {
    let paths: &[&[&str]] = &[
        &["controls"],
        &["controls", "organization_floor"],
        &["controls", "debt_snapshot"],
        &["controls", "waiver_bundle"],
        &["controls", "execution_constraint"],
        &["controls", "trusted_time_source"],
        &["controls", "sandbox"],
    ];
    for &path in paths {
        let (wire, expectations) = golden(Deviation::post(move |payload| {
            let value = path.iter().fold(payload, |value, key| entry(value, key));
            set(value, "future", Value::Bool(true));
        }));
        assert_eq!(accept(&wire, &expectations), Ok(0), "{path:?}");
    }
}

#[test]
fn sealed_reports_use_the_closed_wire_envelope() {
    let (wire, expectations) = golden(Deviation::default());
    let mut envelope = parse(&wire).unwrap();
    set(&mut envelope, "future", Value::Bool(true));
    let mut wire = serde_json_canonicalizer::to_vec(&envelope).unwrap();
    wire.push(b'\n');
    assert_eq!(accept(&wire, &expectations), Err(AcceptanceDefect::Shape));
}

#[test]
fn embedded_closed_controls_do_not_accept_unknown_members() {
    for (control, body) in [
        ("execution_constraint", "descriptor"),
        ("trusted_time_source", "statement"),
    ] {
        let deviation = Deviation::post(move |payload| {
            let body = entry(entry(entry(payload, "controls"), control), body);
            set(body, "future", Value::Null);
        });
        assert_eq!(
            refused(deviation),
            AcceptanceDefect::SealedControls,
            "{control}"
        );
    }
}

#[test]
fn control_extensions_keep_the_strict_parser_depth_boundary() {
    for (depth, result) in [(128, Ok(0)), (512, Err(AcceptanceDefect::Shape))] {
        let (wire, expectations) = golden(Deviation::post(move |payload| {
            let extension = (0..depth).fold(Value::Null, |value, _| Value::array(vec![value]));
            set(entry(payload, "controls"), "future", extension);
        }));
        assert_eq!(accept(&wire, &expectations), result, "{depth}");
    }
}

#[test]
fn missing_nullable_control_members_are_not_null() {
    for (name, key) in [("debt_snapshot", "digest"), ("sandbox", "verification")] {
        let deviation = Deviation::post(move |payload| {
            let Value::Object(members) = entry(entry(payload, "controls"), name) else {
                panic!("a control object");
            };
            *members = std::mem::take(members)
                .into_vec()
                .into_iter()
                .filter(|(name, _)| name != key)
                .collect();
        });
        assert_eq!(
            refused(deviation),
            AcceptanceDefect::SealedControls,
            "{name}.{key}"
        );
    }
}

#[test]
fn semantic_evidence_binds_each_producer_fact() {
    use amiss_wire::report::model::{SemanticEvidenceProducer, SemanticEvidenceProvenance};

    let expected = SemanticEvidenceProvenance {
        payload_digest: FLOOR_DIGEST.parse().unwrap(),
        producer: SemanticEvidenceProducer {
            identity: "producer".parse().unwrap(),
            input_digest: FLOOR_DIGEST.parse().unwrap(),
            kind: "rustdoc".parse().unwrap(),
            version: "1".to_owned(),
        },
    };
    let mut cases: Vec<(&str, Option<Patch>)> = vec![
        ("unchanged", None),
        (
            "payload digest",
            Some(Box::new(|row| {
                set(row, "payload_digest", string(FOREIGN_DIGEST));
            })),
        ),
        (
            "row shape",
            Some(Box::new(|row| {
                let Value::Object(members) = std::mem::replace(row, Value::Null) else {
                    panic!("a row")
                };
                *row = Value::array(
                    members
                        .into_vec()
                        .into_iter()
                        .map(|(_, value)| value)
                        .collect(),
                );
            })),
        ),
        (
            "producer shape",
            Some(Box::new(|row| {
                let producer = entry(row, "producer");
                let Value::Object(members) = std::mem::replace(producer, Value::Null) else {
                    panic!("a producer")
                };
                *producer = Value::array(
                    members
                        .into_vec()
                        .into_iter()
                        .map(|(_, value)| value)
                        .collect(),
                );
            })),
        ),
        (
            "additive",
            Some(Box::new(|row| {
                set(row, "future", Value::Bool(true));
                set(entry(row, "producer"), "future", Value::Bool(true));
            })),
        ),
    ];
    for (field, value) in [
        ("input_digest", FOREIGN_DIGEST),
        ("identity", "other"),
        ("kind", "other"),
        ("version", "2"),
    ] {
        cases.push((
            field,
            Some(Box::new(move |row| {
                set(entry(row, "producer"), field, string(value));
            })),
        ));
    }
    for (name, patch) in cases {
        let mut row = parse(&serde_json::to_vec(&expected).unwrap()).unwrap();
        if let Some(patch) = patch {
            patch(&mut row);
        }
        let (wire, mut expectations) = golden(Deviation::post(move |payload| {
            set(
                entry(payload, "controls"),
                "semantic_evidence",
                Value::array(vec![row]),
            );
        }));
        expectations.sealed.as_mut().unwrap().semantic_evidence = vec![expected.clone()];
        let result = if name == "unchanged" || name == "additive" {
            Ok(0)
        } else {
            Err(AcceptanceDefect::SealedControls)
        };
        assert_eq!(accept(&wire, &expectations), result, "{name}");
        let sealed = expectations.sealed.as_mut().unwrap();
        sealed.semantic_evidence.clear();
        assert_eq!(
            accept(&wire, &expectations),
            Err(AcceptanceDefect::SealedControls),
            "unexpected row: {name}"
        );
    }
}
