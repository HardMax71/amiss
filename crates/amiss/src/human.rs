use std::collections::BTreeSet;

use amiss_wire::json::Value;
use amiss_wire::model::RepoPath;

use crate::view::View;

/// The human output channel: a closed pipe ends the narration and never the
/// verdict, so a consumer that stops reading still gets the run's exit class.
struct Channel {
    out: std::io::Stdout,
    open: bool,
}

impl Channel {
    fn new() -> Self {
        Self {
            out: std::io::stdout(),
            open: true,
        }
    }

    fn line(&mut self, text: std::fmt::Arguments<'_>) {
        use std::io::Write as _;
        if self.open && writeln!(self.out, "{text}").is_err() {
            self.open = false;
        }
    }
}

macro_rules! say {
    ($out:expr, $($arg:tt)*) => {
        $out.line(format_args!($($arg)*))
    };
}

/// The human projection: a non-wire convenience over the same payload that
/// cannot change facts, ordering, totals, or exit. It prints two ten-row
/// windows, fix and check items then the existing backlog, each with its own
/// overflow line, plus retained analysis errors, their meanings, and the
/// exact raw totals. A full replay removes only the two row limits.
pub(crate) fn report(envelope: &Value, explain_scope: bool, full_feedback: bool) {
    let mut out = Channel::new();
    let envelope = View::of(envelope);
    let payload = envelope.view("payload");
    let result = payload.view("result");
    let feedback = payload.view("feedback");
    let items = feedback.rows("items");
    let available = feedback.text("status") == "available";
    if available {
        let fixes = items
            .clone()
            .filter(|item| item.text("action") == "fix")
            .count();
        let checks = items
            .clone()
            .filter(|item| item.text("action") == "check")
            .count();
        say!(
            out,
            "amiss: {} (fix {}, check {}, existing {}, errors {}, exit {})",
            result.text("status"),
            fixes,
            checks,
            feedback.number("existing_count"),
            result.number("error_count"),
            result.number("exit_code")
        );
    } else {
        say!(
            out,
            "amiss: scan failed (errors {}, exit {})",
            result.number("error_count"),
            result.number("exit_code")
        );
    }
    if explain_scope {
        explain(&mut out, payload);
    }
    for row in payload.rows("errors") {
        let resource = row.text("resource");
        if resource.is_empty() {
            say!(
                out,
                "error {} {} {}",
                row.text("phase"),
                row.text("code"),
                row.atom_or_dash("path")
            );
        } else {
            say!(
                out,
                "error {} {} {} {} {}/{}",
                row.text("phase"),
                row.text("code"),
                row.atom_or_dash("path"),
                resource,
                row.number("configured_limit"),
                row.number("observed_lower_bound")
            );
        }
    }
    windowed(
        &mut out,
        items
            .clone()
            .filter(|item| item.text("action") != "existing"),
        "feedback",
        full_feedback,
    );
    windowed(
        &mut out,
        items.filter(|item| item.text("action") == "existing"),
        "existing",
        full_feedback,
    );
    notes(&mut out, payload);
    totals(&mut out, payload);
}

pub(crate) fn references(target: &RepoPath, occurrences: &[&Value]) {
    let mut out = Channel::new();
    let shown_target = target.as_str().map_or_else(
        || amiss_wire::human::atom_bytes(target.as_bytes()),
        amiss_wire::human::atom,
    );
    say!(
        out,
        "amiss refs: target {shown_target} candidate occurrences {}",
        occurrences.len()
    );
    for occurrence in occurrences {
        let row = View::of(occurrence);
        let span = row.view("source_span");
        say!(
            out,
            "reference {}:{}:{} {} {} {}",
            row.atom_or_dash("document"),
            span.number("start_line"),
            span.number("start_column"),
            amiss_wire::human::atom(row.text("source_construct")),
            amiss_wire::human::atom(row.view("resolution").text("kind")),
            amiss_wire::human::atom(row.text("observation_id")),
        );
    }
}

fn windowed<'value>(
    out: &mut Channel,
    items: impl Iterator<Item = View<'value>> + Clone,
    label: &str,
    full: bool,
) {
    let count = items.clone().count();
    let limit = if full { count } else { 10 };
    let overflow = count.saturating_sub(limit);
    for item in items.take(limit) {
        let mut action = item.text("action").to_owned();
        if let Some(first) = action.get_mut(0..1) {
            first.make_ascii_uppercase();
        }
        say!(
            out,
            "{} target {} affected places {}",
            action,
            item.atom_or_dash("target"),
            item.number("location_count")
        );
    }
    if overflow > 0 {
        say!(out, "{label} overflow: {overflow} more in the full report");
    }
}

/// One `note` line per error code used by this run.
fn notes(out: &mut Channel, payload: View<'_>) {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for row in payload.rows("errors") {
        let name = row.text("code");
        let description = row.text("description");
        if !name.is_empty() && !description.is_empty() && seen.insert(name) {
            say!(out, "note {name}: {description}");
        }
    }
}

fn totals(out: &mut Channel, payload: View<'_>) {
    let summary = payload.view("summary");
    let documents = summary.view("documents");
    say!(
        out,
        "documents: discovered {} scanned {} unsupported {} excluded {} unlinked {}",
        documents.number("discovered"),
        documents.number("scanned"),
        documents.number("unsupported"),
        documents.number("excluded_builtin"),
        documents.number("unlinked"),
    );
    let references = summary.view("references");
    say!(
        out,
        "references: extracted {} local {} same-repo {} external {} unsupported {} missing {}",
        references.number("extracted"),
        references.number("explicit_local"),
        references.number("same_repository"),
        references.number("external_out_of_scope"),
        references.number("unsupported"),
        references.number("missing"),
    );
    let undeclared = !matches!(
        payload.view("evaluation").field("repository"),
        Some(Value::Object(_))
    );
    if undeclared && references.number("external_out_of_scope") > 0 {
        say!(
            out,
            "references: without a declared forge identity a same-repository URL counts as external"
        );
    }
    let findings = summary.view("findings");
    say!(
        out,
        "findings: total {} fail {} warn {} record {}",
        findings.number("total"),
        findings.number("fail"),
        findings.number("warn"),
        findings.number("record"),
    );
}

/// The deterministic scope explanation the human projection may add: the
/// closed built-in document classes and this run's discovered surface.
fn explain(out: &mut Channel, payload: View<'_>) {
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
    let documents = payload.view("summary").view("documents");
    say!(
        out,
        "scope: this run discovered {} candidate documents and scanned {}",
        documents.number("discovered"),
        documents.number("scanned"),
    );
}

pub(crate) fn plan(envelope: &Value) {
    let mut out = Channel::new();
    let payload = View::of(envelope).view("payload");
    let introduced = payload.rows("introduced");
    out.line(format_args!(
        "amiss external-plan: introduced {} removed {} retained {}",
        introduced.len(),
        payload.rows("removed").len(),
        payload.number("retained_count"),
    ));
    let overflow = introduced.len().saturating_sub(10);
    for row in introduced.take(10) {
        out.line(format_args!(
            "introduced {} in {} documents",
            row.text("destination"),
            row.rows("documents").len(),
        ));
    }
    if overflow > 0 {
        out.line(format_args!(
            "introduced overflow: {overflow} more in the full plan"
        ));
    }
}

pub(crate) fn assessment(envelope: &Value) {
    let mut out = Channel::new();
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
    out.line(format_args!(
        "amiss external-assess: refuted {refuted} unproven {unproven} reachable {reachable}",
    ));
    for row in verdicts
        .filter(|row| row.text("verdict") == "refuted")
        .take(10)
    {
        out.line(format_args!(
            "refuted {} ({})",
            row.text("destination"),
            row.text("reason")
        ));
    }
    let overflow = refuted.saturating_sub(10);
    if overflow > 0 {
        out.line(format_args!(
            "refuted overflow: {overflow} more in the full assessment"
        ));
    }
}
