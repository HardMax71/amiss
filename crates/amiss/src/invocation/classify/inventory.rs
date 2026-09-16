use std::collections::BTreeSet;
use std::path::PathBuf;

use super::super::arguments::{Gathered, required};
use super::super::{Code, Command, InventoryInvocation, OutputFormat, Refusal, Verb};
use super::{invalid, record, refuse_foreign};

/// The inventory form: one checkout, one plan, one locale layout. The plan
/// names the object format and the commit, so restating either is foreign,
/// as is every scan, authoring, adoption, and other pure-form option.
pub(super) fn classify_locale_inventory(
    mut refusals: BTreeSet<Refusal>,
    gathered: &Gathered,
    format: OutputFormat,
) -> Result<Command, BTreeSet<Refusal>> {
    refuse_foreign(
        &mut refusals,
        gathered,
        Verb::LocaleInventory,
        &["--repo", "--plan", "--context", "--format"],
    );
    if !matches!(format, OutputFormat::Human | OutputFormat::Json) {
        refusals.insert(invalid(format!(
            "--format {} is not admitted by locale-inventory, which takes human, json",
            format.as_ref()
        )));
    }
    let paths = [
        (&gathered.repo, "--repo"),
        (&gathered.plan, "--plan"),
        (&gathered.context, "--context"),
    ]
    .map(|(slot, option)| {
        record(
            &mut refusals,
            required(slot, option).and_then(|value| match value {
                "" => Err(invalid(format!("{option} must not be empty"))),
                path => Ok(PathBuf::from(path)),
            }),
        )
    });
    if !refusals.is_empty() {
        return Err(refusals);
    }
    let [Some(repo), Some(plan), Some(context)] = paths else {
        return Err(BTreeSet::from([invalid(
            Code::InvalidInvocation.meaning().to_owned(),
        )]));
    };
    Ok(Command::LocaleInventory(InventoryInvocation {
        repo,
        plan,
        context,
        format,
    }))
}
