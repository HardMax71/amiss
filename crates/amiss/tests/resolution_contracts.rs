#![expect(
    clippy::expect_used,
    reason = "integration assertions over repository-owned JSON Schemas"
)]

use std::any::type_name;
use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use amiss_wire::controls::{EligibleFindingKind, PromotableFindingKind};
use amiss_wire::resolution::{
    BlobContentTag, BlobMode, ExternalReference, InvalidReference, MissingTag, ResolutionTag,
    TargetTag, UnsupportedSemanticsTag, UnsupportedTargetTag, VersionScopeTag,
};
use serde_json::{Value, json};
use strum::IntoEnumIterator;

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn schema(name: &str) -> Value {
    let path = repository_root().join("spec").join(name);
    serde_json::from_slice(&fs::read(&path).expect("schema is readable"))
        .expect("schema contains valid JSON")
}

fn example(name: &str) -> Value {
    let path = repository_root().join("spec/examples").join(name);
    serde_json::from_slice(&fs::read(&path).expect("example is readable"))
        .expect("example contains valid JSON")
}

fn definition_contract(schema: &Value, name: &str) -> Value {
    json!({
        "$schema": schema
            .get("$schema")
            .expect("schema declares its dialect")
            .clone(),
        "$ref": format!("#/$defs/{name}"),
        "$defs": schema
            .get("$defs")
            .expect("schema contains definitions")
            .clone(),
    })
}

fn replace(instance: &mut Value, pointer: &str, value: Value) {
    *instance
        .pointer_mut(pointer)
        .expect("fixture contains the requested pointer") = value;
}

fn select_type_mismatch_fact_discriminators(fact: &mut Value, selected: u8) {
    if selected & 0b001 != 0 {
        replace(
            fact,
            "/finding_kind",
            json!("explicit-target-type-mismatch"),
        );
    }
    if selected & 0b010 != 0 {
        replace(
            fact,
            "/key_input/finding_kind",
            json!("explicit-target-type-mismatch"),
        );
    }
    if selected & 0b100 != 0 {
        let path = fact
            .pointer("/evidence/resolution/path")
            .expect("missing fixture resolution carries its path")
            .clone();
        replace(
            fact,
            "/evidence/resolution",
            json!({
                "kind": "type-mismatch",
                "target": {
                    "kind": "tree",
                    "path": path,
                },
            }),
        );
    }
}

fn definition<'a>(schema: &'a Value, name: &str) -> &'a Value {
    schema
        .get("$defs")
        .and_then(|definitions| definitions.get(name))
        .expect("requested schema definition exists")
}

fn local_reference<'a>(schema: &'a Value, reference: &str) -> &'a Value {
    let pointer = reference
        .strip_prefix('#')
        .expect("schema reference is local");
    schema
        .pointer(pointer)
        .expect("local schema reference resolves")
}

fn collect_literal_atoms(
    schema: &Value,
    node: &Value,
    visited_references: &mut BTreeSet<String>,
    atoms: &mut BTreeSet<String>,
) {
    if let Some(atom) = node.get("const") {
        atoms.insert(atom.as_str().expect("contract atom is a string").to_owned());
    }
    if let Some(values) = node.get("enum") {
        for atom in values.as_array().expect("contract enum is an array") {
            atoms.insert(
                atom.as_str()
                    .expect("contract enum atom is a string")
                    .to_owned(),
            );
        }
    }
    if let Some(reference) = node.get("$ref").and_then(Value::as_str)
        && visited_references.insert(reference.to_owned())
    {
        collect_literal_atoms(
            schema,
            local_reference(schema, reference),
            visited_references,
            atoms,
        );
    }
    if let Some(branches) = node.get("oneOf") {
        for branch in branches.as_array().expect("oneOf is an array") {
            collect_literal_atoms(schema, branch, visited_references, atoms);
        }
    }
}

fn collect_property_atoms(
    schema: &Value,
    node: &Value,
    property: &str,
    visited_variants: &mut BTreeSet<String>,
    visited_literals: &mut BTreeSet<String>,
    atoms: &mut BTreeSet<String>,
) {
    if let Some(property_schema) = node
        .get("properties")
        .and_then(|properties| properties.get(property))
    {
        collect_literal_atoms(schema, property_schema, visited_literals, atoms);
    }
    if let Some(reference) = node.get("$ref").and_then(Value::as_str)
        && visited_variants.insert(reference.to_owned())
    {
        collect_property_atoms(
            schema,
            local_reference(schema, reference),
            property,
            visited_variants,
            visited_literals,
            atoms,
        );
    }
    if let Some(branches) = node.get("oneOf") {
        for branch in branches.as_array().expect("oneOf is an array") {
            collect_property_atoms(
                schema,
                branch,
                property,
                visited_variants,
                visited_literals,
                atoms,
            );
        }
    }
}

fn schema_atoms(schema: &Value, definition_name: &str, property: &str) -> BTreeSet<String> {
    let mut atoms = BTreeSet::new();
    collect_property_atoms(
        schema,
        definition(schema, definition_name),
        property,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut atoms,
    );
    assert!(
        !atoms.is_empty(),
        "{definition_name}.{property} exposes contract atoms"
    );
    atoms
}

fn rust_atoms<T>() -> BTreeSet<String>
where
    T: IntoEnumIterator + AsRef<str>,
{
    let mut atoms = BTreeSet::new();
    for variant in T::iter() {
        assert!(
            atoms.insert(variant.as_ref().to_owned()),
            "{} has unique wire atoms",
            type_name::<T>()
        );
    }
    atoms
}

fn assert_schema_atoms<T>(schema: &Value, definition_name: &str, property: &str)
where
    T: IntoEnumIterator + AsRef<str>,
{
    assert_eq!(
        schema_atoms(schema, definition_name, property),
        rust_atoms::<T>(),
        "schema {definition_name}.{property} matches {}",
        type_name::<T>()
    );
}

fn definition_atoms(schema: &Value, definition_name: &str) -> BTreeSet<String> {
    let mut atoms = BTreeSet::new();
    collect_literal_atoms(
        schema,
        definition(schema, definition_name),
        &mut BTreeSet::new(),
        &mut atoms,
    );
    assert!(
        !atoms.is_empty(),
        "{definition_name} exposes contract atoms"
    );
    atoms
}

fn assert_definition_atoms<T>(schema: &Value, definition_name: &str)
where
    T: IntoEnumIterator + AsRef<str>,
{
    assert_eq!(
        definition_atoms(schema, definition_name),
        rust_atoms::<T>(),
        "schema {definition_name} matches {}",
        type_name::<T>()
    );
}

fn collect_resolution_definition_references(node: &Value, names: &mut BTreeSet<String>) {
    match node {
        Value::Object(members) => {
            if let Some(reference) = members.get("$ref").and_then(Value::as_str)
                && let Some(name) = reference.strip_prefix("#/$defs/")
                && name.contains("Resolution")
            {
                names.insert(name.to_owned());
            }
            for value in members.values() {
                collect_resolution_definition_references(value, names);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_resolution_definition_references(value, names);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn structural_resolution_definitions(schema: &Value) -> BTreeSet<String> {
    let root = "StructuralResolution";
    let mut pending = VecDeque::from([root.to_owned()]);
    let mut visited = BTreeSet::new();
    let mut dependencies = BTreeSet::new();

    while let Some(name) = pending.pop_front() {
        if !visited.insert(name.clone()) {
            continue;
        }
        let mut referenced = BTreeSet::new();
        collect_resolution_definition_references(definition(schema, &name), &mut referenced);
        for dependency in referenced {
            if dependencies.insert(dependency.clone()) {
                pending.push_back(dependency);
            }
        }
    }
    dependencies.remove(root);
    assert!(
        !dependencies.is_empty(),
        "structural resolution has referenced definitions"
    );
    dependencies
}

#[test]
fn rust_resolution_atoms_match_the_report_schema() {
    let report = schema("scanner-report.schema.json");

    assert_schema_atoms::<ResolutionTag>(&report, "Resolution", "kind");
    assert_schema_atoms::<MissingTag>(&report, "MissingResolution", "reason");
    assert_schema_atoms::<UnsupportedTargetTag>(&report, "UnsupportedTargetResolution", "reason");
    assert_schema_atoms::<UnsupportedSemanticsTag>(
        &report,
        "UnsupportedSemanticsResolution",
        "reason",
    );
    assert_schema_atoms::<TargetTag>(&report, "ResolutionTarget", "kind");
    assert_schema_atoms::<BlobContentTag>(&report, "ResolutionContent", "kind");
    assert_schema_atoms::<VersionScopeTag>(&report, "VersionScope", "kind");
    assert_schema_atoms::<BlobMode>(&report, "BlobResolutionTarget", "mode");
    assert_schema_atoms::<InvalidReference>(&report, "InvalidResolution", "reason");
    assert_schema_atoms::<ExternalReference>(&report, "ExternalResolution", "reason");
}

#[test]
fn control_finding_kind_subsets_match_their_schemas() {
    let floor = schema("organization-floor.schema.json");
    let debt = schema("debt-snapshot.schema.json");
    let waiver = schema("waiver-bundle.schema.json");

    assert_definition_atoms::<PromotableFindingKind>(&floor, "FloorPromotableFindingKind");
    assert_definition_atoms::<EligibleFindingKind>(&floor, "DebtEligibleFindingKind");
    assert_definition_atoms::<EligibleFindingKind>(&debt, "DebtEligibleFindingKind");
    assert_definition_atoms::<EligibleFindingKind>(&waiver, "WaiverEligibleFindingKind");
}

#[test]
fn structural_control_resolution_definitions_match_the_report() {
    let report = schema("scanner-report.schema.json");
    let debt = schema("debt-snapshot.schema.json");
    let waiver = schema("waiver-bundle.schema.json");

    let structural_kinds = BTreeSet::from([
        ResolutionTag::Missing.as_ref().to_owned(),
        ResolutionTag::TypeMismatch.as_ref().to_owned(),
    ]);
    assert_eq!(
        schema_atoms(&debt, "StructuralResolution", "kind"),
        structural_kinds,
        "debt admits exactly the structural resolution families"
    );
    assert_eq!(
        schema_atoms(&waiver, "StructuralResolution", "kind"),
        structural_kinds,
        "waiver admits exactly the structural resolution families"
    );

    assert_eq!(
        definition(&debt, "StructuralResolution"),
        definition(&waiver, "StructuralResolution"),
        "debt and waiver expose the same structural resolution root"
    );
    let debt_definitions = structural_resolution_definitions(&debt);
    let waiver_definitions = structural_resolution_definitions(&waiver);
    assert_eq!(
        debt_definitions, waiver_definitions,
        "debt and waiver structural resolution graphs stay aligned"
    );

    for name in debt_definitions {
        let report_definition = definition(&report, &name);
        assert_eq!(
            definition(&debt, &name),
            report_definition,
            "debt {name} matches the report contract"
        );
        assert_eq!(
            definition(&waiver, &name),
            report_definition,
            "waiver {name} matches the report contract"
        );
    }
}

fn assert_structural_control_discriminators(
    schema_name: &str,
    example_name: &str,
    item_definition: &str,
    fact_field: &str,
) {
    let contract = schema(schema_name);
    let document = example(example_name);
    let item = document
        .pointer("/items/0")
        .expect("control example contains one item")
        .clone();
    let fact = item
        .get(fact_field)
        .expect("control item contains its fact")
        .clone();

    let fact_contract = definition_contract(&contract, "StructuralFindingFactInput");
    let fact_validator =
        jsonschema::validator_for(&fact_contract).expect("fact definition compiles");
    assert!(
        fact_validator.is_valid(&fact),
        "the missing-family example fact is valid"
    );
    for selected in 1_u8..0b111 {
        let mut inconsistent = fact.clone();
        select_type_mismatch_fact_discriminators(&mut inconsistent, selected);
        assert!(
            !fact_validator.is_valid(&inconsistent),
            "{schema_name} admits inconsistent fact discriminators {selected:03b}"
        );
    }
    let mut type_mismatch_fact = fact.clone();
    select_type_mismatch_fact_discriminators(&mut type_mismatch_fact, 0b111);
    assert!(
        fact_validator.is_valid(&type_mismatch_fact),
        "{schema_name} rejects a coherent type-mismatch fact"
    );

    let item_contract = definition_contract(&contract, item_definition);
    let item_validator =
        jsonschema::validator_for(&item_contract).expect("item definition compiles");
    assert!(
        item_validator.is_valid(&item),
        "the missing-family example item is valid"
    );
    for selected in 1_u8..0b111 {
        let mut inconsistent = item.clone();
        let nested_fact = inconsistent
            .get_mut(fact_field)
            .expect("control item contains its fact");
        select_type_mismatch_fact_discriminators(nested_fact, selected);
        assert!(
            !item_validator.is_valid(&inconsistent),
            "{schema_name} item admits inconsistent fact discriminators {selected:03b}"
        );
    }
    let mut type_mismatch_item = item.clone();
    let nested_fact = type_mismatch_item
        .get_mut(fact_field)
        .expect("control item contains its fact");
    select_type_mismatch_fact_discriminators(nested_fact, 0b111);
    assert!(
        item_validator.is_valid(&type_mismatch_item),
        "{schema_name} rejects a coherent type-mismatch item"
    );

    let mut duplicate_kind = item.clone();
    duplicate_kind
        .as_object_mut()
        .expect("control item is an object")
        .insert(
            "finding_kind".to_owned(),
            fact.get("finding_kind")
                .expect("fact carries its kind")
                .clone(),
        );
    assert!(
        !item_validator.is_valid(&duplicate_kind),
        "{schema_name} permits a duplicated outer finding kind"
    );

    let mut duplicate_key = item;
    duplicate_key
        .as_object_mut()
        .expect("control item is an object")
        .insert(
            "key_input".to_owned(),
            fact.get("key_input")
                .expect("fact carries its key input")
                .clone(),
        );
    assert!(
        !item_validator.is_valid(&duplicate_key),
        "{schema_name} permits a duplicated outer key input"
    );
}

#[test]
fn structural_control_schemas_reject_cross_family_discriminator_drift() {
    assert_structural_control_discriminators(
        "debt-snapshot.schema.json",
        "debt-snapshot.json",
        "DebtItem",
        "accepted_fact",
    );
    assert_structural_control_discriminators(
        "waiver-bundle.schema.json",
        "waiver-bundle.json",
        "WaiverItem",
        "authorized_fact",
    );
}
