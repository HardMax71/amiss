#![cfg(test)]

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};
use serde_json::json;

use crate::codec::{borrow_value, decode, object, owned_value};

#[derive(Debug, Deserialize, PartialEq)]
struct Named {
    name: String,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Nested {
    rows: Vec<Named>,
    optional: Option<Box<Named>>,
    dictionary: BTreeMap<String, Named>,
}

#[derive(Debug, Deserialize, PartialEq)]
enum External {
    Unit,
    New(Named),
    Record { name: String },
    Tuple(Named, u8),
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(remote = "Self")]
struct ObjectNamed {
    name: String,
}

impl<'de> Deserialize<'de> for ObjectNamed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(object(deserializer))
    }
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", remote = "Self")]
enum Internal {
    Record { nested: ObjectNamed },
    Empty {},
}

impl<'de> Deserialize<'de> for Internal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize(object(deserializer))
    }
}

#[test]
fn structs_require_objects_in_owned_borrowed_and_byte_inputs() {
    let positional = json!(["row"]);
    assert!(owned_value::<Named>("$", positional.clone()).is_err());
    assert!(borrow_value::<Named>("$", &positional).is_err());
    assert!(decode::<Named>(br#"["row"]"#).is_err());
    assert_eq!(
        decode::<Named>(br#"{"name":"row"}"#).unwrap(),
        Named {
            name: "row".to_owned()
        }
    );
}

#[test]
fn nested_collection_option_and_map_seeds_keep_struct_shape_guards() {
    let valid = json!({
        "rows": [{"name": "row"}],
        "optional": {"name": "row"},
        "dictionary": {"key": {"name": "row"}}
    });
    assert!(borrow_value::<Nested>("$", &valid).is_ok());
    for path in ["/rows/0", "/optional", "/dictionary/key"] {
        let mut positional = valid.clone();
        *positional.pointer_mut(path).unwrap() = json!(["row"]);
        let defect = borrow_value::<Nested>("$", &positional).unwrap_err();
        assert!(defect.path.contains(path.split('/').nth(1).unwrap()));
        assert!(owned_value::<Nested>("$", positional).is_err());
    }
}

#[test]
fn external_enum_variants_keep_string_object_and_tuple_syntax_distinct() {
    for valid in [
        json!("Unit"),
        json!({"New": {"name": "row"}}),
        json!({"Record": {"name": "row"}}),
        json!({"Tuple": [{"name": "row"}, 1]}),
    ] {
        assert!(borrow_value::<External>("$", &valid).is_ok());
    }
    for malformed in [
        json!({"Unit": null}),
        json!({"New": ["row"]}),
        json!({"Record": ["row"]}),
        json!({"Tuple": [["row"], 1]}),
        json!({"Record": {"name": "row"}, "Unit": null}),
    ] {
        assert!(borrow_value::<External>("$", &malformed).is_err());
        assert!(owned_value::<External>("$", malformed).is_err());
    }
}

#[test]
fn explicitly_guarded_tagged_enums_reject_positional_buffered_structs() {
    assert!(decode::<Internal>(br#"{"kind":"Record","nested":{"name":"row"}}"#).is_ok());
    assert!(decode::<Internal>(br#"{"kind":"Record","nested":["row"]}"#).is_err());
    assert!(decode::<Internal>(br#"["Record",{"name":"row"}]"#).is_err());
}

#[test]
fn borrowed_strings_and_upstream_numbers_are_not_copied_or_profiled() {
    #[derive(Deserialize)]
    struct Borrowed<'a> {
        name: &'a str,
        ratio: f64,
        wide: u64,
    }
    let input = json!({"name": "row", "ratio": 0.5, "wide": u64::MAX});
    let decoded: Borrowed<'_> = borrow_value("$", &input).unwrap();
    assert!(std::ptr::eq(decoded.name, input["name"].as_str().unwrap()));
    assert!((decoded.ratio - 0.5).abs() < f64::EPSILON);
    assert_eq!(decoded.wide, u64::MAX);
}
