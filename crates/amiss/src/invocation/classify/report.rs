use std::collections::BTreeSet;
use std::path::PathBuf;

use amiss_wire::human::atom;
use amiss_wire::model::RepoPath;

use super::super::arguments::{Gathered, Slot, optional, required};
use super::super::{
    AnalysisErrorCode, AssessInvocation, Command, OutputFormat, PlanInvocation,
    RecordSetInvocation, RefsInvocation, Refusal, RenderInvocation, Verb,
};
use super::{invalid, refuse_foreign};

pub(super) fn classify_report_command(
    mut refusals: BTreeSet<Refusal>,
    gathered: &Gathered,
    verb: Verb,
    format: OutputFormat,
) -> Result<Command, BTreeSet<Refusal>> {
    let report = [(&gathered.report, "--report")];
    match verb {
        Verb::ExternalPlan => {
            let formats = [OutputFormat::Human, OutputFormat::Json];
            let [report] = classify_pure(refusals, gathered, verb, format, &formats, report)?;
            Ok(Command::Plan(PlanInvocation { report, format }))
        }
        Verb::LocaleAssess | Verb::ExternalAssess => {
            let formats = [OutputFormat::Human, OutputFormat::Json];
            let wanted = [
                (&gathered.plan, "--plan"),
                (&gathered.evidence, "--evidence"),
            ];
            let [plan, evidence] =
                classify_pure(refusals, gathered, verb, format, &formats, wanted)?;
            let assess = AssessInvocation {
                plan,
                evidence,
                format,
            };
            Ok(if verb == Verb::LocaleAssess {
                Command::LocaleAssess(assess)
            } else {
                Command::Assess(assess)
            })
        }
        Verb::Render => {
            if gathered.format.occurrences == 0 {
                refusals.insert(invalid("--format is required by render".to_owned()));
            }
            let formats = [
                OutputFormat::Human,
                OutputFormat::Sarif,
                OutputFormat::CodeQuality,
                OutputFormat::Junit,
            ];
            let [report] = classify_pure(refusals, gathered, verb, format, &formats, report)?;
            Ok(Command::Render(RenderInvocation {
                report,
                format,
                full: gathered.full == 1,
            }))
        }
        Verb::Refs => classify_refs(refusals, gathered, format),
        Verb::RecordSet => {
            let wanted = [(&gathered.evidence, "--evidence")];
            let formats = [OutputFormat::Human];
            let [input] = classify_pure(refusals, gathered, verb, format, &formats, wanted)?;
            Ok(Command::RecordSet(RecordSetInvocation { input }))
        }
        Verb::Check
        | Verb::Fix
        | Verb::Adopt
        | Verb::Claim
        | Verb::PolicyInclude
        | Verb::LocaleInventory => {
            refusals.insert(invalid(
                AnalysisErrorCode::InvalidInvocation.meaning().to_owned(),
            ));
            Err(refusals)
        }
    }
}

/// The reference form: one report and exactly one spelling of the target
/// path, text or raw bytes.
fn classify_refs(
    refusals: BTreeSet<Refusal>,
    gathered: &Gathered,
    format: OutputFormat,
) -> Result<Command, BTreeSet<Refusal>> {
    let formats = [OutputFormat::Human, OutputFormat::Json];
    let wanted = [(&gathered.report, "--report")];
    let [report] = classify_pure(refusals, gathered, Verb::Refs, format, &formats, wanted)?;
    let text =
        optional(&gathered.target, "--target").map_err(|refusal| BTreeSet::from([refusal]))?;
    let hex = optional(&gathered.target_bytes_hex, "--target-bytes-hex")
        .map_err(|refusal| BTreeSet::from([refusal]))?;
    let target = match (text, hex) {
        (Some(text), None) => RepoPath::new(text.to_owned()).ok_or_else(|| {
            invalid(format!(
                "--target must be a repository path, got {}",
                atom(text)
            ))
        }),
        (None, Some(hex)) => (hex.len() <= 8192)
            .then(|| hex::decode(hex).ok())
            .flatten()
            .filter(|bytes| hex::encode(bytes) == *hex)
            .and_then(RepoPath::from_bytes)
            .ok_or_else(|| {
                invalid(format!(
                    "--target-bytes-hex must be the lowercase hex spelling of a repository path, got {}",
                    atom(hex)
                ))
            }),
        (Some(_), Some(_)) => Err(invalid(
            "--target and --target-bytes-hex are exclusive".to_owned(),
        )),
        (None, None) => Err(invalid(
            "one of --target or --target-bytes-hex is required".to_owned(),
        )),
    }
    .map_err(|refusal| BTreeSet::from([refusal]))?;
    Ok(Command::Refs(RefsInvocation {
        report,
        target,
        format,
    }))
}

/// The pure-form gate: a report-bound verb reads its own path flags and
/// projects only through one of its admitted formats; every other option is
/// foreign. Accepts with exactly one path per wanted slot, in order, or
/// carries every refusal.
fn classify_pure<const N: usize>(
    mut refusals: BTreeSet<Refusal>,
    gathered: &Gathered,
    verb: Verb,
    format: OutputFormat,
    formats: &[OutputFormat],
    wanted: [(&Slot, &str); N],
) -> Result<[PathBuf; N], BTreeSet<Refusal>> {
    let mut owned: Vec<&str> = wanted.iter().map(|(_slot, option)| *option).collect();
    if verb == Verb::Refs {
        owned.extend(["--target", "--target-bytes-hex"]);
    }
    if verb != Verb::RecordSet {
        owned.push("--format");
    }
    refuse_foreign(&mut refusals, gathered, verb, &owned);
    if owned.contains(&"--format") && !formats.contains(&format) {
        let admitted: Vec<&'static str> = formats.iter().map(|format| (*format).into()).collect();
        refusals.insert(invalid(format!(
            "--format {} is not admitted by {}, which takes {}",
            format.as_ref(),
            verb.as_ref(),
            admitted.join(", ")
        )));
    }
    let mut paths = Vec::with_capacity(N);
    for (slot, option) in wanted {
        match required(slot, option) {
            Ok("") => {
                refusals.insert(invalid(format!("{option} must not be empty")));
            }
            Ok(path) => paths.push(PathBuf::from(path)),
            Err(refusal) => {
                refusals.insert(refusal);
            }
        }
    }
    if !refusals.is_empty() {
        return Err(refusals);
    }
    paths.try_into().map_err(|_mismatch| {
        BTreeSet::from([invalid(
            AnalysisErrorCode::InvalidInvocation.meaning().to_owned(),
        )])
    })
}
