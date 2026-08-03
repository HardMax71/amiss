#![cfg(test)]

use amiss_wire::json::Value;

use super::View;

fn object(members: &[(&str, Value)]) -> Value {
    Value::Object(
        members
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect(),
    )
}

/// The one object shape the projection reads is the wire's raw-bytes atom.
/// Anything else is a dash, since guessing would print bytes the wire never
/// said were bytes.
#[test]
fn only_the_bytes_atom_is_read_as_bytes() {
    let hex = Value::String("646f6373".to_owned());
    let row = object(&[
        ("path", object(&[("bytes_hex", hex.clone())])),
        ("target", object(&[("hex", hex)])),
        ("code", Value::String("plain".to_owned())),
        ("count", Value::Integer(3)),
    ]);
    let view = View::of(Some(&row));

    assert_eq!(view.atom_or_dash("path"), "\"docs\"");
    assert_eq!(
        view.atom_or_dash("target"),
        "-",
        "another single-member object is not the bytes atom"
    );
    assert_eq!(view.atom_or_dash("code"), "\"plain\"");
    assert_eq!(view.atom_or_dash("count"), "-");
    assert_eq!(view.atom_or_dash("absent"), "-");
}
