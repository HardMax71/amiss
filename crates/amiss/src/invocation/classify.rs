use std::collections::BTreeSet;
use std::path::PathBuf;

use amiss_wire::controls::Profile;
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

use super::arguments::{Gathered, Slot, duplicated};
use super::{
    Adoption, CandidateSelector, Code, Command, Invocation, OutputFormat, ProviderIdentity, Verb,
};

mod authoring;
mod report;

use authoring::{classify_claim, classify_policy_include};
use report::classify_report_command;

pub(super) fn command(
    gathered: &Gathered,
    format: OutputFormat,
) -> Result<Command, BTreeSet<Code>> {
    let mut codes = BTreeSet::new();
    if gathered.lexical_defect || duplicated(gathered) {
        codes.insert(Code::InvalidInvocation);
    }
    if gathered.full > 0 && (gathered.verb != Some(Verb::Render) || format != OutputFormat::Human) {
        codes.insert(Code::InvalidInvocation);
    }
    match gathered.verb {
        Some(Verb::Claim) => return classify_claim(codes, gathered).map(Command::Author),
        Some(Verb::PolicyInclude) => {
            return classify_policy_include(codes, gathered).map(Command::PolicyInclude);
        }
        Some(Verb::ExternalPlan | Verb::ExternalAssess | Verb::Render | Verb::Refs) => {
            return classify_report_command(codes, gathered, format);
        }
        Some(Verb::Check | Verb::Fix | Verb::Adopt) | None => {}
    }
    if format == OutputFormat::Junit {
        codes.insert(Code::InvalidInvocation);
    }
    for required in [&gathered.repo, &gathered.object_format, &gathered.base] {
        if required.occurrences == 0 {
            codes.insert(Code::InvalidInvocation);
        }
    }
    verb_rules(&mut codes, gathered);
    if (gathered.candidate.occurrences > 0) == (gathered.index > 0) {
        codes.insert(Code::InvalidInvocation);
    }

    let target = record(&mut codes, classify_target(gathered));
    let object_format = target.as_ref().ok().map(|(_, format)| *format);
    let base = record(&mut codes, decode_oid(object_format, &gathered.base));
    let candidate_oid = record(&mut codes, decode_oid(object_format, &gathered.candidate));
    if let (Ok(Some(base)), Ok(Some(candidate))) = (&base, &candidate_oid)
        && base == candidate
    {
        codes.insert(Code::InvalidInvocation);
    }

    let profile = record(&mut codes, classify_profile(gathered));
    let adoption = record(&mut codes, classify_adoption(gathered));
    let identity = record(&mut codes, classify_identity(gathered));
    let forge = record(&mut codes, classify_forge(gathered, &identity));

    if !codes.is_empty() {
        return Err(codes);
    }
    let (
        Some(verb),
        Ok((repo, object_format)),
        Ok(Some(base)),
        Ok(candidate_oid),
        Ok(profile),
        Ok(adoption),
        Ok(identity),
        Ok(forge),
    ) = (
        gathered.verb,
        target,
        base,
        candidate_oid,
        profile,
        adoption,
        identity,
        forge,
    )
    else {
        return Err(BTreeSet::from([Code::InvalidInvocation]));
    };
    let candidate = candidate_oid.map_or(CandidateSelector::Index, CandidateSelector::Commit);
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
    })))
}

fn classify_target(gathered: &Gathered) -> Validation<(PathBuf, ObjectFormat)> {
    let repo = gathered
        .repo
        .unique_value()
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .ok_or(Code::InvalidInvocation)?;
    let object_format = gathered
        .object_format
        .unique_value()
        .ok_or(Code::InvalidInvocation)?
        .parse()
        .map_err(|_unknown| Code::InvalidInvocation)?;
    Ok((repo, object_format))
}

fn classify_profile(gathered: &Gathered) -> Validation<Profile> {
    if gathered.verb == Some(Verb::Adopt) {
        return Ok(Profile::Enforce);
    }
    gathered
        .profile
        .unique_value()
        .ok_or(Code::InvalidInvocation)?
        .parse()
        .map_err(|_unknown| Code::InvalidProfile)
}

/// The per-verb shape: adoption bakes enforce so the profile is refused
/// there and required elsewhere, the six adoption values are required there
/// and refused elsewhere, the repair form is staged-only without report
/// flags, and the adoption form is commit-pair only without them.
fn verb_rules(codes: &mut BTreeSet<Code>, gathered: &Gathered) {
    if [
        &gathered.report,
        &gathered.plan,
        &gathered.evidence,
        &gathered.target,
        &gathered.target_bytes_hex,
    ]
    .iter()
    .any(|slot| slot.occurrences > 0)
    {
        codes.insert(Code::InvalidInvocation);
    }
    if [
        &gathered.claim_path,
        &gathered.claim_line,
        &gathered.claim_name,
        &gathered.suffix,
        &gathered.adapter,
    ]
    .iter()
    .any(|slot| slot.occurrences > 0)
    {
        codes.insert(Code::InvalidInvocation);
    }
    if gathered.verb == Some(Verb::Adopt) {
        if gathered.profile.occurrences > 0 {
            codes.insert(Code::InvalidInvocation);
        }
    } else if gathered.profile.occurrences == 0 {
        codes.insert(Code::InvalidInvocation);
    }
    let adoption_slots = [
        &gathered.floor_digest,
        &gathered.debt_owner,
        &gathered.debt_reason,
        &gathered.created_at,
        &gathered.expires_at,
        &gathered.debt_output,
    ];
    if gathered.verb == Some(Verb::Adopt) {
        if adoption_slots.iter().any(|slot| slot.occurrences == 0) {
            codes.insert(Code::InvalidInvocation);
        }
    } else if adoption_slots.iter().any(|slot| slot.occurrences > 0) {
        codes.insert(Code::InvalidInvocation);
    }
    if gathered.verb == Some(Verb::Fix)
        && (gathered.candidate.occurrences > 0
            || gathered.format.occurrences > 0
            || gathered.explain_scope > 0)
    {
        codes.insert(Code::InvalidInvocation);
    }
    if gathered.verb == Some(Verb::Adopt)
        && (gathered.index > 0
            || gathered.format.occurrences > 0
            || gathered.explain_scope > 0
            || gathered.repository.occurrences == 0)
    {
        codes.insert(Code::InvalidInvocation);
    }
}

/// Every adoption value is validated where the grammar can see it: the
/// floor digest by its exact spelling, both instants by the wire's own
/// clock grammar, and the free-text fields by being nonempty.
fn classify_adoption(gathered: &Gathered) -> Validation<Option<Adoption>> {
    if gathered.verb != Some(Verb::Adopt) {
        return Ok(None);
    }
    let floor_digest = gathered
        .floor_digest
        .unique_value()
        .filter(|value| {
            value.strip_prefix("sha256:").is_some_and(|hex| {
                hex.len() == 64
                    && hex
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            })
        })
        .map(str::to_owned)
        .ok_or(Code::InvalidInvocation)?;
    let instant = |slot: &Slot| {
        slot.unique_value()
            .filter(|value| amiss_wire::model::UtcInstant::new((*value).to_owned()).is_some())
            .map(str::to_owned)
    };
    let nonempty = |slot: &Slot| {
        slot.unique_value()
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let owner = nonempty(&gathered.debt_owner).ok_or(Code::InvalidInvocation)?;
    let reason = nonempty(&gathered.debt_reason).ok_or(Code::InvalidInvocation)?;
    let created_at = instant(&gathered.created_at).ok_or(Code::InvalidInvocation)?;
    let expires_at = instant(&gathered.expires_at).ok_or(Code::InvalidInvocation)?;
    let output = nonempty(&gathered.debt_output).ok_or(Code::InvalidInvocation)?;
    if created_at >= expires_at {
        return Err(Code::InvalidInvocation);
    }
    Ok(Some(Adoption {
        floor_digest,
        owner,
        reason,
        created_at,
        expires_at,
        output: PathBuf::from(output),
    }))
}

/// The dialect law: an explicit `--forge` names a grammar the engine knows
/// and accompanies the identity triple; without the flag the known-host
/// table decides, and an identity on a host outside the table is refused,
/// since accepting it would silently leave every same-repository URL
/// external. The github dialect cannot match a nested owner, so that
/// pairing is refused rather than left deterministically dead.
fn classify_forge(
    gathered: &Gathered,
    identity: &Validation<Option<ProviderIdentity>>,
) -> Validation<Option<ForgeDialect>> {
    let declared = if gathered.forge.occurrences > 0 {
        Some(
            gathered
                .forge
                .unique_value()
                .ok_or(Code::InvalidInvocation)?
                .parse::<ForgeDialect>()
                .map_err(|_unknown| Code::InvalidInvocation)?,
        )
    } else {
        None
    };
    let Some(identity) = identity.as_ref().map_err(|code| *code)? else {
        return if declared.is_some() {
            Err(Code::InvalidInvocation)
        } else {
            Ok(None)
        };
    };
    let dialect = declared
        .or_else(|| ForgeDialect::default_for_host(identity.repository.host()))
        .ok_or(Code::InvalidEvent)?;
    if matches!(
        dialect,
        ForgeDialect::Github
            | ForgeDialect::Gitea
            | ForgeDialect::BitbucketCloud
            | ForgeDialect::BitbucketDataCenter
    ) && identity.repository.owner().contains('/')
    {
        Err(Code::InvalidEvent)
    } else {
        Ok(Some(dialect))
    }
}

fn decode_oid(object_format: Option<ObjectFormat>, slot: &Slot) -> Validation<Option<Oid>> {
    let (Some(format), Some(raw)) = (object_format, slot.unique_value()) else {
        return Ok(None);
    };
    Oid::new(format, raw.to_owned())
        .map(Some)
        .ok_or(Code::InvalidInvocation)
}

type Validation<T> = Result<T, Code>;

fn record<T>(codes: &mut BTreeSet<Code>, validation: Validation<T>) -> Validation<T> {
    validation.inspect_err(|code| {
        codes.insert(*code);
    })
}

fn classify_identity(gathered: &Gathered) -> Validation<Option<ProviderIdentity>> {
    let present = [
        gathered.repository.occurrences > 0,
        gathered.ref_name.occurrences > 0,
        gathered.default_branch_ref.occurrences > 0,
    ];
    if present == [false, false, false] {
        return Ok(None);
    }
    if present != [true, true, true] {
        return Err(Code::InvalidInvocation);
    }
    let repository = gathered
        .repository
        .unique_value()
        .ok_or(Code::InvalidInvocation)?;
    let ref_value = gathered
        .ref_name
        .unique_value()
        .ok_or(Code::InvalidInvocation)?;
    let default_value = gathered
        .default_branch_ref
        .unique_value()
        .ok_or(Code::InvalidInvocation)?;
    let (host, owner_and_name) = repository.split_once('/').ok_or(Code::InvalidInvocation)?;
    let (owner, name) = owner_and_name
        .rsplit_once('/')
        .ok_or(Code::InvalidInvocation)?;

    let identity = RepositoryIdentity::new(host.to_owned(), owner.to_owned(), name.to_owned());
    let ref_name = BranchRef::new(ref_value.to_owned());
    let default_branch_ref = BranchRef::new(default_value.to_owned());
    if let (Some(repository), Some(ref_name), Some(default_branch_ref)) =
        (identity, ref_name, default_branch_ref)
    {
        Ok(Some(ProviderIdentity {
            repository,
            ref_name,
            default_branch_ref,
        }))
    } else {
        Err(Code::InvalidEvent)
    }
}
