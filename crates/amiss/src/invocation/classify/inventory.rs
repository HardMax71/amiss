use std::collections::BTreeSet;
use std::path::PathBuf;

use super::super::arguments::{Gathered, optional, required};
use amiss_wire::model::ArtifactId;

use super::super::{
    AnalysisErrorCode, Command, InventoryInvocation, LocalePlanInvocation, OutputFormat, Refusal,
    Verb,
};
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
            AnalysisErrorCode::InvalidInvocation.meaning().to_owned(),
        )]));
    };
    Ok(Command::LocaleInventory(InventoryInvocation {
        repo,
        plan,
        context,
        format,
    }))
}

/// The locale plan form: the report whose candidate the audit binds, the
/// locale layout whose producer it accepts, and the scope and policy the
/// operator names. The locales come from the layout, so naming them again is
/// foreign.
pub(super) fn classify_locale_plan(
    mut refusals: BTreeSet<Refusal>,
    gathered: &Gathered,
    format: OutputFormat,
) -> Result<Command, BTreeSet<Refusal>> {
    refuse_foreign(
        &mut refusals,
        gathered,
        Verb::LocalePlan,
        &[
            "--report",
            "--context",
            "--site",
            "--channel",
            "--scope-version",
            "--fallback",
            "--require-lineage",
            "--format",
        ],
    );
    if !matches!(format, OutputFormat::Human | OutputFormat::Json) {
        refusals.insert(invalid(format!(
            "--format {} is not admitted by locale-plan, which takes human, json",
            format.as_ref()
        )));
    }
    let [report, context] = [
        (&gathered.report, "--report"),
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
    let identity = |value: &str, option: &str| {
        ArtifactId::try_from(value.to_owned())
            .map_err(|_defect| invalid(format!("{option} {value} is not an artifact identity")))
    };
    let [site, channel] =
        [(&gathered.site, "--site"), (&gathered.channel, "--channel")].map(|(slot, option)| {
            record(
                &mut refusals,
                required(slot, option).and_then(|value| identity(value, option)),
            )
        });
    let fallback = record(
        &mut refusals,
        optional(&gathered.fallback, "--fallback")
            .and_then(|value| value.map(|value| identity(value, "--fallback")).transpose()),
    );
    let version = record(
        &mut refusals,
        optional(&gathered.scope_version, "--scope-version").map(|value| value.map(str::to_owned)),
    );
    if !refusals.is_empty() {
        return Err(refusals);
    }
    let (Some(report), Some(context), Some(site), Some(channel), Some(fallback), Some(version)) =
        (report, context, site, channel, fallback, version)
    else {
        return Err(BTreeSet::from([invalid(
            AnalysisErrorCode::InvalidInvocation.meaning().to_owned(),
        )]));
    };
    Ok(Command::LocalePlan(LocalePlanInvocation {
        report,
        context,
        site,
        channel,
        version,
        fallback,
        require_lineage: gathered.require_lineage == 1,
        format,
    }))
}
