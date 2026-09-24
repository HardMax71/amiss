use std::collections::{BTreeMap, BTreeSet};

use amiss_wire::human::{atom, atom_bytes};
use amiss_wire::model::{Digest, RepoPath};
use amiss_wire::report::model::{
    Attribution, Evaluation, Feedback, FeedbackAction, FeedbackItem, FindingFactEvidence, Impact,
    LocationSide, Occurrence, ReportPayload, SourceSpan, occurrences,
};
use amiss_wire::report::{Disposition, FindingKind};
use amiss_wire::resolution::{
    Missing, MissingTag, Resolution, ResolutionTag, UnsupportedSemanticsTag, UnsupportedTargetTag,
    VersionScopeTag,
};

mod tests;

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

/// What the caller asked the human projection to add or unbound.
#[derive(Clone, Copy)]
pub(crate) struct Options {
    pub(crate) explain_scope: bool,
    pub(crate) full: bool,
}

/// One finding under its feedback row: where it was seen and why it stands.
struct Place<'report, P> {
    action: FeedbackAction,
    target: Option<&'report P>,
    document: Option<&'report P>,
    span: Option<SourceSpan>,
    kind: FindingKind,
    key: Digest,
    members: u64,
    reason: Option<String>,
}

pub(crate) fn report<P, R, M, S, D, F>(
    payload: &ReportPayload<P, R, M, FindingFactEvidence<P, R, S, D, M>>,
    options: Options,
    path_atom: impl Fn(Option<&P>) -> String + Copy,
    resolution: F,
) where
    P: PartialEq,
    F: Fn(&R) -> (ResolutionTag, Option<String>),
{
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
                "amiss: {} (fix {}, check {}, pre-existing {}, errors {}, exit {})",
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
    if options.explain_scope {
        explain(&mut out, payload, &resolution);
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
    let affected = places(payload, path_atom, &resolution);
    windowed(
        &mut out,
        items
            .iter()
            .filter(|item| item.action != FeedbackAction::Existing),
        "feedback",
        options.full,
        path_atom,
        &affected,
    );
    windowed(
        &mut out,
        items
            .iter()
            .filter(|item| item.action == FeedbackAction::Existing),
        "pre-existing",
        options.full,
        path_atom,
        &affected,
    );
    let kinds: BTreeSet<FindingKind> = affected.iter().map(|place| place.kind).collect();
    for kind in kinds {
        say!(&mut out, "note {}: {}", kind.as_ref(), kind.meaning());
    }
    notes(&mut out, payload);
    unscanned(&mut out, payload, options.full, path_atom);
    totals(&mut out, payload);
}

/// An engine path, spelled through the atom law whichever form it took.
pub(crate) fn engine_path(path: &RepoPath) -> String {
    path.as_str()
        .map_or_else(|| atom_bytes(path.as_bytes()), atom)
}

pub(crate) fn references(target: &RepoPath, occurrences: &[Occurrence]) {
    let mut out = Channel {
        out: std::io::stdout(),
        open: true,
    };
    say!(
        &mut out,
        "amiss refs: target {} candidate occurrences {}",
        engine_path(target),
        occurrences.len()
    );
    for row in occurrences {
        say!(
            &mut out,
            "reference {}:{}:{} {} {} {}",
            engine_path(&row.observation_id_input.document),
            row.source_span.start_line,
            row.source_span.start_column,
            atom(row.observation_id_input.source_construct.as_ref()),
            atom(row.resolution.as_ref()),
            atom(&row.observation_id.to_string()),
        );
    }
}

/// The engine's own resolution, spelled for a place line: its tag and,
/// where the resolution says more, the reason with any nearby spelling.
pub(crate) fn engine_resolution(
    resolution: &Resolution<RepoPath>,
) -> (ResolutionTag, Option<String>) {
    let detail = match resolution {
        Resolution::Missing(missing) => {
            let near = match missing {
                Missing::PathNotFound {
                    near: Some(near), ..
                } => Some(engine_path(near)),
                Missing::HeadingAnchorNotFound {
                    near: Some(near), ..
                } => Some(atom(near)),
                Missing::PathNotFound { near: None, .. }
                | Missing::HeadingAnchorNotFound { near: None, .. }
                | Missing::LineFragmentOutOfRange { .. }
                | Missing::LabelNotDeclared => None,
            };
            Some(missing_detail(MissingTag::from(missing), near))
        }
        Resolution::Invalid { reason } => Some(reason.as_ref().to_owned()),
        Resolution::UnsupportedTarget(target) => {
            Some(UnsupportedTargetTag::from(target).as_ref().to_owned())
        }
        Resolution::UnsupportedSemantics(semantics) => {
            Some(UnsupportedSemanticsTag::from(semantics).as_ref().to_owned())
        }
        Resolution::UnsupportedVersion { scope } => {
            Some(VersionScopeTag::from(scope).as_ref().to_owned())
        }
        Resolution::Resolved { .. }
        | Resolution::TypeMismatch { .. }
        | Resolution::DeclaredUntracked(_)
        | Resolution::External { .. } => None,
    };
    (ResolutionTag::from(resolution), detail)
}

pub(crate) fn missing_detail(tag: MissingTag, near: Option<String>) -> String {
    near.map_or_else(
        || tag.as_ref().to_owned(),
        |near| format!("{} near {near}", tag.as_ref()),
    )
}

/// The engine groups feedback by action and target; the same grouping over
/// the findings the report carries puts every place under its row. Places
/// read in document then position order, closed by the finding key the
/// report holds distinct, so no two of them compare equal.
fn places<'report, P, R, M, S, D, A, F>(
    payload: &'report ReportPayload<P, R, M, FindingFactEvidence<P, R, S, D, M>>,
    path_atom: A,
    resolution: &F,
) -> Vec<Place<'report, P>>
where
    A: Fn(Option<&P>) -> String,
    F: Fn(&R) -> (ResolutionTag, Option<String>),
{
    let mut candidates = BTreeMap::new();
    for comparison in &payload.observations {
        let check = comparison.impact == Impact::DependencyChangedSubjectUnchanged;
        if let Some(candidate) = occurrences(comparison).candidate {
            candidates.insert(&candidate.observation_id, (candidate, check));
        }
        for candidate in &comparison.alternatives.candidate {
            candidates.insert(&candidate.observation_id, (candidate, false));
        }
    }
    let mut ordered: Vec<Place<'report, P>> = payload
        .findings
        .iter()
        .filter(|finding| finding.effective_disposition != Disposition::Record)
        .filter_map(|finding| {
            let candidate = finding
                .observation_ids
                .iter()
                .find_map(|id| candidates.get(id))
                .copied();
            let invalid = candidate.is_some_and(|(occurrence, _check)| {
                resolution(&occurrence.resolution).0 == ResolutionTag::Invalid
            });
            let target = if invalid {
                None
            } else {
                match finding.location.side {
                    LocationSide::Control => finding.location.path.as_ref(),
                    LocationSide::Global => None,
                    LocationSide::Base | LocationSide::Candidate => {
                        candidate.and_then(|(occurrence, _check)| {
                            occurrence
                                .observation_id_input
                                .extracted_intent
                                .repository_path
                                .as_ref()
                        })
                    }
                }
            };
            let action = if candidate.is_some_and(|(_occurrence, check)| check) {
                Some(FeedbackAction::Check)
            } else {
                attributed(finding.attribution, finding.location.side)
            };
            let reason = finding
                .candidate_fact
                .as_ref()
                .or(finding.base_fact.as_ref())
                .and_then(|fact| evidence_reason(&fact.evidence, resolution))
                .or_else(|| {
                    let (tag, detail) = resolution(&candidate?.0.resolution);
                    (tag != ResolutionTag::Resolved).then(|| spelled((tag, detail)))
                });
            Some(Place {
                action: action?,
                target,
                document: finding.location.path.as_ref(),
                span: finding.location.span,
                kind: finding.kind,
                key: finding.finding_key,
                members: finding.aggregation.member_count,
                reason,
            })
        })
        .collect();
    ordered.sort_by_cached_key(|place| {
        let (line, column) = place
            .span
            .map_or((0, 0), |span| (span.start_line, span.start_column));
        (path_atom(place.document), line, column, place.key)
    });
    ordered
}

fn attributed(attribution: Attribution, side: LocationSide) -> Option<FeedbackAction> {
    match attribution {
        Attribution::Introduced => Some(FeedbackAction::Fix),
        Attribution::PreExisting => Some(FeedbackAction::Existing),
        Attribution::Unknown => Some(FeedbackAction::Check),
        Attribution::Resolved => None,
        Attribution::NotApplicable => match side {
            LocationSide::Candidate | LocationSide::Control | LocationSide::Global => {
                Some(FeedbackAction::Fix)
            }
            LocationSide::Base => None,
        },
    }
}

/// What the fact's own evidence says about the place, where it says anything.
fn evidence_reason<P, R, S, D, M, F>(
    evidence: &FindingFactEvidence<P, R, S, D, M>,
    resolution: &F,
) -> Option<String>
where
    F: Fn(&R) -> (ResolutionTag, Option<String>),
{
    match evidence {
        FindingFactEvidence::Reference {
            resolution: resolved,
            ..
        } => Some(spelled(resolution(resolved))),
        FindingFactEvidence::Claim { observed, .. } => Some(observed.to_string()),
        FindingFactEvidence::Projection { observed, .. } => Some(observed.as_ref().to_owned()),
        FindingFactEvidence::BrokenRedirect { reason, .. } => Some(reason.to_string()),
        FindingFactEvidence::Control { .. }
        | FindingFactEvidence::Document { .. }
        | FindingFactEvidence::DuplicateRoute { .. }
        | FindingFactEvidence::Observation { .. } => None,
    }
}

fn spelled((tag, detail): (ResolutionTag, Option<String>)) -> String {
    detail.unwrap_or_else(|| tag.as_ref().to_owned())
}

/// One place's kind and reason, in the single spelling a row heading and a
/// place line both read from.
fn finding_tokens<P>(place: &Place<'_, P>) -> String {
    place.reason.as_ref().map_or_else(
        || place.kind.as_ref().to_owned(),
        |reason| format!("{} {reason}", place.kind.as_ref()),
    )
}

/// The kind and reason every place under one row carries, where they all
/// carry the same one. A row whose places disagree answers `None`, so a
/// heading never speaks for a place that differs from it.
fn shared_tokens<P>(places: &[&Place<'_, P>]) -> Option<String> {
    let first = places.first()?;
    places
        .iter()
        .all(|place| place.kind == first.kind && place.reason == first.reason)
        .then(|| finding_tokens(first))
}

fn windowed<'report, P: 'report + PartialEq>(
    out: &mut Channel,
    items: impl Iterator<Item = &'report FeedbackItem<P>> + Clone,
    label: &str,
    full: bool,
    path_atom: impl Fn(Option<&P>) -> String,
    places: &[Place<'report, P>],
) {
    let rows = items.clone().count();
    let window = if full { usize::MAX } else { 10 };
    let overflow = rows.saturating_sub(window);
    for item in items.take(window) {
        let action = match item.action {
            FeedbackAction::Fix => "Fix",
            FeedbackAction::Check => "Check",
            FeedbackAction::Existing => "Pre-existing",
        };
        let under: Vec<&Place<'report, P>> = places
            .iter()
            .filter(|place| place.action == item.action && place.target == item.target.as_ref())
            .collect();
        let shared = shared_tokens(&under);
        say!(
            out,
            "{action} target {} affected places {}{}",
            path_atom(item.target.as_ref()),
            item.location_count,
            shared
                .as_ref()
                .map_or_else(String::new, |tokens| format!(" {tokens}"))
        );
        for place in under.iter().take(window) {
            let at = place.span.map_or_else(String::new, |span| {
                format!(":{}:{}", span.start_line, span.start_column)
            });
            let tokens = if shared.is_some() {
                String::new()
            } else {
                format!(" {}", finding_tokens(place))
            };
            let members = if place.members > 1 {
                format!(" ({} places)", place.members)
            } else {
                String::new()
            };
            say!(out, "  {}{at}{tokens}{members}", path_atom(place.document));
        }
        let hidden = under.len().saturating_sub(window);
        if hidden > 0 {
            say!(out, "  places overflow: {hidden} more in the full report");
        }
    }
    if overflow > 0 {
        say!(out, "{label} overflow: {overflow} more in the full report");
    }
}

/// Every candidate document the run did not scan, named with the reason it
/// carries, so the unsupported total below is a list a reader can act on
/// rather than a number.
fn unscanned<P, R, M, E, A>(
    out: &mut Channel,
    payload: &ReportPayload<P, R, M, E>,
    full: bool,
    path_atom: A,
) where
    A: Fn(Option<&P>) -> String,
{
    let rows: Vec<_> = payload
        .documents
        .iter()
        .filter_map(|row| {
            let reason = row.candidate.as_ref()?.unsupported_reason.as_ref()?;
            Some((&row.path, reason))
        })
        .collect();
    let window = if full { usize::MAX } else { 10 };
    for (path, reason) in rows.iter().take(window) {
        say!(out, "unsupported {} {reason}", path_atom(Some(path)));
    }
    let hidden = rows.len().saturating_sub(window);
    if hidden > 0 {
        say!(
            out,
            "unsupported overflow: {hidden} more in the full report"
        );
    }
}

fn notes<P, R, M, E>(out: &mut Channel, payload: &ReportPayload<P, R, M, E>) {
    let mut seen = BTreeSet::new();
    for row in &payload.errors {
        if !row.description.is_empty() && seen.insert(row.code) {
            say!(out, "note {}: {}", row.code.as_ref(), row.description);
        }
    }
}

fn totals<P, R, M, E>(out: &mut Channel, payload: &ReportPayload<P, R, M, E>) {
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
            "references: without --repository <host>/<owner>/<name> --ref refs/heads/<branch> --default-branch-ref refs/heads/<default> (and --forge <dialect> on a self-hosted host) a same-repository URL counts as external"
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
    let mut records: BTreeMap<FindingKind, u64> = BTreeMap::new();
    for finding in &payload.findings {
        if finding.effective_disposition == Disposition::Record {
            let count = records.entry(finding.kind).or_default();
            *count = count.saturating_add(finding.aggregation.member_count);
        }
    }
    if !records.is_empty() {
        let listed: Vec<String> = records
            .iter()
            .map(|(kind, count)| format!("{} {count}", kind.as_ref()))
            .collect();
        say!(out, "records: {}", listed.join(", "));
    }
}

/// How often each declined reason answered a reference this report carries,
/// over the same candidate occurrences the summary counted as extracted.
fn declines<P, R, M, E, F>(
    payload: &ReportPayload<P, R, M, E>,
    resolution: &F,
) -> BTreeMap<String, u64>
where
    F: Fn(&R) -> (ResolutionTag, Option<String>),
{
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for comparison in &payload.observations {
        let alternatives = comparison.alternatives.candidate.iter();
        for occurrence in occurrences(comparison)
            .candidate
            .into_iter()
            .chain(alternatives)
        {
            let (tag, reason) = resolution(&occurrence.resolution);
            if !matches!(
                tag,
                ResolutionTag::UnsupportedSemantics
                    | ResolutionTag::UnsupportedTarget
                    | ResolutionTag::UnsupportedVersion
            ) {
                continue;
            }
            let row = reason.map_or_else(
                || tag.as_ref().to_owned(),
                |reason| format!("{} {reason}", tag.as_ref()),
            );
            let count = counts.entry(row).or_default();
            *count = count.saturating_add(1);
        }
    }
    counts
}

fn explain<P, R, M, E, F>(out: &mut Channel, payload: &ReportPayload<P, R, M, E>, resolution: &F)
where
    F: Fn(&R) -> (ResolutionTag, Option<String>),
{
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
        "scope: {}",
        amiss_scan::document::EXCLUDED_TREES.join(", ")
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
    say!(
        out,
        "scope: a destination the tree cannot answer for is declined with a reason"
    );
    let declined = declines(payload, resolution);
    let total = declined.values().copied().fold(0_u64, u64::saturating_add);
    say!(
        out,
        "scope: this run declined {total} references and resolved {}",
        payload.summary.references.resolved,
    );
    let mut ranked: Vec<(String, u64)> = declined.into_iter().collect();
    ranked.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (row, count) in ranked {
        say!(out, "scope: declined {row} {count}");
    }
}

pub(crate) fn plan(payload: &amiss_wire::external::ExternalPlan) {
    let mut out = Channel {
        out: std::io::stdout(),
        open: true,
    };
    let introduced = &payload.introduced;
    line(
        &mut out,
        format_args!(
            "amiss external-plan: introduced {} removed {} retained {}",
            introduced.len(),
            payload.removed.len(),
            payload.retained_count,
        ),
    );
    let overflow = introduced.len().saturating_sub(10);
    for row in introduced.iter().take(10) {
        line(
            &mut out,
            format_args!(
                "introduced {} in {} documents",
                row.destination,
                row.documents.len(),
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

pub(crate) fn coverage(payload: &amiss_wire::locale::LocaleCoverageAssessment) {
    let mut out = Channel {
        out: std::io::stdout(),
        open: true,
    };
    let coverage = &payload.coverage;
    line(
        &mut out,
        format_args!(
            "amiss locale-assess: {} missing {} orphaned {} fallbacks {} lineage {}",
            payload.verdict.as_ref(),
            coverage.target_missing.len(),
            coverage.target_orphaned.len(),
            coverage.fallbacks.len(),
            coverage.lineage.len(),
        ),
    );
    for reason in payload.reasons.iter().take(10) {
        line(&mut out, format_args!("reason {reason}"));
    }
    let overflow = payload.reasons.len().saturating_sub(10);
    if overflow > 0 {
        line(
            &mut out,
            format_args!("reason overflow: {overflow} more in the full assessment"),
        );
    }
}

pub(crate) fn inventory(payload: &amiss_wire::locale::LocaleCoverageEvidence) {
    let mut out = Channel {
        out: std::io::stdout(),
        open: true,
    };
    let identical = payload
        .target
        .pages
        .iter()
        .filter(|page| {
            matches!(
                page.origin,
                amiss_wire::locale::LocaleTargetOrigin::Fallback { .. }
            )
        })
        .count();
    for (locale, pages, complete) in [
        (
            &payload.scope.source_locale,
            payload.source.pages.len(),
            payload.source.complete,
        ),
        (
            &payload.scope.target_locale,
            payload.target.pages.len(),
            payload.target.complete,
        ),
    ] {
        let completeness = if complete { "complete" } else { "partial" };
        line(
            &mut out,
            format_args!("amiss locale-inventory: {locale} {pages} pages {completeness}"),
        );
    }
    line(
        &mut out,
        format_args!("target pages still carrying the source bytes: {identical}"),
    );
    let producer = &payload.producer;
    line(
        &mut out,
        format_args!(
            "producer {} {} context {}",
            producer.identity.as_str(),
            producer.version,
            producer.context_digest,
        ),
    );
}

pub(crate) fn assessment(payload: &amiss_wire::external::ExternalAssessment) {
    use amiss_wire::external::ExternalVerdict;
    let mut out = Channel {
        out: std::io::stdout(),
        open: true,
    };
    let verdicts = &payload.verdicts;
    let count = |wanted| verdicts.iter().filter(|row| row.verdict == wanted).count();
    let refuted = count(ExternalVerdict::Refuted);
    let unproven = count(ExternalVerdict::Unproven);
    let reachable = count(ExternalVerdict::Reachable);
    line(
        &mut out,
        format_args!(
            "amiss external-assess: refuted {refuted} unproven {unproven} reachable {reachable}",
        ),
    );
    for row in verdicts
        .iter()
        .filter(|row| row.verdict == ExternalVerdict::Refuted)
        .take(10)
    {
        line(
            &mut out,
            format_args!(
                "refuted {} ({})",
                atom(&row.destination),
                row.reason.as_ref().map_or("", AsRef::as_ref)
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
    let retargets = verdicts.iter().filter_map(|row| {
        row.retarget
            .as_ref()
            .map(|target| (&row.destination, target))
    });
    let retarget_count = retargets.clone().count();
    for (destination, target) in retargets.take(10) {
        line(
            &mut out,
            format_args!(
                "retarget suggestion {} -> {}",
                atom(destination),
                atom(target)
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
