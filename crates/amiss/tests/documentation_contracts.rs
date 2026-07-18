#![expect(
    clippy::expect_used,
    reason = "integration assertions over repository-owned documentation and fixtures"
)]

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use amiss_git::GitLimits;
use amiss_scan::ScanLimits;
use amiss_wire::controls::{
    DebtSnapshot, ExecutionConstraintDescriptor, ORGANIZATION_POLICY_ENTRIES_LIMIT,
    OrganizationFloor, ResourceName, ScannerPolicy, TrustedTimeStatement, WaiverBundle,
};
use amiss_wire::manifest::ReleaseManifest;
use amiss_wire::model::ForgeDialect;
use amiss_wire::report::{
    AnalysisErrorCode, ENVELOPE_SCHEMA, EVALUATOR_MANAGED_MEMORY_BYTES, FindingKind,
    MACHINE_JSON_BYTES, PAYLOAD_SCHEMA, PRIVATE_TEMPORARY_STORAGE_BYTES,
};
use amiss_wire::requests::{ControlsRequest, EvaluationRequest, SnapshotRequest};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn report_schema() -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(repository_root().join("spec/scanner-report.schema.json"))
            .expect("report schema is readable"),
    )
    .expect("report schema is JSON")
}

fn public_schema_examples() -> Vec<(String, PathBuf, PathBuf)> {
    let specification_directory = repository_root().join("spec");
    let examples_directory = specification_directory.join("examples");
    let mut pairs = Vec::new();

    for entry in
        fs::read_dir(&specification_directory).expect("specification directory is readable")
    {
        let schema_path = entry.expect("specification entry is readable").path();
        if !schema_path.is_file() {
            continue;
        }

        let file_name = schema_path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("public schema names are UTF-8");
        let Some(contract_name) = file_name.strip_suffix(".schema.json") else {
            continue;
        };
        let example_path = examples_directory.join(format!("{contract_name}.json"));
        assert!(
            example_path.is_file(),
            "{} has no matching public example at {}",
            schema_path.display(),
            example_path.display(),
        );
        pairs.push((contract_name.to_owned(), schema_path, example_path));
    }

    pairs.sort();
    assert!(!pairs.is_empty(), "no public JSON Schema contracts found");
    pairs
}

fn parse_defect<T, E: std::fmt::Debug>(result: Result<T, E>) -> Option<String> {
    result.err().map(|error| format!("{error:?}"))
}

fn example_reader_defect(contract_name: &str, bytes: &[u8]) -> Option<String> {
    match contract_name {
        "debt-snapshot" => parse_defect(DebtSnapshot::parse(bytes)),
        "organization-floor" => parse_defect(OrganizationFloor::parse(bytes)),
        "scanner-controls-request" => parse_defect(ControlsRequest::parse(bytes)),
        "scanner-evaluation-request" => parse_defect(EvaluationRequest::parse(bytes)),
        "scanner-execution-constraint" => parse_defect(ExecutionConstraintDescriptor::parse(bytes)),
        "scanner-policy" => parse_defect(ScannerPolicy::parse(bytes)),
        "scanner-release-manifest" => parse_defect(ReleaseManifest::parse(bytes)),
        "scanner-report" => parse_defect(amiss_wire::json::parse(bytes)),
        "scanner-snapshot-request" => parse_defect(SnapshotRequest::parse(bytes)),
        "scanner-trusted-time-statement" => parse_defect(TrustedTimeStatement::parse(bytes)),
        "waiver-bundle" => parse_defect(WaiverBundle::parse(bytes)),
        _ => Some("no authoritative example reader is registered".to_owned()),
    }
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
    for kind in FindingKind::all() {
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

fn meanings_list<'a>(rows: impl Iterator<Item = (&'a str, &'a str)>) -> String {
    let mut list = String::new();
    for (name, meaning) in rows {
        if !list.is_empty() {
            list.push('\n');
        }
        write!(list, "- `{name}`: {meaning}").expect("writing to a String is infallible");
    }
    list
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
        ResourceName::AggregateLineFragmentEvaluationBytesPerSnapshot => {
            scan.aggregate_line_fragment_evaluation_bytes_per_snapshot
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
fn documented_finding_meanings_are_generated_from_the_engine_text() {
    let path = repository_root().join("docs/src/profiles.md");
    let document = fs::read_to_string(&path).expect("profiles documentation is readable");
    assert_eq!(
        documented_contract(&document, "finding-meanings"),
        meanings_list(FindingKind::all().map(|kind| (kind.as_str(), kind.meaning()))),
        "{} drifted from FindingKind::meaning",
        path.display(),
    );
}

#[test]
fn documented_error_meanings_are_generated_from_the_engine_text() {
    let path = repository_root().join("docs/src/limits.md");
    let document = fs::read_to_string(&path).expect("limits documentation is readable");
    assert_eq!(
        documented_contract(&document, "error-meanings"),
        meanings_list(AnalysisErrorCode::all().map(|code| (code.as_str(), code.meaning()))),
        "{} drifted from AnalysisErrorCode::meaning",
        path.display(),
    );
}

#[test]
fn documented_grammar_matches_the_refusal_grammar() {
    let path = repository_root().join("docs/src/invocation.md");
    let document = fs::read_to_string(&path).expect("invocation documentation is readable");
    let fenced = documented_contract(&document, "invocation-grammar");
    let body = fenced
        .strip_prefix("```text\n")
        .and_then(|rest| rest.strip_suffix("\n```"))
        .expect("the grammar contract is one text fence");
    assert_eq!(
        body,
        amiss::invocation::GRAMMAR,
        "{} drifted from the grammar the refusal prints",
        path.display(),
    );
}

#[test]
fn meaning_sentences_stay_inside_the_wire_bounds() {
    let sentences = FindingKind::all()
        .map(|kind| (kind.as_str(), kind.meaning()))
        .chain(AnalysisErrorCode::all().map(|code| (code.as_str(), code.meaning())));
    for (name, sentence) in sentences {
        assert!(
            (1..=400).contains(&sentence.len()),
            "{name}: the schema bounds a description at 400 bytes, got {}",
            sentence.len(),
        );
        assert!(
            sentence.chars().all(|scalar| (' '..='~').contains(&scalar)),
            "{name}: a description is printable ASCII so every lane prints it inert",
        );
        assert!(
            !sentence.contains('"'),
            "{name}: the human lane reserves double quotes for repository atoms",
        );
    }
}

#[test]
fn the_llms_index_names_real_chapters_on_the_published_book() {
    let root = repository_root();
    let path = root.join("docs/src/llms.txt");
    let document = fs::read_to_string(&path).expect("the llms index is readable");
    let mut checked = 0_usize;
    for line in document.lines() {
        let Some(rest) = line.strip_prefix("- [") else {
            continue;
        };
        let (_, after) = rest
            .split_once("](")
            .expect("an index row is a markdown link");
        let (url, tail) = after.split_once(')').expect("an index link closes");
        assert!(tail.starts_with(": "), "each row explains its page: {line}");
        let chapter = url
            .strip_prefix("https://hardmax71.github.io/amiss/")
            .and_then(|page| page.strip_suffix(".html"))
            .expect("an index link names a chapter on the published book");
        assert!(
            root.join(format!("docs/src/{chapter}.md")).is_file(),
            "{url} names a chapter that does not exist"
        );
        checked = checked.saturating_add(1);
    }
    assert!(checked >= 15, "the index covers the book, saw {checked}");
}

#[test]
fn documented_finding_examples_cover_the_report_schema() {
    let path = repository_root().join("docs/src/profiles.md");
    let document = fs::read_to_string(&path).expect("profiles documentation is readable");
    let table = documented_contract(&document, "finding-examples");
    let mut lines = table.lines();
    assert_eq!(
        lines.next(),
        Some("| Finding kind | Before | After |"),
        "{} has the wrong finding-example table header",
        path.display(),
    );
    assert_eq!(
        lines.next(),
        Some("| --- | --- | --- |"),
        "{} has the wrong finding-example table divider",
        path.display(),
    );

    let mut documented_kinds = Vec::new();
    for (index, line) in lines.enumerate() {
        let cells: Vec<&str> = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        let [kind, before, after] = cells.as_slice() else {
            panic!(
                "{} finding-example row {} must have exactly three cells",
                path.display(),
                index + 1,
            );
        };
        let kind = kind
            .strip_prefix('`')
            .and_then(|value| value.strip_suffix('`'))
            .unwrap_or_else(|| {
                panic!(
                    "{} finding-example row {} must format its kind as inline code",
                    path.display(),
                    index + 1,
                )
            });
        for (side, example) in [("before", before), ("after", after)] {
            assert!(
                !example.is_empty()
                    && !example.eq_ignore_ascii_case("tbd")
                    && !example.eq_ignore_ascii_case("todo"),
                "{} finding-example row {} needs a concrete {side} state",
                path.display(),
                index + 1,
            );
        }
        assert_ne!(
            before,
            after,
            "{} finding-example row {} must describe a change",
            path.display(),
            index + 1,
        );
        documented_kinds.push(kind.to_owned());
    }

    assert_eq!(
        documented_kinds,
        schema_enum(&report_schema(), "FindingKind"),
        "{} must give every schema finding one before/after example in schema order",
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
fn documented_enum_sources_match_the_active_report_schema() {
    let schema = report_schema();
    let findings: Vec<String> = FindingKind::all()
        .map(|kind| kind.as_str().to_owned())
        .collect();
    let codes: Vec<String> = AnalysisErrorCode::all()
        .map(|code| code.as_str().to_owned())
        .collect();
    let resources: Vec<String> = ResourceName::all()
        .map(|resource| resource.as_str().to_owned())
        .collect();
    let forges: Vec<String> = ForgeDialect::all()
        .map(|forge| forge.as_str().to_owned())
        .collect();

    assert_eq!(
        findings,
        schema_enum(&schema, "FindingKind"),
        "the runtime finding kinds drifted from the report schema"
    );
    assert_eq!(
        codes,
        schema_enum(&schema, "AnalysisErrorCode"),
        "the runtime analysis-error codes drifted from the report schema"
    );
    assert_eq!(
        resources,
        schema_enum(&schema, "ResourceName"),
        "the runtime resource names drifted from the report schema"
    );
    assert_eq!(
        forges,
        schema_enum(&schema, "ForgeDialect"),
        "the runtime forge dialects drifted from the report schema"
    );
}

#[test]
fn published_ci_examples_expose_every_moving_release_choice() {
    let root = repository_root();
    let sources = [
        (root.join("README.md"), 1_usize),
        (root.join("docs/src/ci.md"), 2_usize),
    ];
    let workspace_major = env!("CARGO_PKG_VERSION")
        .split('.')
        .next()
        .expect("a Cargo package version has a major component");
    let expected_action = format!("v{workspace_major}");

    for (path, expected_upstream_references) in &sources {
        let document = fs::read_to_string(path).expect("published CI example is readable");
        let mut amiss_references = 0_usize;
        let mut upstream_references = 0_usize;
        for (line_index, line) in document.lines().enumerate() {
            let trimmed = line.trim();
            let Some(specification) = trimmed.strip_prefix("- uses: ") else {
                continue;
            };
            if specification.starts_with("./") {
                continue;
            }
            let Some((action, reference)) = specification
                .split_whitespace()
                .next()
                .and_then(|token| token.split_once('@'))
            else {
                panic!(
                    "{}:{} has an external Action without a reference",
                    path.display(),
                    line_index + 1,
                );
            };

            if action == "HardMax71/amiss" {
                assert_eq!(
                    reference,
                    expected_action,
                    "{}:{} advertises the wrong moving Amiss release major",
                    path.display(),
                    line_index + 1,
                );
                amiss_references = amiss_references.saturating_add(1);
            } else {
                assert!(
                    reference == "<pinned-sha>"
                        || (reference.len() == 40
                            && reference.bytes().all(|byte| byte.is_ascii_hexdigit())),
                    "{}:{} hides a mutable upstream Action reference",
                    path.display(),
                    line_index + 1,
                );
                upstream_references = upstream_references.saturating_add(1);
            }
        }

        assert_eq!(
            amiss_references,
            1,
            "{} must advertise the supported Amiss Action exactly once",
            path.display(),
        );
        assert_eq!(
            upstream_references,
            *expected_upstream_references,
            "{} must keep every published upstream Action dependency explicit",
            path.display(),
        );
    }

    let ci = fs::read_to_string(root.join("docs/src/ci.md")).expect("CI documentation is readable");
    let installs: Vec<&str> = ci
        .lines()
        .map(str::trim)
        .filter(|line| line.contains("cargo install") && line.ends_with(" amiss"))
        .collect();
    assert_eq!(
        installs,
        [
            "- run: cargo install --locked --registry crates-io --version '=<reviewed-version>' amiss"
        ],
        "the direct CI form must demand an exact reviewed version without copying the current patch release"
    );
}

#[test]
fn active_report_schema_ids_match_the_writer_contract() {
    let schema = report_schema();
    assert_eq!(
        amiss_scan::report::ENVELOPE_SCHEMA,
        ENVELOPE_SCHEMA,
        "the scan and wire envelope writers disagree on the active identity"
    );
    assert_eq!(
        schema
            .pointer("/properties/schema/const")
            .and_then(serde_json::Value::as_str),
        Some(ENVELOPE_SCHEMA),
        "the active schema and writer disagree on the envelope identity"
    );
    assert_eq!(
        schema
            .pointer("/$defs/ReportPayload/properties/schema/const")
            .and_then(serde_json::Value::as_str),
        Some(PAYLOAD_SCHEMA),
        "the active schema and writer disagree on the payload identity"
    );
}

#[test]
fn all_public_contract_examples_clear_their_schema_and_registered_reader() {
    let mut defects = Vec::new();

    for (contract_name, schema_path, example_path) in public_schema_examples() {
        let schema_bytes = fs::read(&schema_path).expect("public schema is readable");
        let example_bytes = fs::read(&example_path).expect("public example is readable");
        let schema: serde_json::Value = serde_json::from_slice(&schema_bytes)
            .unwrap_or_else(|error| panic!("{} is not JSON: {error}", schema_path.display()));
        let example: serde_json::Value = serde_json::from_slice(&example_bytes)
            .unwrap_or_else(|error| panic!("{} is not JSON: {error}", example_path.display()));
        let validator = jsonschema::validator_for(&schema)
            .unwrap_or_else(|error| panic!("{} does not compile: {error}", schema_path.display()));

        defects.extend(validator.iter_errors(&example).map(|error| {
            format!(
                "{} against {} at {}: {error}",
                example_path.display(),
                schema_path.display(),
                error.instance_path(),
            )
        }));

        if let Some(error) = example_reader_defect(&contract_name, &example_bytes) {
            defects.push(format!(
                "{} was rejected by the {contract_name} example reader: {error}",
                example_path.display(),
            ));
        }
    }

    assert!(
        defects.is_empty(),
        "public contract examples violate their schemas or registered readers:\n{}",
        defects.join("\n"),
    );
}

#[test]
fn repository_relative_documentation_links_resolve() {
    let documentation_directory = repository_root().join("docs/src");
    let mut checked = 0_u64;

    for entry in
        fs::read_dir(&documentation_directory).expect("documentation directory is readable")
    {
        let path = entry.expect("documentation entry is readable").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }

        let document = fs::read_to_string(&path).expect("documentation source is readable");
        let mut fenced = false;
        for (line_index, line) in document.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                fenced = !fenced;
                continue;
            }
            if fenced {
                continue;
            }

            let mut remainder = line;
            while let Some(open) = remainder.find("](") {
                let after_open = remainder
                    .get(open + 2..)
                    .expect("the ASCII link opener ends at a UTF-8 boundary");
                let Some(close) = after_open.find(')') else {
                    break;
                };
                let destination = after_open
                    .get(..close)
                    .expect("the ASCII link closer starts at a UTF-8 boundary");
                let tree_target = if destination.starts_with("../../") {
                    Some(
                        path.parent()
                            .expect("documentation source has a parent")
                            .join(destination),
                    )
                } else {
                    destination
                        .strip_prefix("https://github.com/HardMax71/amiss/blob/main/")
                        .map(|target| repository_root().join(target))
                };
                if let Some(resolved) = tree_target {
                    let resolved = resolved
                        .to_str()
                        .and_then(|text| text.split(['#', '?']).next())
                        .map(PathBuf::from)
                        .expect("documentation link paths are UTF-8");
                    assert!(
                        resolved.exists(),
                        "{}:{} links to missing repository path {destination}",
                        path.display(),
                        line_index + 1,
                    );
                    checked = checked.saturating_add(1);
                }
                remainder = after_open
                    .get(close + 1..)
                    .expect("the ASCII link closer ends at a UTF-8 boundary");
            }
        }
    }

    assert!(
        checked > 0,
        "documentation contains no repository-relative implementation links"
    );
}

#[test]
fn report_example_is_schema_clean_and_matches_its_canonical_form() {
    let root = repository_root();
    let pretty = fs::read(root.join("spec/examples/scanner-report.json"))
        .expect("pretty report example is readable");
    let canonical_fixture = fs::read(root.join("spec/examples/scanner-report.canonical.json"))
        .expect("canonical report example is readable");

    let parsed = amiss_wire::json::parse(&pretty).expect("pretty example is strict JSON");
    let mut canonical = amiss_wire::json::canonical(&parsed);
    canonical.push(b'\n');
    assert_eq!(
        canonical, canonical_fixture,
        "pretty and canonical report examples drifted"
    );

    let schema = report_schema();
    let example: serde_json::Value =
        serde_json::from_slice(&pretty).expect("report example is JSON");
    let validator = jsonschema::validator_for(&schema).expect("report schema compiles");
    let defects: Vec<String> = validator
        .iter_errors(&example)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert_eq!(
        defects,
        Vec::<String>::new(),
        "report example violates its schema"
    );

    let payload = &example["payload"];
    for row in payload["errors"].as_array().expect("errors is an array") {
        let code = row["code"].as_str().expect("an error row names its code");
        let meaning = AnalysisErrorCode::all()
            .find(|candidate| candidate.as_str() == code)
            .expect("the example uses schema error codes")
            .meaning();
        assert_eq!(
            row["description"], meaning,
            "the example description for {code} drifted from the engine text"
        );
    }
    for row in payload["findings"]
        .as_array()
        .expect("findings is an array")
    {
        let kind = row["kind"].as_str().expect("a finding row names its kind");
        let meaning = FindingKind::all()
            .find(|candidate| candidate.as_str() == kind)
            .expect("the example uses schema finding kinds")
            .meaning();
        assert_eq!(
            row["description"], meaning,
            "the example description for {kind} drifted from the engine text"
        );
    }
}
