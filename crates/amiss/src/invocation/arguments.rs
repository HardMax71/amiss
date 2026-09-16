use std::collections::BTreeSet;
use std::ffi::OsString;

use amiss_wire::human::atom;
use strum::IntoEnumIterator as _;

use super::{Code, HELP_FLAGS, OutputFormat, Refusal, VERSION_FLAGS, Verb};

#[derive(Default)]
pub(super) struct Slot {
    pub(super) occurrences: usize,
    values: Vec<String>,
}

impl Slot {
    fn record(&mut self, value: Option<String>) {
        self.occurrences = self.occurrences.saturating_add(1);
        if let Some(value) = value {
            self.values.push(value);
        }
    }

    pub(super) fn unique_value(&self) -> Option<&str> {
        if self.occurrences == 1 {
            self.values.first().map(String::as_str)
        } else {
            None
        }
    }
}

#[derive(Default)]
pub(super) struct Gathered {
    pub(super) verb: Option<Verb>,
    pub(super) repo: Slot,
    pub(super) object_format: Slot,
    pub(super) base: Slot,
    pub(super) candidate: Slot,
    pub(super) repository: Slot,
    pub(super) ref_name: Slot,
    pub(super) default_branch_ref: Slot,
    pub(super) forge: Slot,
    pub(super) profile: Slot,
    pub(super) format: Slot,
    pub(super) floor_digest: Slot,
    pub(super) debt_owner: Slot,
    pub(super) debt_reason: Slot,
    pub(super) created_at: Slot,
    pub(super) expires_at: Slot,
    pub(super) debt_output: Slot,
    pub(super) claim_path: Slot,
    pub(super) claim_line: Slot,
    pub(super) claim_name: Slot,
    pub(super) suffix: Slot,
    pub(super) adapter: Slot,
    pub(super) report: Slot,
    pub(super) plan: Slot,
    pub(super) evidence: Slot,
    pub(super) context: Slot,
    pub(super) semantic_template: Slot,
    pub(super) target: Slot,
    pub(super) target_bytes_hex: Slot,
    pub(super) index: usize,
    pub(super) explain_scope: usize,
    pub(super) full: usize,
    pub(super) refusals: BTreeSet<Refusal>,
}

pub(super) fn gather(argv: &[OsString]) -> Gathered {
    let mut gathered = Gathered::default();
    let mut tokens = argv.iter().map(|token| token.to_str()).peekable();
    match tokens.next() {
        None => {
            gathered
                .refusals
                .insert((Code::InvalidInvocation, "a verb must come first".to_owned()));
        }
        Some(None) => {
            gathered
                .refusals
                .insert((Code::InvalidInvocation, "the verb is not UTF-8".to_owned()));
        }
        Some(Some(token)) => match token.parse() {
            Ok(verb) => gathered.verb = Some(verb),
            Err(_unknown) => {
                let reason = if HELP_FLAGS.contains(&token) || VERSION_FLAGS.contains(&token) {
                    format!("{token} stands alone")
                } else {
                    format!("unknown verb {}", atom(token))
                };
                gathered.refusals.insert((Code::InvalidInvocation, reason));
            }
        },
    }

    while let Some(token) = tokens.next() {
        let Some(token) = token else {
            gathered.refusals.insert((
                Code::InvalidInvocation,
                "an argument is not UTF-8".to_owned(),
            ));
            continue;
        };
        if !token.starts_with("--") {
            gathered.refusals.insert((
                Code::InvalidInvocation,
                format!("unexpected argument {}", atom(token)),
            ));
            continue;
        }
        if token == "--index" {
            gathered.index = gathered.index.saturating_add(1);
            continue;
        }
        if token == "--explain-scope" {
            gathered.explain_scope = gathered.explain_scope.saturating_add(1);
            continue;
        }
        if token == "--full" {
            gathered.full = gathered.full.saturating_add(1);
            continue;
        }
        let Some(slot) = slot_for(&mut gathered, token) else {
            let reason = if token.contains('=') {
                format!("options take a separate value, not {}", atom(token))
            } else {
                format!("unknown option {}", atom(token))
            };
            gathered.refusals.insert((Code::InvalidInvocation, reason));
            continue;
        };
        let value = match tokens.peek() {
            Some(Some(next)) if !next.starts_with("--") => {
                let owned = (*next).to_owned();
                tokens.next();
                Some(owned)
            }
            Some(Some(_) | None) | None => None,
        };
        slot.record(value);
    }
    gathered
}

fn slot_for<'a>(gathered: &'a mut Gathered, option: &str) -> Option<&'a mut Slot> {
    match option {
        "--repo" => Some(&mut gathered.repo),
        "--object-format" => Some(&mut gathered.object_format),
        "--base" => Some(&mut gathered.base),
        "--candidate" => Some(&mut gathered.candidate),
        "--repository" => Some(&mut gathered.repository),
        "--ref" => Some(&mut gathered.ref_name),
        "--default-branch-ref" => Some(&mut gathered.default_branch_ref),
        "--forge" => Some(&mut gathered.forge),
        "--profile" => Some(&mut gathered.profile),
        "--format" => Some(&mut gathered.format),
        "--floor-digest" => Some(&mut gathered.floor_digest),
        "--debt-owner" => Some(&mut gathered.debt_owner),
        "--debt-reason" => Some(&mut gathered.debt_reason),
        "--created-at" => Some(&mut gathered.created_at),
        "--expires-at" => Some(&mut gathered.expires_at),
        "--debt-output" => Some(&mut gathered.debt_output),
        "--path" => Some(&mut gathered.claim_path),
        "--line" => Some(&mut gathered.claim_line),
        "--name" => Some(&mut gathered.claim_name),
        "--suffix" => Some(&mut gathered.suffix),
        "--adapter" => Some(&mut gathered.adapter),
        "--report" => Some(&mut gathered.report),
        "--plan" => Some(&mut gathered.plan),
        "--evidence" => Some(&mut gathered.evidence),
        "--context" => Some(&mut gathered.context),
        "--semantic-template" => Some(&mut gathered.semantic_template),
        "--target" => Some(&mut gathered.target),
        "--target-bytes-hex" => Some(&mut gathered.target_bytes_hex),
        _ => None,
    }
}

/// Every option's count beside its spelling, so a form can refuse what it
/// does not own; `--full` is judged once, before the verb is known.
pub(super) fn counts(gathered: &Gathered) -> [(usize, &'static str); 30] {
    [
        (gathered.repo.occurrences, "--repo"),
        (gathered.object_format.occurrences, "--object-format"),
        (gathered.base.occurrences, "--base"),
        (gathered.candidate.occurrences, "--candidate"),
        (gathered.repository.occurrences, "--repository"),
        (gathered.ref_name.occurrences, "--ref"),
        (
            gathered.default_branch_ref.occurrences,
            "--default-branch-ref",
        ),
        (gathered.forge.occurrences, "--forge"),
        (gathered.profile.occurrences, "--profile"),
        (gathered.format.occurrences, "--format"),
        (gathered.floor_digest.occurrences, "--floor-digest"),
        (gathered.debt_owner.occurrences, "--debt-owner"),
        (gathered.debt_reason.occurrences, "--debt-reason"),
        (gathered.created_at.occurrences, "--created-at"),
        (gathered.expires_at.occurrences, "--expires-at"),
        (gathered.debt_output.occurrences, "--debt-output"),
        (gathered.claim_path.occurrences, "--path"),
        (gathered.claim_line.occurrences, "--line"),
        (gathered.claim_name.occurrences, "--name"),
        (gathered.suffix.occurrences, "--suffix"),
        (gathered.adapter.occurrences, "--adapter"),
        (gathered.report.occurrences, "--report"),
        (gathered.plan.occurrences, "--plan"),
        (gathered.evidence.occurrences, "--evidence"),
        (gathered.context.occurrences, "--context"),
        (
            gathered.semantic_template.occurrences,
            "--semantic-template",
        ),
        (gathered.target.occurrences, "--target"),
        (gathered.target_bytes_hex.occurrences, "--target-bytes-hex"),
        (gathered.index, "--index"),
        (gathered.explain_scope, "--explain-scope"),
    ]
}

/// An option that may be absent; one that repeats or arrives without its
/// value is refused by name.
pub(super) fn optional<'a>(slot: &'a Slot, option: &str) -> Result<Option<&'a str>, Refusal> {
    match slot.occurrences {
        0 => Ok(None),
        1 => slot
            .unique_value()
            .map(Some)
            .ok_or_else(|| (Code::InvalidInvocation, format!("{option} needs a value"))),
        _ => Err((
            Code::InvalidInvocation,
            format!("{option} appears more than once"),
        )),
    }
}

pub(super) fn required<'a>(slot: &'a Slot, option: &str) -> Result<&'a str, Refusal> {
    optional(slot, option)?
        .ok_or_else(|| (Code::InvalidInvocation, format!("{option} is required")))
}

pub(super) fn output_selection(format: &Slot) -> Result<OutputFormat, String> {
    let Some(value) = optional(format, "--format").map_err(|(_code, reason)| reason)? else {
        return Ok(OutputFormat::Human);
    };
    value.parse().map_err(|_unknown| {
        let admitted: Vec<&'static str> = OutputFormat::iter().map(Into::into).collect();
        format!(
            "--format must be one of {}, got {}",
            admitted.join(", "),
            atom(value)
        )
    })
}
