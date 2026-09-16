use std::collections::BTreeSet;
use std::path::PathBuf;

use amiss_wire::controls::Profile;
use amiss_wire::human::atom;
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};
use strum::IntoEnumIterator as _;

use super::arguments::{Gathered, Slot, counts, optional, required};
use super::{
    Adoption, CandidateSelector, Code, Command, Invocation, OutputFormat, ProviderIdentity,
    Refusal, Verb,
};

mod authoring;
mod inventory;
mod report;

use authoring::{classify_claim, classify_policy_include};
use inventory::classify_locale_inventory;
use report::classify_report_command;

type Validation<T> = Result<T, Refusal>;

fn invalid(reason: String) -> Refusal {
    (Code::InvalidInvocation, reason)
}

fn record<T>(refusals: &mut BTreeSet<Refusal>, validation: Validation<T>) -> Option<T> {
    validation
        .map_err(|refusal| {
            refusals.insert(refusal);
        })
        .ok()
}

/// Refuses every option the form does not own, by name.
fn refuse_foreign(
    refusals: &mut BTreeSet<Refusal>,
    gathered: &Gathered,
    verb: Verb,
    owned: &[&str],
) {
    for (count, option) in counts(gathered) {
        if count > 0 && !owned.contains(&option) {
            refusals.insert(invalid(format!(
                "{option} is not an option of {}",
                verb.as_ref()
            )));
        }
    }
}

/// The refusals a line earns before its verb is known.
fn lexical(gathered: &Gathered, format: OutputFormat) -> BTreeSet<Refusal> {
    let mut refusals = gathered.refusals.clone();
    for (count, flag) in [
        (gathered.index, "--index"),
        (gathered.explain_scope, "--explain-scope"),
        (gathered.full, "--full"),
    ] {
        if count > 1 {
            refusals.insert(invalid(format!("{flag} appears more than once")));
        }
    }
    if gathered.full > 0 && (gathered.verb != Some(Verb::Render) || format != OutputFormat::Human) {
        refusals.insert(invalid(
            "--full is only for render --format human".to_owned(),
        ));
    }
    refusals
}

pub(super) fn command(
    gathered: &Gathered,
    format: OutputFormat,
) -> Result<Command, BTreeSet<Refusal>> {
    let mut refusals = lexical(gathered, format);
    let Some(verb) = gathered.verb else {
        return Err(refusals);
    };
    match verb {
        Verb::Claim => return classify_claim(refusals, gathered).map(Command::Author),
        Verb::LocaleInventory => {
            return classify_locale_inventory(refusals, gathered, format);
        }
        Verb::PolicyInclude => {
            return classify_policy_include(refusals, gathered).map(Command::PolicyInclude);
        }
        Verb::ExternalPlan
        | Verb::ExternalAssess
        | Verb::LocaleAssess
        | Verb::Render
        | Verb::Refs
        | Verb::RecordSet => {
            return classify_report_command(refusals, gathered, verb, format);
        }
        Verb::Check | Verb::Fix | Verb::Adopt => {}
    }
    verb_rules(&mut refusals, gathered, verb, format);

    let repo = record(&mut refusals, classify_repo(gathered));
    let object_format = record(&mut refusals, classify_object_format(gathered));
    let base = record(
        &mut refusals,
        required(&gathered.base, "--base").and_then(|raw| decode_oid(object_format, "--base", raw)),
    );
    let candidate = record(
        &mut refusals,
        optional(&gathered.candidate, "--candidate").and_then(|raw| {
            raw.map(|raw| decode_oid(object_format, "--candidate", raw))
                .transpose()
                .map(Option::flatten)
        }),
    );
    if let (Some(Some(base)), Some(Some(candidate))) = (&base, &candidate)
        && base == candidate
    {
        refusals.insert(invalid(
            "--candidate and --base name the same commit".to_owned(),
        ));
    }

    let profile = record(&mut refusals, classify_profile(gathered, verb));
    let adoption = classify_adoption(&mut refusals, gathered, verb);
    let identity = classify_identity(&mut refusals, gathered);
    let forge = record(&mut refusals, classify_forge(gathered, identity.as_ref()));
    let semantic_template = record(&mut refusals, classify_semantic_template(gathered));

    if !refusals.is_empty() {
        return Err(refusals);
    }
    let (
        Some(repo),
        Some(object_format),
        Some(Some(base)),
        Some(candidate),
        Some(profile),
        Some(forge),
        Some(semantic_template),
    ) = (
        repo,
        object_format,
        base,
        candidate,
        profile,
        forge,
        semantic_template,
    )
    else {
        return Err(BTreeSet::from([invalid(
            Code::InvalidInvocation.meaning().to_owned(),
        )]));
    };
    let candidate = candidate.map_or(CandidateSelector::Index, CandidateSelector::Commit);
    Ok(Command::Scan(Box::new(Invocation {
        verb,
        adoption,
        repo,
        object_format,
        base,
        candidate,
        identity,
        forge,
        profile,
        explain_scope: gathered.explain_scope == 1,
        format,
        semantic_template,
    })))
}

fn classify_semantic_template(gathered: &Gathered) -> Validation<Option<PathBuf>> {
    match optional(&gathered.semantic_template, "--semantic-template")? {
        None => Ok(None),
        Some("") => Err(invalid("--semantic-template must not be empty".to_owned())),
        Some(path) => Ok(Some(PathBuf::from(path))),
    }
}

fn classify_repo(gathered: &Gathered) -> Validation<PathBuf> {
    match required(&gathered.repo, "--repo")? {
        "" => Err(invalid("--repo must not be empty".to_owned())),
        path => Ok(PathBuf::from(path)),
    }
}

fn classify_object_format(gathered: &Gathered) -> Validation<ObjectFormat> {
    let value = required(&gathered.object_format, "--object-format")?;
    value.parse().map_err(|_unknown| {
        invalid(format!(
            "--object-format must be sha1 or sha256, got {}",
            atom(value)
        ))
    })
}

fn classify_profile(gathered: &Gathered, verb: Verb) -> Validation<Profile> {
    if verb == Verb::Adopt {
        return Ok(Profile::Enforce);
    }
    let value = required(&gathered.profile, "--profile")?;
    value.parse().map_err(|_unknown| {
        (
            Code::InvalidProfile,
            format!(
                "--profile must be observe, enforce-introduced, or enforce, got {}",
                atom(value)
            ),
        )
    })
}

/// The per-verb shape: adoption bakes enforce and needs the identity
/// triple, the repair form is staged-only, and every option a verb does not
/// own is refused by name.
fn verb_rules(
    refusals: &mut BTreeSet<Refusal>,
    gathered: &Gathered,
    verb: Verb,
    format: OutputFormat,
) {
    if format == OutputFormat::Junit {
        refusals.insert(invalid("--format junit is only for render".to_owned()));
    }
    match (gathered.candidate.occurrences > 0, gathered.index > 0) {
        (true, true) => {
            refusals.insert(invalid("--candidate and --index are exclusive".to_owned()));
        }
        (false, false) => {
            refusals.insert(invalid(
                "one of --candidate or --index is required".to_owned(),
            ));
        }
        (true, false) | (false, true) => {}
    }
    let mut owned = vec![
        "--repo",
        "--object-format",
        "--base",
        "--repository",
        "--ref",
        "--default-branch-ref",
        "--forge",
    ];
    if verb == Verb::Adopt {
        owned.extend([
            "--candidate",
            "--floor-digest",
            "--debt-owner",
            "--debt-reason",
            "--created-at",
            "--expires-at",
            "--debt-output",
        ]);
        if gathered.repository.occurrences == 0 {
            refusals.insert(invalid(
                "adopt needs --repository, --ref, and --default-branch-ref".to_owned(),
            ));
        }
    } else {
        owned.extend(["--index", "--profile"]);
    }
    if verb == Verb::Check {
        owned.extend([
            "--candidate",
            "--semantic-template",
            "--explain-scope",
            "--format",
        ]);
    }
    refuse_foreign(refusals, gathered, verb, &owned);
}

/// Every adoption value is validated where the grammar can see it: the
/// floor digest by its exact spelling, both instants by the wire's own
/// clock grammar, and the free-text fields by being nonempty.
fn classify_adoption(
    refusals: &mut BTreeSet<Refusal>,
    gathered: &Gathered,
    verb: Verb,
) -> Option<Adoption> {
    if verb != Verb::Adopt {
        return None;
    }
    let instant = |slot: &Slot, option: &str| {
        required(slot, option).and_then(|value| {
            amiss_wire::model::UtcInstant::new(value.to_owned()).ok_or_else(|| {
                invalid(format!(
                    "{option} must be a UTC instant like 2026-01-31T00:00:00Z, got {}",
                    atom(value)
                ))
            })
        })
    };
    let nonempty = |slot: &Slot, option: &str| match required(slot, option)? {
        "" => Err(invalid(format!("{option} must not be empty"))),
        value => Ok(value.to_owned()),
    };
    let floor_digest = record(
        refusals,
        required(&gathered.floor_digest, "--floor-digest").and_then(|value| {
            value.parse().map_err(|_unknown| {
                invalid(format!(
                    "--floor-digest must be sha256:<64-hex>, got {}",
                    atom(value)
                ))
            })
        }),
    );
    let owner = record(
        refusals,
        required(&gathered.debt_owner, "--debt-owner").and_then(|value| {
            amiss_wire::model::OwnerId::new(value.to_owned()).ok_or_else(|| {
                invalid(format!(
                    "--debt-owner must be team:<name>, service:<name>, or user:<name>, got {}",
                    atom(value)
                ))
            })
        }),
    );
    let reason = record(refusals, nonempty(&gathered.debt_reason, "--debt-reason"));
    let created_at = record(refusals, instant(&gathered.created_at, "--created-at"));
    let expires_at = record(refusals, instant(&gathered.expires_at, "--expires-at"));
    let output = record(refusals, nonempty(&gathered.debt_output, "--debt-output"));
    if let (Some(created), Some(expires)) = (&created_at, &expires_at)
        && created >= expires
    {
        refusals.insert(invalid(
            "--expires-at must be after --created-at".to_owned(),
        ));
        return None;
    }
    Some(Adoption {
        floor_digest: floor_digest?,
        owner: owner?,
        reason: reason?,
        created_at: created_at?,
        expires_at: expires_at?,
        output: PathBuf::from(output?),
    })
}

/// The dialect law: an explicit `--forge` names a grammar the engine knows
/// and accompanies the identity triple; without the flag the known-host
/// table decides, and an identity on a host outside the table is refused,
/// since accepting it would silently leave every same-repository URL
/// external. The github dialect cannot match a nested owner, so that
/// pairing is refused rather than left deterministically dead.
fn classify_forge(
    gathered: &Gathered,
    identity: Option<&ProviderIdentity>,
) -> Validation<Option<ForgeDialect>> {
    let declared = match optional(&gathered.forge, "--forge")? {
        Some(value) => Some(value.parse::<ForgeDialect>().map_err(|_unknown| {
            let dialects: Vec<&'static str> = ForgeDialect::iter().map(Into::into).collect();
            invalid(format!(
                "--forge must be one of {}, got {}",
                dialects.join(", "),
                atom(value)
            ))
        })?),
        None => None,
    };
    let Some(identity) = identity else {
        if declared.is_some() && gathered.repository.occurrences == 0 {
            return Err(invalid(
                "--forge needs --repository, --ref, and --default-branch-ref".to_owned(),
            ));
        }
        return Ok(None);
    };
    let host = identity.repository.host();
    let dialect = declared
        .or_else(|| ForgeDialect::default_for_host(host))
        .ok_or_else(|| {
            (
                Code::InvalidEvent,
                format!(
                    "--forge must name the dialect of {}, a host outside the known table",
                    atom(host)
                ),
            )
        })?;
    if matches!(
        dialect,
        ForgeDialect::Github
            | ForgeDialect::Gitea
            | ForgeDialect::BitbucketCloud
            | ForgeDialect::BitbucketDataCenter
    ) && identity.repository.owner().contains('/')
    {
        Err((
            Code::InvalidEvent,
            format!(
                "--forge {} cannot match the nested owner {}",
                dialect.as_ref(),
                atom(identity.repository.owner())
            ),
        ))
    } else {
        Ok(Some(dialect))
    }
}

/// A commit id is checked only against a declared object format; without
/// one the format's own refusal already stands.
fn decode_oid(
    object_format: Option<ObjectFormat>,
    option: &str,
    raw: &str,
) -> Validation<Option<Oid>> {
    let Some(format) = object_format else {
        return Ok(None);
    };
    Oid::new(format, raw.to_owned()).map(Some).ok_or_else(|| {
        let digits = match format {
            ObjectFormat::Sha1 => 40,
            ObjectFormat::Sha256 => 64,
        };
        invalid(format!(
            "{option} must be the full {digits}-character lowercase hex id of a {format} commit, got {}",
            atom(raw)
        ))
    })
}

/// The identity triple travels together; each member is then validated by
/// name, the shape as a grammar refusal and the spelling as an event one.
fn classify_identity(
    refusals: &mut BTreeSet<Refusal>,
    gathered: &Gathered,
) -> Option<ProviderIdentity> {
    let group = [
        (&gathered.repository, "--repository"),
        (&gathered.ref_name, "--ref"),
        (&gathered.default_branch_ref, "--default-branch-ref"),
    ];
    let present: Vec<&str> = group
        .iter()
        .filter(|(slot, _option)| slot.occurrences > 0)
        .map(|(_slot, option)| *option)
        .collect();
    let missing: Vec<&str> = group
        .iter()
        .filter(|(slot, _option)| slot.occurrences == 0)
        .map(|(_slot, option)| *option)
        .collect();
    if present.is_empty() {
        return None;
    }
    if !missing.is_empty() {
        refusals.insert(invalid(format!(
            "{} needs {}",
            present.join(" and "),
            missing.join(" and ")
        )));
        return None;
    }
    let repository = record(
        refusals,
        required(&gathered.repository, "--repository").and_then(identity_of),
    );
    let ref_name = record(refusals, branch_of(&gathered.ref_name, "--ref"));
    let default_branch_ref = record(
        refusals,
        branch_of(&gathered.default_branch_ref, "--default-branch-ref"),
    );
    Some(ProviderIdentity {
        repository: repository?,
        ref_name: ref_name?,
        default_branch_ref: default_branch_ref?,
    })
}

fn identity_of(value: &str) -> Validation<RepositoryIdentity> {
    let shape = || {
        invalid(format!(
            "--repository must be <host>/<owner>/<name>, got {}",
            atom(value)
        ))
    };
    let (host, owner_and_name) = value.split_once('/').ok_or_else(shape)?;
    let (owner, name) = owner_and_name.rsplit_once('/').ok_or_else(shape)?;
    RepositoryIdentity::new(host.to_owned(), owner.to_owned(), name.to_owned()).ok_or_else(|| {
        (
            Code::InvalidEvent,
            format!(
                "--repository must be <host>/<owner>/<name> with a lowercase owner and name, got {}",
                atom(value)
            ),
        )
    })
}

fn branch_of(slot: &Slot, option: &str) -> Validation<BranchRef> {
    let value = required(slot, option)?;
    BranchRef::try_from(value.to_owned()).map_err(|_invalid| {
        (
            Code::InvalidEvent,
            format!("{option} must be refs/heads/<name>, got {}", atom(value)),
        )
    })
}
