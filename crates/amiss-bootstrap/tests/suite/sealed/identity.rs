use amiss_bootstrap::supervise::{AcceptanceDefect, accept};
use amiss_wire::json::Value;
use amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN;

use super::{Deviation, entry, golden, refused, set, string};

#[test]
fn additive_identity_fields_are_retained_and_bound() {
    for path in [None, Some("base"), Some("candidate")] {
        for changed in [false, true] {
            let deviation = Deviation {
                pre: Some(Box::new(move |payload| {
                    let evaluation = entry(payload, "evaluation");
                    let target = match path {
                        Some(key) => entry(evaluation, key),
                        None => evaluation,
                    };
                    set(
                        target,
                        "future",
                        Value::array(vec![Value::Null, string("\u{1f600}\u{e000}")]),
                    );
                })),
                post: changed.then(|| -> super::Patch {
                    Box::new(move |payload: &mut Value| {
                        let evaluation = entry(payload, "evaluation");
                        let target = match path {
                            Some(key) => entry(evaluation, key),
                            None => evaluation,
                        };
                        set(target, "future", Value::Bool(true));
                    })
                }),
                ..Deviation::default()
            };
            let (wire, expectations) = golden(deviation);
            let expected = if changed {
                Err(AcceptanceDefect::SealedIdentity)
            } else {
                Ok(0)
            };
            assert_eq!(
                accept(&wire, &expectations),
                expected,
                "{path:?}, {changed}"
            );
        }
    }
}

#[test]
fn a_reserved_schema_cannot_join_the_identity_preimage() {
    for schema in [
        Value::Null,
        Value::Bool(false),
        string(CANDIDATE_IDENTITY_DOMAIN),
    ] {
        assert_eq!(
            refused(Deviation::pre(move |payload| {
                set(entry(payload, "evaluation"), "schema", schema);
            })),
            AcceptanceDefect::SealedIdentity
        );
    }
}

#[test]
fn malformed_runtime_time_is_a_control_defect_not_an_identity_defect() {
    for value in [Value::Null, Value::Bool(true), string("not-an-instant")] {
        assert_eq!(
            refused(Deviation::post(move |payload| {
                set(entry(payload, "evaluation"), "evaluation_instant", value);
            })),
            AcceptanceDefect::SealedControls
        );
    }
    assert_eq!(
        refused(Deviation::post(|payload| {
            let Value::Object(members) = entry(payload, "evaluation") else {
                panic!("an evaluation object")
            };
            *members = std::mem::take(members)
                .into_vec()
                .into_iter()
                .filter(|(name, _)| name != "evaluation_instant")
                .collect();
        })),
        AcceptanceDefect::SealedControls
    );
}

#[test]
fn deep_identity_extensions_keep_the_outer_depth_limit() {
    for (depth, expected) in [(256, Ok(0)), (513, Err(AcceptanceDefect::Shape))] {
        let (wire, expectations) = golden(Deviation::pre(move |payload| {
            let nested = (0..depth).fold(Value::Null, |value, _| Value::array(vec![value]));
            set(
                entry(entry(payload, "evaluation"), "candidate"),
                "future",
                nested,
            );
        }));
        assert_eq!(accept(&wire, &expectations), expected, "{depth}");
    }
}
