use amiss_bootstrap::supervise::{AcceptanceDefect, accept};
use serde_json::Value;

use super::{Deviation, golden, refused};

#[test]
fn sealed_controls_reject_noncanonical_positional_arrays() {
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
        &["controls", "sandbox", "descriptor"],
        &["controls", "sandbox", "descriptor", "physical_memory"],
        &["controls", "sandbox", "descriptor", "temporary_storage"],
        &["controls", "sandbox", "descriptor", "watchdog"],
    ];
    for &path in paths {
        let deviation = Deviation::post(move |payload| {
            let value = path.iter().fold(payload, |value, key| {
                (value).get_mut(key).expect("fixture member exists")
            });
            let Value::Object(members) = std::mem::replace(value, Value::Null) else {
                panic!("an object fixture at {path:?}");
            };
            *value = Value::Array(members.into_iter().map(|(_, value)| value).collect());
        });
        assert!(
            matches!(
                refused(deviation),
                AcceptanceDefect::Noncanonical | AcceptanceDefect::Shape
            ),
            "{path:?}"
        );
    }
}

#[test]
fn unknown_control_members_are_refused_with_correct_payload_digests() {
    let paths: &[&[&str]] = &[
        &["controls"],
        &["controls", "organization_floor"],
        &["controls", "debt_snapshot"],
        &["controls", "waiver_bundle"],
        &["controls", "execution_constraint"],
        &["controls", "trusted_time_source"],
        &["controls", "sandbox"],
        &["controls", "sandbox", "descriptor"],
        &["controls", "sandbox", "descriptor", "physical_memory"],
        &["controls", "sandbox", "descriptor", "temporary_storage"],
        &["controls", "sandbox", "descriptor", "watchdog"],
    ];
    for &path in paths {
        let (wire, expectations) = golden(Deviation::post(move |payload| {
            let value = path.iter().fold(payload, |value, key| {
                (value).get_mut(key).expect("fixture member exists")
            });
            (value)["future"] = Value::Bool(true);
        }));
        assert_eq!(
            accept(&wire, &expectations),
            Err(AcceptanceDefect::Shape),
            "{path:?}"
        );
    }
}

#[test]
fn sealed_reports_use_the_closed_wire_envelope() {
    let (wire, expectations) = golden(Deviation::default());
    let mut envelope = serde_json::from_slice::<Value>(&wire).unwrap();
    (&mut envelope)["future"] = Value::Bool(true);
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
            let body = (((payload)
                .get_mut("controls")
                .expect("fixture member exists"))
            .get_mut(control)
            .expect("fixture member exists"))
            .get_mut(body)
            .expect("fixture member exists");
            (body)["future"] = Value::Null;
        });
        assert_eq!(refused(deviation), AcceptanceDefect::Shape, "{control}");
    }
}

#[test]
fn control_extensions_keep_the_strict_parser_depth_boundary() {
    for depth in [128, 512] {
        let (wire, expectations) = golden(Deviation::post(move |payload| {
            let extension = (0..depth).fold(Value::Null, |value, _| Value::Array(vec![value]));
            ((payload)
                .get_mut("controls")
                .expect("fixture member exists"))["future"] = extension;
        }));
        assert_eq!(
            accept(&wire, &expectations),
            Err(AcceptanceDefect::Shape),
            "{depth}"
        );
    }
}

#[test]
fn missing_nullable_control_members_are_noncanonical() {
    for (name, key) in [("debt_snapshot", "digest"), ("sandbox", "verification")] {
        let deviation = Deviation::post(move |payload| {
            let Value::Object(members) = ((payload)
                .get_mut("controls")
                .expect("fixture member exists"))
            .get_mut(name)
            .expect("fixture member exists") else {
                panic!("a control object");
            };
            *members = std::mem::take(members)
                .into_iter()
                .filter(|(name, _)| name != key)
                .collect();
        });
        assert_eq!(
            refused(deviation),
            AcceptanceDefect::Noncanonical,
            "{name}.{key}"
        );
    }
}
