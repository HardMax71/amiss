#![cfg(test)]

use std::collections::HashMap;

use rustdoc_types::{
    Abi, Crate, Function, FunctionHeader, FunctionSignature, Generics, Id, Item, ItemEnum,
    ItemKind, ItemSummary, Module, Path, Struct, StructKind, Target, Trait, Type, Visibility,
};

use super::{Error, FunctionRow, function_declarations, record};

fn rustdoc(root_id: u32, function_id: u32, unrelated_id: Option<u32>) -> Vec<u8> {
    let root = Id(root_id);
    let function_item = Id(function_id);
    let mut index = HashMap::from([
        (
            root,
            item(
                root,
                Some("example"),
                Visibility::Public,
                ItemEnum::Module(Module {
                    is_crate: true,
                    items: vec![function_item],
                    is_stripped: false,
                }),
            ),
        ),
        (
            function_item,
            item(
                function_item,
                Some("run"),
                Visibility::Public,
                ItemEnum::Function(function(
                    vec![("value".to_owned(), Type::Primitive("u64".to_owned()))],
                    Some(Type::Primitive("bool".to_owned())),
                    true,
                )),
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
            function_item,
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
            item(
                unrelated,
                Some("internal"),
                Visibility::Public,
                ItemEnum::Function(function(Vec::new(), None, true)),
            ),
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
    encode_rustdoc(root, index, paths)
}

fn associated_rustdoc(duplicate_name: bool) -> Vec<u8> {
    let root = Id(0);
    let owner = Id(1);
    let implementation_id = Id(2);
    let inherent = Id(3);
    let hidden = Id(4);
    let trait_ = Id(5);
    let trait_method = Id(6);
    let sized = Id(1_000_000);
    let duplicate_implementation = Id(7);
    let duplicate_method = Id(8);
    let implementations = if duplicate_name {
        vec![implementation_id, duplicate_implementation]
    } else {
        vec![implementation_id]
    };
    let mut index = HashMap::from([
        (
            root,
            item(
                root,
                Some("example"),
                Visibility::Public,
                ItemEnum::Module(Module {
                    is_crate: true,
                    items: vec![owner, trait_],
                    is_stripped: false,
                }),
            ),
        ),
        (
            owner,
            item(
                owner,
                Some("Widget"),
                Visibility::Public,
                ItemEnum::Struct(structure(implementations)),
            ),
        ),
        (
            implementation_id,
            item(
                implementation_id,
                None,
                Visibility::Default,
                ItemEnum::Impl(implementation(owner, vec![inherent, hidden])),
            ),
        ),
        (
            inherent,
            item(
                inherent,
                Some("new"),
                Visibility::Public,
                ItemEnum::Function(function(
                    vec![("value".to_owned(), Type::Primitive("u64".to_owned()))],
                    Some(Type::Generic("Self".to_owned())),
                    true,
                )),
            ),
        ),
        (
            hidden,
            item(
                hidden,
                Some("hidden"),
                Visibility::Default,
                ItemEnum::Function(function(Vec::new(), None, true)),
            ),
        ),
    ]);
    index.extend(trait_declarations(trait_, trait_method));
    if duplicate_name {
        index.insert(
            duplicate_implementation,
            item(
                duplicate_implementation,
                None,
                Visibility::Default,
                ItemEnum::Impl(implementation(owner, vec![duplicate_method])),
            ),
        );
        index.insert(
            duplicate_method,
            item(
                duplicate_method,
                Some("new"),
                Visibility::Public,
                ItemEnum::Function(function(
                    vec![("value".to_owned(), Type::Primitive("u16".to_owned()))],
                    Some(Type::Generic("Self".to_owned())),
                    true,
                )),
            ),
        );
    }
    encode_rustdoc(root, index, associated_paths(root, owner, trait_, sized))
}

fn structure(implementations: Vec<Id>) -> Struct {
    Struct {
        kind: StructKind::Unit,
        generics: Generics {
            params: Vec::new(),
            where_predicates: Vec::new(),
        },
        impls: implementations,
    }
}

fn implementation(owner: Id, items: Vec<Id>) -> rustdoc_types::Impl {
    rustdoc_types::Impl {
        is_unsafe: false,
        generics: Generics {
            params: Vec::new(),
            where_predicates: Vec::new(),
        },
        provided_trait_methods: Vec::new(),
        trait_: None,
        for_: Type::ResolvedPath(Path {
            path: "Widget".to_owned(),
            id: owner,
            args: None,
        }),
        items,
        is_negative: false,
        is_synthetic: false,
        blanket_impl: None,
    }
}

fn trait_declarations(trait_id: Id, method: Id) -> [(Id, Item); 2] {
    [
        (
            trait_id,
            item(
                trait_id,
                Some("Service"),
                Visibility::Public,
                ItemEnum::Trait(Trait {
                    is_auto: false,
                    is_unsafe: false,
                    is_dyn_compatible: true,
                    items: vec![method],
                    generics: Generics {
                        params: Vec::new(),
                        where_predicates: Vec::new(),
                    },
                    bounds: Vec::new(),
                    implementations: Vec::new(),
                }),
            ),
        ),
        (
            method,
            item(
                method,
                Some("run"),
                Visibility::Default,
                ItemEnum::Function(function(
                    vec![(
                        "self".to_owned(),
                        Type::BorrowedRef {
                            lifetime: None,
                            is_mutable: false,
                            type_: Box::new(Type::Generic("Self".to_owned())),
                        },
                    )],
                    Some(Type::Primitive("bool".to_owned())),
                    false,
                )),
            ),
        ),
    ]
}

fn associated_paths(root: Id, owner: Id, trait_: Id, sized: Id) -> HashMap<Id, ItemSummary> {
    HashMap::from([
        (
            root,
            ItemSummary {
                crate_id: 0,
                path: vec!["example".to_owned()],
                kind: ItemKind::Module,
            },
        ),
        (
            owner,
            ItemSummary {
                crate_id: 0,
                path: vec!["example".to_owned(), "Widget".to_owned()],
                kind: ItemKind::Struct,
            },
        ),
        (
            trait_,
            ItemSummary {
                crate_id: 0,
                path: vec!["example".to_owned(), "Service".to_owned()],
                kind: ItemKind::Trait,
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
    ])
}

fn encode_rustdoc(root: Id, index: HashMap<Id, Item>, paths: HashMap<Id, ItemSummary>) -> Vec<u8> {
    serde_json::to_vec(&Crate {
        root,
        crate_version: None,
        includes_private: true,
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

fn item(id: Id, name: Option<&str>, visibility: Visibility, inner: ItemEnum) -> Item {
    Item {
        id,
        crate_id: 0,
        name: name.map(str::to_owned),
        span: None,
        visibility,
        docs: None,
        links: HashMap::new(),
        attrs: Vec::new(),
        deprecation: None,
        stability: None,
        const_stability: None,
        inner,
    }
}

fn function(inputs: Vec<(String, Type)>, output: Option<Type>, has_body: bool) -> Function {
    Function {
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
        has_body,
        default_unstable: None,
    }
}

#[test]
fn numeric_ids_and_unrelated_items_do_not_change_existing_records() {
    let first = function_declarations(
        &rustdoc(0, 1, None),
        rustdoc_types::FORMAT_VERSION,
        "example",
        "x86_64-unknown-linux-gnu",
    )
    .unwrap();
    let shifted = function_declarations(
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
fn associated_functions_use_public_owner_paths_and_distinct_namespaces() {
    let normalized = function_declarations(
        &associated_rustdoc(false),
        rustdoc_types::FORMAT_VERSION,
        "example",
        "x86_64-unknown-linux-gnu",
    )
    .unwrap();

    assert!(normalized.complete);
    assert_eq!(
        normalized
            .records
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec![
            "inherent-fn/example::Widget::new",
            "trait-fn/example::Service::run",
        ]
    );
    assert_eq!(
        normalized
            .records
            .get("inherent-fn/example::Widget::new")
            .map(String::as_str),
        Some("pub fn example::Widget::new(value: u64) -> Self")
    );
    assert_eq!(
        normalized
            .records
            .get("trait-fn/example::Service::run")
            .map(String::as_str),
        Some("pub fn example::Service::run(self: &Self) -> bool")
    );
}

#[test]
fn duplicate_owner_and_method_paths_refuse_completion() {
    assert!(matches!(
        function_declarations(
            &associated_rustdoc(true),
            rustdoc_types::FORMAT_VERSION,
            "example",
            "x86_64-unknown-linux-gnu",
        ),
        Err(Error::Ambiguous)
    ));
}

#[test]
fn context_must_match_the_rustdoc_format_and_target() {
    let bytes = rustdoc(0, 1, None);
    assert!(matches!(
        function_declarations(
            &bytes,
            rustdoc_types::FORMAT_VERSION.saturating_sub(1),
            "example",
            "x86_64-unknown-linux-gnu"
        ),
        Err(Error::Format)
    ));
    assert!(matches!(
        function_declarations(
            &bytes,
            rustdoc_types::FORMAT_VERSION,
            "different",
            "x86_64-unknown-linux-gnu"
        ),
        Err(Error::Target)
    ));
    assert!(matches!(
        function_declarations(
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
    let (key, value) = record(
        &FunctionRow {
            name: "original".to_owned(),
            path: vec!["example".to_owned(), "renamed".to_owned()],
            signature: "unsafe fn original(value: u64) -> bool".to_owned(),
        },
        "fn",
    )
    .unwrap();

    assert_eq!(key, "fn/example::renamed");
    assert_eq!(value, "pub unsafe fn example::renamed(value: u64) -> bool");
}

#[test]
fn adapter_whitespace_and_rustdoc_names_have_canonical_records() {
    let (key, value) = record(
        &FunctionRow {
            name: "two_pred".to_owned(),
            path: vec!["example".to_owned(), "two_pred".to_owned()],
            signature: "fn two_pred<T, U>(t: T, u: U) -> T where T: Clone,\nU: Default".to_owned(),
        },
        "fn",
    )
    .unwrap();
    assert_eq!(key, "fn/example::two_pred");
    assert_eq!(
        value,
        "pub fn example::two_pred<T, U>(t: T, u: U) -> T where T: Clone, U: Default"
    );

    let (key, value) = record(
        &FunctionRow {
            name: "loop".to_owned(),
            path: vec![
                "example".to_owned(),
                "visible".to_owned(),
                "loop".to_owned(),
            ],
            signature: "fn loop(value: u64) -> u64".to_owned(),
        },
        "fn",
    )
    .unwrap();
    assert_eq!(key, "fn/example::visible::loop");
    assert_eq!(value, "pub fn example::visible::loop(value: u64) -> u64");
}
