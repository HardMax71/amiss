use amiss_bootstrap::supervise::{AcceptanceDefect, accept};
use amiss_wire::json::{Value, parse};

use super::{Deviation, entry, golden, refused, set};

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
        &["controls", "sandbox", "descriptor"],
        &["controls", "sandbox", "descriptor", "physical_memory"],
        &["controls", "sandbox", "descriptor", "temporary_storage"],
        &["controls", "sandbox", "descriptor", "watchdog"],
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
        assert_eq!(refused(deviation), AcceptanceDefect::Shape, "{path:?}");
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
            let value = path.iter().fold(payload, |value, key| entry(value, key));
            set(value, "future", Value::Bool(true));
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
        assert_eq!(refused(deviation), AcceptanceDefect::Shape, "{control}");
    }
}

#[test]
fn control_extensions_keep_the_strict_parser_depth_boundary() {
    for depth in [128, 512] {
        let (wire, expectations) = golden(Deviation::post(move |payload| {
            let extension = (0..depth).fold(Value::Null, |value, _| Value::array(vec![value]));
            set(entry(payload, "controls"), "future", extension);
        }));
        assert_eq!(
            accept(&wire, &expectations),
            Err(AcceptanceDefect::Shape),
            "{depth}"
        );
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
        assert_eq!(refused(deviation), AcceptanceDefect::Shape, "{name}.{key}");
    }
}
