use std::collections::BTreeSet;

use amiss_wire::human::{atom, atom_bytes};
use amiss_wire::json::Value;
use amiss_wire::model::RepoPath;
use amiss_wire::report::model::RepoPath as ReportPath;
use amiss_wire::report::model::{
    Evaluation, Feedback, FeedbackAction, FeedbackItem, ReportPayload,
};

use crate::view::View;

struct Channel {
    out: std::io::Stdout,
    open: bool,
}

fn line(out: &mut Channel, text: std::fmt::Arguments<'_>) {
    use std::io::Write as _;
    if out.open && writeln!(out.out, "{text}").is_err() {
        out.open = false;
    }
}

macro_rules! say {
    ($out:expr, $($arg:tt)*) => {
        line($out, format_args!($($arg)*))
    };
}

pub(crate) fn report(payload: &ReportPayload, explain_scope: bool, full_feedback: bool) {
    let mut out = Channel {
        out: std::io::stdout(),
        open: true,
    };
    let result = &payload.result;
    let items = match &payload.feedback {
        Feedback::Available(feedback) => {
            let fixes = feedback
                .items
                .iter()
                .filter(|item| item.action == FeedbackAction::Fix)
                .count();
            let checks = feedback
                .items
                .iter()
                .filter(|item| item.action == FeedbackAction::Check)
                .count();
            say!(
                &mut out,
                "amiss: {} (fix {}, check {}, existing {}, errors {}, exit {})",
                result.status.as_ref(),
                fixes,
                checks,
                feedback.existing_count,
                result.error_count,
                result.exit_code
            );
            feedback.items.as_slice()
        }
        Feedback::Unavailable(_) => {
            say!(
                &mut out,
                "amiss: scan failed (errors {}, exit {})",
                result.error_count,
                result.exit_code
            );
            &[]
        }
    };
    if explain_scope {
        explain(&mut out, payload);
    }
    for row in &payload.errors {
        if let Some(resource) = row.resource {
            say!(
                &mut out,
                "error {} {} {} {} {}/{}",
                row.phase.as_ref(),
                row.code.as_ref(),
                path_atom(row.path.as_ref()),
                resource.as_ref(),
                row.configured_limit.unwrap_or(0),
                row.observed_lower_bound.unwrap_or(0)
            );
        } else {
            say!(
                &mut out,
                "error {} {} {}",
                row.phase.as_ref(),
                row.code.as_ref(),
                path_atom(row.path.as_ref())
            );
        }
    }
    windowed(
        &mut out,
        items
            .iter()
            .filter(|item| item.action != FeedbackAction::Existing),
        "feedback",
        full_feedback,
    );
    windowed(
        &mut out,
        items
            .iter()
            .filter(|item| item.action == FeedbackAction::Existing),
        "existing",
        full_feedback,
    );
    notes(&mut out, payload);
    totals(&mut out, payload);
}

fn path_atom(path: Option<&ReportPath>) -> String {
    match path {
        Some(ReportPath::Text(path)) => atom(path.as_str()),
        Some(ReportPath::Bytes(path)) => {
            atom_bytes(&amiss_wire::human::decode_hex(&path.bytes_hex))
        }
        None => "-".to_owned(),
    }
}

pub(crate) fn references(target: &RepoPath, occurrences: &[&Value]) {
    let mut out = Channel {
        out: std::io::stdout(),
        open: true,
    };
    let shown_target = target
        .as_str()
        .map_or_else(|| atom_bytes(target.as_bytes()), atom);
    say!(
        &mut out,
        "amiss refs: target {shown_target} candidate occurrences {}",
        occurrences.len()
    );
    for occurrence in occurrences {
        let row = View::of(occurrence);
        let span = row.view("source_span");
        say!(
            &mut out,
            "reference {}:{}:{} {} {} {}",
            row.atom_or_dash("document"),
            span.number("start_line"),
            span.number("start_column"),
            atom(row.text("source_construct")),
            atom(row.view("resolution").text("kind")),
            atom(row.text("observation_id")),
        );
    }
}

fn windowed<'report>(
    out: &mut Channel,
    items: impl Iterator<Item = &'report FeedbackItem> + Clone,
    label: &str,
    full: bool,
) {
    let count = items.clone().count();
    let limit = if full { count } else { 10 };
    let overflow = count.saturating_sub(limit);
    for item in items.take(limit) {
        let mut action = item.action.as_ref().to_owned();
        if let Some(first) = action.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        say!(
            out,
            "{} target {} affected places {}",
            action,
            path_atom(item.target.as_ref()),
            item.location_count
        );
    }
    if overflow > 0 {
        say!(out, "{label} overflow: {overflow} more in the full report");
    }
}

fn notes(out: &mut Channel, payload: &ReportPayload) {
    let mut seen = BTreeSet::new();
    for row in &payload.errors {
        if !row.description.is_empty() && seen.insert(row.code) {
            say!(out, "note {}: {}", row.code.as_ref(), row.description);
        }
    }
}

fn totals(out: &mut Channel, payload: &ReportPayload) {
    let summary = &payload.summary;
    let documents = &summary.documents;
    say!(
        out,
        "documents: discovered {} scanned {} unsupported {} excluded {} unlinked {}",
        documents.discovered,
        documents.scanned,
        documents.unsupported,
        documents.excluded_builtin,
        documents.unlinked,
    );
    let references = &summary.references;
    say!(
        out,
        "references: extracted {} local {} same-repo {} external {} unsupported {} missing {}",
        references.extracted,
        references.explicit_local,
        references.same_repository,
        references.external_out_of_scope,
        references.unsupported,
        references.missing,
    );
    let declared = matches!(&payload.evaluation, Evaluation::Resolved(evaluation) if evaluation.repository.is_some());
    if !declared && references.external_out_of_scope > 0 {
        say!(
            out,
            "references: without a declared forge identity a same-repository URL counts as external"
        );
    }
    let findings = &summary.findings;
    say!(
        out,
        "findings: total {} fail {} warn {} record {}",
        findings.total,
        findings.fail,
        findings.warn,
        findings.record
    );
}

fn explain(out: &mut Channel, payload: &ReportPayload) {
    say!(
        out,
        "scope: built-in documents are *.md, *.mdx, *.markdown, *.adoc, *.asciidoc,"
    );
    say!(
        out,
        "scope: *.rst, six extensionless basenames, and .cursorrules and llms.txt"
    );
    say!(
        out,
        "scope: as plain advisory; *.ipynb and *.org are counted, never parsed"
    );
    say!(
        out,
        "scope: node_modules, vendor, third_party, dist, build, .next, and target"
    );
    say!(
        out,
        "scope: trees are excluded unless a repository policy includes them"
    );
    let documents = &payload.summary.documents;
    say!(
        out,
        "scope: this run discovered {} candidate documents and scanned {}",
        documents.discovered,
        documents.scanned,
    );
}

pub(crate) fn plan(envelope: &Value) {
    let mut out = Channel {
        out: std::io::stdout(),
        open: true,
    };
    let payload = View::of(envelope).view("payload");
    let introduced = payload.rows("introduced");
    line(
        &mut out,
        format_args!(
            "amiss external-plan: introduced {} removed {} retained {}",
            introduced.len(),
            payload.rows("removed").len(),
            payload.number("retained_count"),
        ),
    );
    let overflow = introduced.len().saturating_sub(10);
    for row in introduced.take(10) {
        line(
            &mut out,
            format_args!(
                "introduced {} in {} documents",
                row.text("destination"),
                row.rows("documents").len(),
            ),
        );
    }
    if overflow > 0 {
        line(
            &mut out,
            format_args!("introduced overflow: {overflow} more in the full plan"),
        );
    }
}

pub(crate) fn assessment(envelope: &Value) {
    let mut out = Channel {
        out: std::io::stdout(),
        open: true,
    };
    let payload = View::of(envelope).view("payload");
    let verdicts = payload.rows("verdicts");
    let count = |wanted: &str| {
        verdicts
            .clone()
            .filter(|row| row.text("verdict") == wanted)
            .count()
    };
    let refuted = count("refuted");
    let unproven = count("unproven");
    let reachable = count("reachable");
    line(
        &mut out,
        format_args!(
            "amiss external-assess: refuted {refuted} unproven {unproven} reachable {reachable}",
        ),
    );
    for row in verdicts
        .clone()
        .filter(|row| row.text("verdict") == "refuted")
        .take(10)
    {
        line(
            &mut out,
            format_args!(
                "refuted {} ({})",
                atom(row.text("destination")),
                row.text("reason")
            ),
        );
    }
    let overflow = refuted.saturating_sub(10);
    if overflow > 0 {
        line(
            &mut out,
            format_args!("refuted overflow: {overflow} more in the full assessment"),
        );
    }
    let retargets = verdicts.filter(|row| !row.text("retarget").is_empty());
    let retarget_count = retargets.clone().count();
    for row in retargets.take(10) {
        line(
            &mut out,
            format_args!(
                "retarget suggestion {} -> {}",
                atom(row.text("destination")),
                atom(row.text("retarget"))
            ),
        );
    }
    let overflow = retarget_count.saturating_sub(10);
    if overflow > 0 {
        line(
            &mut out,
            format_args!("retarget overflow: {overflow} more in the full assessment"),
        );
    }
}
