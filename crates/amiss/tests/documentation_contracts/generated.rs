#![expect(
    clippy::expect_used,
    reason = "integration assertions over repository-owned documentation and fixtures"
)]

use std::fmt::Write as _;
use std::fs;

use amiss_git::GitLimits;
use amiss_scan::ScanLimits;
use amiss_wire::controls::{ORGANIZATION_POLICY_ENTRIES_LIMIT, ResourceName};
use amiss_wire::model::ForgeDialect;
use amiss_wire::report::{
    AnalysisErrorCode, EVALUATOR_MANAGED_MEMORY_BYTES, FindingKind, MACHINE_JSON_BYTES,
    PRIVATE_TEMPORARY_STORAGE_BYTES,
};
use strum::IntoEnumIterator;

use crate::support::{command_grammar, report_schema, repository_root, schema_enum};

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
            kind.as_ref(),
            kind.built_in_disposition(amiss_wire::controls::Profile::Observe)
                .as_ref(),
            kind.built_in_disposition(amiss_wire::controls::Profile::Enforce)
                .as_ref(),
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
        ResourceName::AggregateHeadingAnchorEvaluationBytesPerSnapshot => {
            scan.aggregate_heading_anchor_evaluation_bytes_per_snapshot
        }
        ResourceName::ProjectionAssertionsPerSnapshot => scan.projection_assertions_per_snapshot,
        ResourceName::AggregateProjectionSelectedBytesPerSnapshot => {
            scan.aggregate_projection_selected_bytes_per_snapshot
        }
        ResourceName::ProjectionRecordsComparedPerSnapshot => {
            scan.projection_records_compared_per_snapshot
        }
        ResourceName::AggregateProjectionProjectedBytesPerSnapshot => {
            scan.aggregate_projection_projected_bytes_per_snapshot
        }
        ResourceName::AggregateProjectionPreviewBytesPerSnapshot => {
            scan.aggregate_projection_preview_bytes_per_snapshot
        }
        ResourceName::AggregateDocumentBytesPerSnapshot => {
            scan.aggregate_document_bytes_per_snapshot
        }
        ResourceName::RawLinkDestinationBytes => scan.raw_link_destination_bytes,
        ResourceName::ParserNesting => scan.parser_nesting,
        ResourceName::ParserNodesPerDocument => scan.parser_nodes_per_document,
        ResourceName::ParserNodesPerSnapshot => scan.parser_nodes_per_snapshot,
        ResourceName::AggregateEmbeddedCodeEvaluationBytesPerSnapshot => {
            scan.aggregate_embedded_code_evaluation_bytes_per_snapshot
        }
        ResourceName::IgnoreDeclarationBlobBytes => scan.ignore_declaration_blob_bytes,
        ResourceName::AggregateIgnoreDeclarationBytesPerSnapshot => {
            scan.aggregate_ignore_declaration_bytes_per_snapshot
        }
        ResourceName::ReferencesPerDocument => scan.references_per_document,
        ResourceName::ReferencesPerSnapshot => scan.references_per_snapshot,
        ResourceName::DeclaredLabelsPerSnapshot => scan.declared_labels_per_snapshot,
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
        meanings_list(
            FindingKind::all().map(|kind| (Into::<&'static str>::into(kind), kind.meaning())),
        ),
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
        meanings_list(
            AnalysisErrorCode::all().map(|code| (Into::<&'static str>::into(code), code.meaning())),
        ),
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
        command_grammar(),
        "{} drifted from the grammar the refusal prints",
        path.display(),
    );
}

#[test]
fn the_status_page_names_every_grammar_form() {
    let path = repository_root().join("docs/src/status.md");
    let document = fs::read_to_string(&path).expect("status documentation is readable");
    let row = document
        .lines()
        .find(|line| line.starts_with("| Command |"))
        .expect("the supported-surface table has a Command row");
    let forms: Vec<String> = command_grammar()
        .lines()
        .filter_map(|line| line.strip_prefix("amiss "))
        .filter_map(|form| form.split_whitespace().next())
        .map(|verb| format!("`amiss {verb}`"))
        .collect();
    for form in &forms {
        assert!(
            row.contains(form.as_str()),
            "{} Command row omits {form}",
            path.display(),
        );
    }
    assert_eq!(
        row.matches("`amiss ").count(),
        forms.len(),
        "{} Command row names a form outside the closed grammar",
        path.display(),
    );
    let spelled = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven", "twelve",
    ]
    .get(forms.len())
    .expect("the grammar stays below twelve forms");
    assert!(
        row.contains(&format!("closed at those {spelled} forms")),
        "{} Command row miscounts the closed grammar",
        path.display(),
    );
}

#[test]
fn meaning_sentences_stay_inside_the_wire_bounds() {
    let sentences = FindingKind::all()
        .map(|kind| (Into::<&'static str>::into(kind), kind.meaning()))
        .chain(
            AnalysisErrorCode::all().map(|code| (Into::<&'static str>::into(code), code.meaning())),
        );
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
        .map(|kind| kind.as_ref().to_owned())
        .collect();
    let codes: Vec<String> = AnalysisErrorCode::all()
        .map(|code| code.as_ref().to_owned())
        .collect();
    let resources: Vec<String> = ResourceName::all()
        .map(|resource| resource.as_str().to_owned())
        .collect();
    let forges: Vec<String> = ForgeDialect::iter()
        .map(|forge| forge.as_ref().to_owned())
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
