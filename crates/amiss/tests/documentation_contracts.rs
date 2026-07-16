#![expect(
    clippy::expect_used,
    reason = "integration assertions over repository-owned documentation and fixtures"
)]

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use amiss_git::GitLimits;
use amiss_scan::ScanLimits;
use amiss_wire::controls::{ORGANIZATION_POLICY_ENTRIES_LIMIT, ResourceName};
use amiss_wire::report::{
    EVALUATOR_MANAGED_MEMORY_BYTES, FindingKind, MACHINE_JSON_BYTES,
    PRIVATE_TEMPORARY_STORAGE_BYTES,
};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn v3_report_schema() -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(repository_root().join("spec/scanner-report-v3.schema.json"))
            .expect("v3 report schema is readable"),
    )
    .expect("v3 report schema is JSON")
}

fn schema_enum(schema: &serde_json::Value, name: &str) -> Vec<String> {
    schema
        .pointer(&format!("/$defs/{name}/enum"))
        .expect("schema enum definition exists")
        .as_array()
        .expect("schema definition is a string enum")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("schema enum member is a string")
                .to_owned()
        })
        .collect()
}

fn documented_contract(document: &str, name: &str) -> String {
    let start = format!("<!-- amiss-doc-contract:{name}:start -->");
    let end = format!("<!-- amiss-doc-contract:{name}:end -->");
    let (_, after_start) = document
        .split_once(&start)
        .expect("documentation contract start marker exists");
    let (body, after_end) = after_start
        .split_once(&end)
        .expect("documentation contract end marker exists");
    assert!(
        !after_end.contains(&start) && !after_end.contains(&end),
        "documentation contract {name} must occur exactly once"
    );
    body.trim_matches('\n').to_owned()
}

fn profile_table() -> String {
    let mut table = String::from("| Finding kind | Observe | Enforce |\n| --- | --- | --- |");
    for kind in FindingKind::ALL {
        write!(
            table,
            "\n| `{}` | `{}` | `{}` |",
            kind.as_str(),
            kind.built_in_disposition(false).as_str(),
            kind.built_in_disposition(true).as_str(),
        )
        .expect("writing to a String is infallible");
    }
    table
}

fn grouped_decimal(number: u64) -> String {
    let digits = number.to_string();
    let mut grouped = String::with_capacity(digits.len().saturating_add(digits.len() / 3));
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && digits.len().saturating_sub(index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

fn resource_limit(resource: ResourceName) -> u64 {
    let git = GitLimits::CONTRACT;
    let scan = ScanLimits::CONTRACT;
    match resource {
        ResourceName::GitObjectBytes => git.inflated_object_bytes,
        ResourceName::GitCompressedObjectBytes => git.compressed_stream_bytes,
        ResourceName::AggregateGitCompressedObjectBytesPerEvaluation => {
            git.aggregate_compressed_bytes
        }
        ResourceName::GitPackDirectoryEntries => git.pack_directory_entries,
        ResourceName::GitPackFiles => git.pack_files,
        ResourceName::GitPackIndexBytes => git.pack_index_bytes,
        ResourceName::AggregateGitPackIndexBytes => git.aggregate_pack_index_bytes,
        ResourceName::GitDeltaDepth => git.delta_depth,
        ResourceName::GitIndexBytes => git.index_bytes,
        ResourceName::GitTreeEntriesPerSnapshot => git.tree_entries_per_snapshot,
        ResourceName::DocumentsPerSnapshot => scan.documents_per_snapshot,
        ResourceName::ControlInputBytes => scan.control_input_bytes,
        ResourceName::SelectedControlBlobBytes => scan.selected_control_blob_bytes,
        ResourceName::AggregateSelectedControlBytesPerSnapshot => {
            scan.aggregate_selected_control_bytes_per_snapshot
        }
        ResourceName::RepositoryPolicyEntries => scan.repository_policy_entries,
        ResourceName::DebtItems => scan.debt_items,
        ResourceName::WaiverItems => scan.waiver_items,
        ResourceName::RawPathBytes => git.raw_path_bytes,
        ResourceName::DocumentBlobBytes => scan.document_blob_bytes,
        ResourceName::ReferencedTargetBlobBytes => scan.referenced_target_blob_bytes,
        ResourceName::AggregateReferencedTargetBytesPerSnapshot => {
            scan.aggregate_referenced_target_bytes_per_snapshot
        }
        ResourceName::AggregateDocumentBytesPerSnapshot => {
            scan.aggregate_document_bytes_per_snapshot
        }
        ResourceName::RawLinkDestinationBytes => scan.raw_link_destination_bytes,
        ResourceName::ParserNesting => scan.parser_nesting,
        ResourceName::ParserNodesPerDocument => scan.parser_nodes_per_document,
        ResourceName::ParserNodesPerSnapshot => scan.parser_nodes_per_snapshot,
        ResourceName::ReferencesPerDocument => scan.references_per_document,
        ResourceName::ReferencesPerSnapshot => scan.references_per_snapshot,
        ResourceName::OrganizationPolicyEntries => ORGANIZATION_POLICY_ENTRIES_LIMIT,
        ResourceName::CompleteFindings => scan.complete_findings,
        ResourceName::TypedAnalysisErrorsRetained => scan.errors_retained,
        ResourceName::MachineJsonBytes => MACHINE_JSON_BYTES,
        ResourceName::PrivateTemporaryStorageBytes => PRIVATE_TEMPORARY_STORAGE_BYTES,
        ResourceName::EvaluatorManagedMemoryBytes => EVALUATOR_MANAGED_MEMORY_BYTES,
    }
}

fn limits_table() -> String {
    let mut table = String::from("| Report resource | Limit |\n| --- | ---: |");
    for resource in ResourceName::all() {
        write!(
            table,
            "\n| `{}` | {} |",
            resource.as_str(),
            grouped_decimal(resource_limit(resource)),
        )
        .expect("writing to a String is infallible");
    }
    table
}

#[test]
fn documented_profiles_are_generated_from_the_policy_contract() {
    let path = repository_root().join("docs/src/profiles.md");
    let document = fs::read_to_string(&path).expect("profiles documentation is readable");
    assert_eq!(
        documented_contract(&document, "profiles"),
        profile_table(),
        "{} drifted from FindingKind::built_in_disposition",
        path.display(),
    );
}

#[test]
fn documented_limits_are_generated_from_runtime_constants() {
    let path = repository_root().join("docs/src/limits.md");
    let document = fs::read_to_string(&path).expect("limits documentation is readable");
    assert_eq!(
        documented_contract(&document, "limits"),
        limits_table(),
        "{} drifted from the runtime resource contracts",
        path.display(),
    );
}

#[test]
fn documented_enum_sources_match_the_report_schema() {
    let schema = v3_report_schema();
    let findings: Vec<String> = FindingKind::ALL
        .iter()
        .map(|kind| kind.as_str().to_owned())
        .collect();
    let resources: Vec<String> = ResourceName::all()
        .map(|resource| resource.as_str().to_owned())
        .collect();

    assert_eq!(
        findings,
        schema_enum(&schema, "FindingKind"),
        "FindingKind::ALL drifted from the v3 schema"
    );
    assert_eq!(
        resources,
        schema_enum(&schema, "ResourceName"),
        "ResourceName::all drifted from the v3 schema"
    );
}

#[test]
fn frozen_v3_report_is_schema_clean_and_matches_its_canonical_form() {
    let root = repository_root();
    let pretty = fs::read(root.join("spec/examples/scanner-report-v3.json"))
        .expect("pretty v3 report example is readable");
    let frozen = fs::read(root.join("spec/examples/scanner-report-v3.canonical.json"))
        .expect("canonical v3 report example is readable");

    let parsed = amiss_wire::json::parse(&pretty).expect("pretty example is strict JSON");
    let mut canonical = amiss_wire::json::canonical(&parsed);
    canonical.push(b'\n');
    assert_eq!(
        canonical, frozen,
        "pretty and canonical v3 examples drifted"
    );

    let schema = v3_report_schema();
    let example: serde_json::Value =
        serde_json::from_slice(&pretty).expect("v3 report example is JSON");
    let validator = jsonschema::validator_for(&schema).expect("v3 report schema compiles");
    let defects: Vec<String> = validator
        .iter_errors(&example)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert_eq!(
        defects,
        Vec::<String>::new(),
        "v3 example violates its schema"
    );
}
