use std::ffi::OsString;

use super::{OutputFormat, Verb};

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
    pub(super) semantic_template: Slot,
    pub(super) target: Slot,
    pub(super) target_bytes_hex: Slot,
    pub(super) index: usize,
    pub(super) explain_scope: usize,
    pub(super) full: usize,
    pub(super) lexical_defect: bool,
}

pub(super) fn gather(argv: &[OsString]) -> Gathered {
    let mut gathered = Gathered::default();
    let mut tokens = argv.iter().map(|token| token.to_str()).peekable();
    gathered.verb = tokens.next().flatten().and_then(|token| token.parse().ok());
    gathered.lexical_defect = gathered.verb.is_none();

    while let Some(token) = tokens.next() {
        let Some(token) = token else {
            gathered.lexical_defect = true;
            continue;
        };
        if !token.starts_with("--") {
            gathered.lexical_defect = true;
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
            gathered.lexical_defect = true;
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
        "--semantic-template" => Some(&mut gathered.semantic_template),
        "--target" => Some(&mut gathered.target),
        "--target-bytes-hex" => Some(&mut gathered.target_bytes_hex),
        _ => None,
    }
}

pub(super) fn output_selection(format: &Slot) -> Option<OutputFormat> {
    if format.occurrences > 0 {
        format.unique_value()?.parse().ok()
    } else {
        Some(OutputFormat::Human)
    }
}

pub(super) fn duplicated(gathered: &Gathered) -> bool {
    gathered.index > 1
        || gathered.explain_scope > 1
        || gathered.full > 1
        || [
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
            &gathered.report,
            &gathered.plan,
            &gathered.evidence,
            &gathered.semantic_template,
            &gathered.target,
            &gathered.target_bytes_hex,
        ]
        .iter()
        .any(|slot| slot.occurrences > 1 || slot.values.len() < slot.occurrences)
}
