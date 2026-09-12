use amiss_bootstrap::supervise::{AcceptanceDefect, accept};
use amiss_wire::requests::CANDIDATE_IDENTITY_DOMAIN;
use serde_json::Value;

use super::{Deviation, golden, refused};

#[test]
fn identity_extensions_are_refused_even_with_matching_bindings() {
    for path in [None, Some("base"), Some("candidate")] {
        for changed in [false, true] {
            let deviation = Deviation {
                pre: Some(Box::new(move |payload| {
                    let evaluation = (payload)
                        .get_mut("evaluation")
                        .expect("fixture member exists");
                    let target = match path {
                        Some(key) => (evaluation).get_mut(key).expect("fixture member exists"),
                        None => evaluation,
                    };
                    (target)["future"] =
                        Value::Array(vec![Value::Null, Value::from("\u{1f600}\u{e000}")]);
                })),
                post: changed.then(|| -> super::Patch {
                    Box::new(move |payload: &mut Value| {
                        let evaluation = (payload)
                            .get_mut("evaluation")
                            .expect("fixture member exists");
                        let target = match path {
                            Some(key) => (evaluation).get_mut(key).expect("fixture member exists"),
                            None => evaluation,
                        };
                        (target)["future"] = Value::Bool(true);
                    })
                }),
                ..Deviation::default()
            };
            let (wire, expectations) = golden(deviation);
            assert_eq!(
                accept(&wire, &expectations),
                Err(AcceptanceDefect::Shape),
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
        Value::from(CANDIDATE_IDENTITY_DOMAIN),
    ] {
        assert_eq!(
            refused(Deviation::pre(move |payload| {
                ((payload)
                    .get_mut("evaluation")
                    .expect("fixture member exists"))["schema"] = schema;
            })),
            AcceptanceDefect::Shape
        );
    }
}

#[test]
fn clock_shape_and_binding_defects_remain_distinct() {
    for (value, defect) in [
        (Value::Null, AcceptanceDefect::SealedControls),
        (Value::Bool(true), AcceptanceDefect::Shape),
        (
            Value::from("not-an-instant"),
            AcceptanceDefect::SealedControls,
        ),
    ] {
        assert_eq!(
            refused(Deviation::post(move |payload| {
                ((payload)
                    .get_mut("evaluation")
                    .expect("fixture member exists"))["evaluation_instant"] = value;
            })),
            defect
        );
    }
    assert_eq!(
        refused(Deviation::post(|payload| {
            let Value::Object(members) = (payload)
                .get_mut("evaluation")
                .expect("fixture member exists")
            else {
                panic!("an evaluation object")
            };
            *members = std::mem::take(members)
                .into_iter()
                .filter(|(name, _)| name != "evaluation_instant")
                .collect();
        })),
        AcceptanceDefect::Noncanonical
    );
}

#[test]
fn identity_extensions_cannot_bypass_the_closed_shape_or_outer_depth_limit() {
    for depth in [256, 513] {
        let (wire, expectations) = golden(Deviation::pre(move |payload| {
            let nested = (0..depth).fold(Value::Null, |value, _| Value::Array(vec![value]));
            (((payload)
                .get_mut("evaluation")
                .expect("fixture member exists"))
            .get_mut("candidate")
            .expect("fixture member exists"))["future"] = nested;
        }));
        assert_eq!(
            accept(&wire, &expectations),
            Err(AcceptanceDefect::Shape),
            "{depth}"
        );
    }
}
