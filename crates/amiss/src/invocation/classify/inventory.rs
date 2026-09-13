use std::collections::BTreeSet;
use std::path::PathBuf;

use super::super::arguments::Gathered;
use super::super::{Code, Command, InventoryInvocation, OutputFormat};

/// The inventory form: one checkout, one plan, one locale layout. The plan
/// names the object format and the commit, so restating either is foreign,
/// as is every scan, authoring, adoption, and other pure-form option.
pub(super) fn classify_locale_inventory(
    mut codes: BTreeSet<Code>,
    gathered: &Gathered,
    format: OutputFormat,
) -> Result<Command, BTreeSet<Code>> {
    let foreign = [
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
        &gathered.evidence,
        &gathered.semantic_template,
        &gathered.target,
        &gathered.target_bytes_hex,
    ];
    if foreign.iter().any(|slot| slot.occurrences > 0)
        || gathered.index > 0
        || gathered.explain_scope > 0
        || !matches!(format, OutputFormat::Human | OutputFormat::Json)
    {
        codes.insert(Code::InvalidInvocation);
    }
    let paths = [&gathered.repo, &gathered.plan, &gathered.context].map(|slot| {
        slot.unique_value()
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    });
    if paths.iter().any(Option::is_none) {
        codes.insert(Code::InvalidInvocation);
    }
    let [Some(repo), Some(plan), Some(context)] = paths else {
        codes.insert(Code::InvalidInvocation);
        return Err(codes);
    };
    if !codes.is_empty() {
        return Err(codes);
    }
    Ok(Command::LocaleInventory(InventoryInvocation {
        repo,
        plan,
        context,
        format,
    }))
}
