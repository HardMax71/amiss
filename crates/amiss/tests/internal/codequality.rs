#![cfg(test)]

use amiss_wire::json::Value;

use super::issues;

fn row(members: Vec<(&str, Value)>) -> Value {
    Value::Object(
        members
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.clone()))
            .collect(),
    )
}

/// A control finding without a control path is the wire's global side: path
/// null, span null. GitLab still requires both fields, so the issue answers
/// with the documented placeholder and line one.
#[test]
fn a_global_finding_yields_a_valid_placeholder_location() {
    let finding = row(vec![
        (
            "kind",
            Value::String("organization-floor-unavailable".to_owned()),
        ),
        (
            "description",
            Value::String("a control sentence".to_owned()),
        ),
        ("finding_key", Value::String("sha256:ab".to_owned())),
        ("effective_disposition", Value::String("fail".to_owned())),
        (
            "location",
            row(vec![
                ("side", Value::String("global".to_owned())),
                ("path", Value::Null),
                ("span", Value::Null),
            ]),
        ),
    ]);
    let envelope = row(vec![(
        "payload",
        row(vec![("findings", Value::Array(vec![finding]))]),
    )]);

    let Value::Array(issues) = issues(&envelope) else {
        panic!("the artifact is an array");
    };
    let [issue] = issues.as_slice() else {
        panic!("one finding is one issue");
    };
    let Value::Object(members) = issue else {
        panic!("an issue is an object");
    };
    let location = members
        .iter()
        .find(|(key, _)| key == "location")
        .map(|(_, value)| value.clone());
    let Some(Value::Object(location)) = location else {
        panic!("the issue carries a location");
    };
    assert!(
        location
            .iter()
            .any(|(key, value)| key == "path" && *value == Value::String("(global)".to_owned())),
        "a null wire path answers with the placeholder, got {location:?}",
    );
    assert!(
        location.iter().any(|(key, value)| {
            key == "lines"
                && matches!(
                    value,
                    Value::Object(lines)
                        if lines.iter().any(|(name, line)| name == "begin"
                            && *line == Value::Integer(1))
                )
        }),
        "a null span reads as line one, got {location:?}",
    );
}
