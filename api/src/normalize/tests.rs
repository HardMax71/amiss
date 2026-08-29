#![cfg(test)]

use std::collections::HashMap;

use rustdoc_types::{
    Abi, Crate, Function, FunctionHeader, FunctionSignature, Generics, Id, Item, ItemEnum,
    ItemKind, ItemSummary, Module, Target, Type, Visibility,
};

use super::{Error, FunctionRow, free_functions, record};

fn rustdoc(root_id: u32, function_id: u32, unrelated_id: Option<u32>) -> Vec<u8> {
    let root = Id(root_id);
    let function = Id(function_id);
    let mut index = HashMap::from([
        (root, root_module(root, function)),
        (
            function,
            function_item(
                function,
                "run",
                vec![("value".to_owned(), Type::Primitive("u64".to_owned()))],
                Some(Type::Primitive("bool".to_owned())),
            ),
        ),
    ]);
    let sized = Id(1_000_000);
    let mut paths = HashMap::from([
        (
            root,
            ItemSummary {
                crate_id: 0,
                path: vec!["example".to_owned()],
                kind: ItemKind::Module,
            },
        ),
        (
            function,
            ItemSummary {
                crate_id: 0,
                path: vec!["example".to_owned(), "run".to_owned()],
                kind: ItemKind::Function,
            },
        ),
        (
            sized,
            ItemSummary {
                crate_id: 1,
                path: vec!["core".to_owned(), "marker".to_owned(), "Sized".to_owned()],
                kind: ItemKind::Trait,
            },
        ),
    ]);
    if let Some(id) = unrelated_id {
        let unrelated = Id(id);
        if let Some(Item {
            inner: ItemEnum::Module(module),
            ..
        }) = index.get_mut(&root)
        {
            module.items.push(unrelated);
        }
        index.insert(
            unrelated,
            function_item(unrelated, "internal", Vec::new(), None),
        );
        paths.insert(
            unrelated,
            ItemSummary {
                crate_id: 0,
                path: vec!["example".to_owned(), "internal".to_owned()],
                kind: ItemKind::Function,
            },
        );
    }
    serde_json::to_vec(&Crate {
        root,
        crate_version: None,
        includes_private: false,
        index,
        paths,
        external_crates: HashMap::from([(
            1,
            rustdoc_types::ExternalCrate {
                name: "core".to_owned(),
                html_root_url: None,
                path: std::path::PathBuf::from("libcore.rmeta"),
            },
        )]),
        target: Target {
            triple: "x86_64-unknown-linux-gnu".to_owned(),
            target_features: Vec::new(),
        },
        format_version: rustdoc_types::FORMAT_VERSION,
    })
    .unwrap()
}

fn root_module(root: Id, function: Id) -> Item {
    Item {
        id: root,
        crate_id: 0,
        name: Some("example".to_owned()),
        span: None,
        visibility: Visibility::Public,
        docs: None,
        links: HashMap::new(),
        attrs: Vec::new(),
        deprecation: None,
        stability: None,
        const_stability: None,
        inner: ItemEnum::Module(Module {
            is_crate: true,
            items: vec![function],
            is_stripped: false,
        }),
    }
}

fn function_item(id: Id, name: &str, inputs: Vec<(String, Type)>, output: Option<Type>) -> Item {
    Item {
        id,
        crate_id: 0,
        name: Some(name.to_owned()),
        span: None,
        visibility: Visibility::Public,
        docs: None,
        links: HashMap::new(),
        attrs: Vec::new(),
        deprecation: None,
        stability: None,
        const_stability: None,
        inner: ItemEnum::Function(Function {
            sig: FunctionSignature {
                inputs,
                output,
                is_c_variadic: false,
            },
            generics: Generics {
                params: Vec::new(),
                where_predicates: Vec::new(),
            },
            header: FunctionHeader {
                is_const: false,
                is_unsafe: false,
                is_async: false,
                abi: Abi::Rust,
            },
            has_body: true,
            default_unstable: None,
        }),
    }
}

#[test]
fn numeric_ids_and_unrelated_items_do_not_change_existing_records() {
    let first = free_functions(
        &rustdoc(0, 1, None),
        rustdoc_types::FORMAT_VERSION,
        "example",
        "x86_64-unknown-linux-gnu",
    )
    .unwrap();
    let shifted = free_functions(
        &rustdoc(10, 42, Some(2)),
        rustdoc_types::FORMAT_VERSION,
        "example",
        "x86_64-unknown-linux-gnu",
    )
    .unwrap();

    assert!(first.complete);
    assert_eq!(
        first.records.get("fn/example::run").map(String::as_str),
        Some("pub fn example::run(value: u64) -> bool")
    );
    assert_eq!(
        first.records.get("fn/example::run"),
        shifted.records.get("fn/example::run")
    );
    assert!(shifted.records.contains_key("fn/example::internal"));
}

#[test]
fn context_must_match_the_rustdoc_format_and_target() {
    let bytes = rustdoc(0, 1, None);
    assert!(matches!(
        free_functions(
            &bytes,
            rustdoc_types::FORMAT_VERSION.saturating_sub(1),
            "example",
            "x86_64-unknown-linux-gnu"
        ),
        Err(Error::Format)
    ));
    assert!(matches!(
        free_functions(
            &bytes,
            rustdoc_types::FORMAT_VERSION,
            "different",
            "x86_64-unknown-linux-gnu"
        ),
        Err(Error::Target)
    ));
    assert!(matches!(
        free_functions(
            &bytes,
            rustdoc_types::FORMAT_VERSION,
            "example",
            "aarch64-unknown-linux-gnu"
        ),
        Err(Error::Target)
    ));
}

#[test]
fn public_aliases_own_their_visible_path() {
    let (key, value) = record(&FunctionRow {
        name: "original".to_owned(),
        path: vec!["example".to_owned(), "renamed".to_owned()],
        signature: "unsafe fn original(value: u64) -> bool".to_owned(),
    })
    .unwrap();

    assert_eq!(key, "fn/example::renamed");
    assert_eq!(value, "pub unsafe fn example::renamed(value: u64) -> bool");
}

#[test]
fn adapter_whitespace_and_rustdoc_names_have_canonical_records() {
    let (key, value) = record(&FunctionRow {
        name: "two_pred".to_owned(),
        path: vec!["example".to_owned(), "two_pred".to_owned()],
        signature: "fn two_pred<T, U>(t: T, u: U) -> T where T: Clone,\nU: Default".to_owned(),
    })
    .unwrap();
    assert_eq!(key, "fn/example::two_pred");
    assert_eq!(
        value,
        "pub fn example::two_pred<T, U>(t: T, u: U) -> T where T: Clone, U: Default"
    );

    let (key, value) = record(&FunctionRow {
        name: "loop".to_owned(),
        path: vec![
            "example".to_owned(),
            "visible".to_owned(),
            "loop".to_owned(),
        ],
        signature: "fn loop(value: u64) -> u64".to_owned(),
    })
    .unwrap();
    assert_eq!(key, "fn/example::visible::loop");
    assert_eq!(value, "pub fn example::visible::loop(value: u64) -> u64");
}
