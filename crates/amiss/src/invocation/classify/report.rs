use std::collections::BTreeSet;
use std::path::PathBuf;

use amiss_wire::model::RepoPath;

use super::super::arguments::{Gathered, Slot};
use super::super::{
    AssessInvocation, Code, Command, OutputFormat, PlanInvocation, RecordSetInvocation,
    RefsInvocation, RenderInvocation, Verb,
};

pub(super) fn classify_report_command(
    mut codes: BTreeSet<Code>,
    gathered: &Gathered,
    format: OutputFormat,
) -> Result<Command, BTreeSet<Code>> {
    match gathered.verb {
        Some(Verb::ExternalPlan) => {
            let formats = [OutputFormat::Human, OutputFormat::Json];
            let report = report_path(codes, gathered, format, &formats, &pure_foreign(gathered))?;
            Ok(Command::Plan(PlanInvocation { report, format }))
        }
        Some(Verb::LocaleAssess) => {
            let [plan, evidence] = assess_pair(codes, gathered, format)?;
            Ok(Command::LocaleAssess(AssessInvocation {
                plan,
                evidence,
                format,
            }))
        }
        Some(Verb::ExternalAssess) => {
            let [plan, evidence] = assess_pair(codes, gathered, format)?;
            Ok(Command::Assess(AssessInvocation {
                plan,
                evidence,
                format,
            }))
        }
        Some(Verb::Render) => {
            if gathered.format.occurrences == 0 {
                codes.insert(Code::InvalidInvocation);
            }
            let formats = [
                OutputFormat::Human,
                OutputFormat::Sarif,
                OutputFormat::CodeQuality,
                OutputFormat::Junit,
            ];
            let report = report_path(codes, gathered, format, &formats, &pure_foreign(gathered))?;
            Ok(Command::Render(RenderInvocation {
                report,
                format,
                full: gathered.full == 1,
            }))
        }
        Some(Verb::Refs) => classify_refs(codes, gathered, format),
        Some(Verb::RecordSet) => classify_record_set(codes, gathered, format),
        Some(
            Verb::Check
            | Verb::Fix
            | Verb::Adopt
            | Verb::Claim
            | Verb::PolicyInclude
            | Verb::LocaleInventory,
        )
        | None => {
            codes.insert(Code::InvalidInvocation);
            Err(codes)
        }
    }
}

/// The report-bound forms: one report path, projected through one of the
/// formats that form admits.
fn report_path(
    codes: BTreeSet<Code>,
    gathered: &Gathered,
    format: OutputFormat,
    formats: &[OutputFormat],
    foreign_pure: &[&Slot],
) -> Result<PathBuf, BTreeSet<Code>> {
    let [report] = classify_pure(
        codes,
        gathered,
        format,
        formats,
        [&gathered.report],
        foreign_pure,
    )?;
    Ok(report)
}

/// The paths every report-bound form but `refs` treats as foreign.
fn pure_foreign(gathered: &Gathered) -> [&Slot; 4] {
    [
        &gathered.plan,
        &gathered.evidence,
        &gathered.target,
        &gathered.target_bytes_hex,
    ]
}

/// The reference form: one report and exactly one spelling of the target
/// path, text or raw bytes.
fn classify_refs(
    codes: BTreeSet<Code>,
    gathered: &Gathered,
    format: OutputFormat,
) -> Result<Command, BTreeSet<Code>> {
    let formats = [OutputFormat::Human, OutputFormat::Json];
    let foreign: [&Slot; 2] = [&gathered.plan, &gathered.evidence];
    let report = report_path(codes, gathered, format, &formats, &foreign)?;
    let target = match (
        gathered.target.unique_value(),
        gathered.target_bytes_hex.unique_value(),
    ) {
        (Some(target), None) => RepoPath::new(target.to_owned()),
        (None, Some(hex)) if hex.len() <= 8192 && hex.len() % 2 == 0 => hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            .then(|| hex::decode(hex).ok())
            .flatten()
            .and_then(RepoPath::from_bytes),
        (Some(_) | None, Some(_)) | (None, None) => None,
    }
    .ok_or_else(|| BTreeSet::from([Code::InvalidInvocation]))?;
    Ok(Command::Refs(RefsInvocation {
        report,
        target,
        format,
    }))
}

/// The shared pair form: one plan and one evidence document, judged offline.
fn assess_pair(
    codes: BTreeSet<Code>,
    gathered: &Gathered,
    format: OutputFormat,
) -> Result<[PathBuf; 2], BTreeSet<Code>> {
    classify_pure(
        codes,
        gathered,
        format,
        &[OutputFormat::Human, OutputFormat::Json],
        [&gathered.plan, &gathered.evidence],
        &[
            &gathered.report,
            &gathered.target,
            &gathered.target_bytes_hex,
        ],
    )
}

fn classify_record_set(
    mut codes: BTreeSet<Code>,
    gathered: &Gathered,
    format: OutputFormat,
) -> Result<Command, BTreeSet<Code>> {
    if gathered.format.occurrences > 0 {
        codes.insert(Code::InvalidInvocation);
    }
    let [input] = classify_pure(
        codes,
        gathered,
        format,
        &[OutputFormat::Human],
        [&gathered.evidence],
        &[
            &gathered.report,
            &gathered.plan,
            &gathered.target,
            &gathered.target_bytes_hex,
        ],
    )?;
    Ok(Command::RecordSet(RecordSetInvocation { input }))
}

/// The pure-form gate: a report-bound verb reads its own path flags and
/// projects only through one of its admitted formats; every scan, claim, and
/// adoption option is foreign, as are the other pure forms' paths. Accepts
/// with exactly one path per required slot, in order, or carries every code.
fn classify_pure<const N: usize>(
    mut codes: BTreeSet<Code>,
    gathered: &Gathered,
    format: OutputFormat,
    formats: &[OutputFormat],
    required: [&Slot; N],
    foreign_pure: &[&Slot],
) -> Result<[PathBuf; N], BTreeSet<Code>> {
    let foreign = [
        &gathered.repo,
        &gathered.object_format,
        &gathered.base,
        &gathered.candidate,
        &gathered.repository,
        &gathered.ref_name,
        &gathered.default_branch_ref,
        &gathered.forge,
        &gathered.profile,
        &gathered.floor_digest,
        &gathered.debt_owner,
        &gathered.debt_reason,
        &gathered.created_at,
        &gathered.expires_at,
        &gathered.debt_output,
        &gathered.claim_path,
        &gathered.claim_line,
        &gathered.claim_name,
        &gathered.suffix,
        &gathered.adapter,
        &gathered.context,
        &gathered.semantic_template,
    ];
    if foreign
        .iter()
        .chain(foreign_pure)
        .any(|slot| slot.occurrences > 0)
        || gathered.index > 0
        || gathered.explain_scope > 0
        || !formats.contains(&format)
    {
        codes.insert(Code::InvalidInvocation);
    }
    let mut paths = Vec::with_capacity(N);
    for slot in required {
        match slot.unique_value() {
            Some("") | None => {
                codes.insert(Code::InvalidInvocation);
            }
            Some(path) => paths.push(PathBuf::from(path)),
        }
    }
    if !codes.is_empty() {
        return Err(codes);
    }
    paths
        .try_into()
        .map_err(|_mismatch| BTreeSet::from([Code::InvalidInvocation]))
}
