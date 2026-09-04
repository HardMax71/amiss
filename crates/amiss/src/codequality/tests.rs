#![cfg(test)]

use amiss_wire::json::Value;

use super::issues;

fn row(members: Vec<(&str, Value)>) -> Value {
    Value::object(
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
        ("kind", Value::string("organization-floor-unavailable")),
        ("description", Value::string("a control sentence")),
        ("finding_key", Value::string("sha256:ab")),
        ("effective_disposition", Value::string("fail")),
        (
            "location",
            row(vec![
                ("side", Value::string("global")),
                ("path", Value::Null),
                ("span", Value::Null),
            ]),
        ),
    ]);
    let envelope = row(vec![(
        "payload",
        row(vec![("findings", Value::array(vec![finding]))]),
    )]);

    assert_eq!(
        serde_json::to_vec(&issues(&envelope)).unwrap(),
        br#"[{"check_name":"organization-floor-unavailable","description":"a control sentence","fingerprint":"sha256:ab","location":{"lines":{"begin":1},"path":"(global)"},"severity":"major"}]"#,
    );
}

#[test]
fn paths_and_dispositions_keep_their_projection_without_owned_json_rows() {
    for (path, line, disposition, expected_path, expected_line, severity) in [
        (
            Value::string("docs/a\"b\n.md"),
            7,
            "warn",
            "docs/a\"b\n.md",
            7,
            "minor",
        ),
        (
            row(vec![("bytes_hex", Value::string("646f63732fff2e6d64"))]),
            0,
            "record",
            "646f63732fff2e6d64",
            1,
            "info",
        ),
    ] {
        let envelope = row(vec![(
            "payload",
            row(vec![(
                "findings",
                Value::array(vec![row(vec![
                    ("kind", Value::string("explicit-target-missing")),
                    ("description", Value::string("a \"missing\" target\n")),
                    ("finding_key", Value::string("sha256:ab")),
                    ("effective_disposition", Value::string(disposition)),
                    (
                        "location",
                        row(vec![
                            ("path", path),
                            ("span", row(vec![("start_line", Value::Integer(line))])),
                        ]),
                    ),
                ])]),
            )]),
        )]);
        let projected = issues(&envelope);
        let bytes = serde_json::to_vec(&projected).unwrap();
        assert_eq!(bytes, serde_json_canonicalizer::to_vec(&projected).unwrap());
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value[0]["location"]["path"], expected_path);
        assert_eq!(value[0]["location"]["lines"]["begin"], expected_line);
        assert_eq!(value[0]["severity"], severity);
        assert_eq!(value[0]["description"], "a \"missing\" target\n");
    }
}
