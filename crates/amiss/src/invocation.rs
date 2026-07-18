use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::PathBuf;

use amiss_wire::controls::Profile;
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};

pub const MALFORMED_OUTPUT_LINE: &str = "amiss: invalid invocation\n";

/// The closed grammar, verbatim. A rejected human invocation prints it after
/// the code lines, because there is no `--help` and the caller may hold
/// neither the book nor a network; the documentation contract test keeps the
/// invocation chapter's copy equal to this one.
pub const GRAMMAR: &str = "amiss check --repo <path> --object-format <sha1|sha256>
            --base <full-oid> (--candidate <full-oid> | --index)
            [--repository <host>/<owner>/<name>
             --ref refs/heads/<name>
             --default-branch-ref refs/heads/<name>
             [--forge <github|gitlab|gitea>]]
            --profile <observe|enforce>
            [--explain-scope] [--format <human|json>]";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Code {
    InvalidEvent,
    InvalidInvocation,
    InvalidProfile,
}

impl Code {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidEvent => "INVALID_EVENT",
            Self::InvalidInvocation => "INVALID_INVOCATION",
            Self::InvalidProfile => "INVALID_PROFILE",
        }
    }

    /// The contract the code enforced, in one line, for the human projection
    /// only. The report envelope carries the code and no prose, on purpose, and
    /// human output is a non-wire projection whose wording is free to say more.
    /// The command has no `--help`, so a refusal is the only place the closed
    /// grammar can explain itself to the person who just tripped over it.
    #[must_use]
    pub const fn contract(self) -> &'static str {
        match self {
            Self::InvalidEvent => {
                "--repository is host/owner/name: the host is any spelling without a slash, \
                 matched byte for byte wherever it appears, so give the lowercase form your \
                 links use; owner segments and the name are canonical ASCII lowercase, and \
                 owners nest as group/subgroup on GitLab only. --ref and --default-branch-ref \
                 are full refs such as refs/heads/main. Forges report the owner with its \
                 original capitals, so a workflow passing ${{ github.repository }} has to \
                 lowercase it first."
            }
            Self::InvalidInvocation => {
                "every option is spelled exactly, appears at most once, and carries a value. \
                 --base and --candidate are distinct full lowercase object IDs, never refs and \
                 never abbreviations. --forge is github, gitlab, or gitea, names a dialect the \
                 engine knows, and accompanies the --repository triple."
            }
            Self::InvalidProfile => "--profile is observe or enforce.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Human,
    Json,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateSelector {
    Commit(Oid),
    Index,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderIdentity {
    pub repository: RepositoryIdentity,
    pub ref_name: BranchRef,
    pub default_branch_ref: BranchRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    pub repo: PathBuf,
    pub object_format: ObjectFormat,
    pub base: Oid,
    pub candidate: CandidateSelector,
    pub identity: Option<ProviderIdentity>,
    pub forge: Option<ForgeDialect>,
    pub profile: Profile,
    pub explain_scope: bool,
    pub format: OutputFormat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Output selection itself is invalid: empty stdout, one fixed stderr
    /// line, exit 2, and no envelope may be chosen by conflicting values.
    MalformedOutputSelection,
    Rejected {
        format: OutputFormat,
        codes: BTreeSet<Code>,
    },
    Accepted(Invocation),
}

#[derive(Default)]
struct Slot {
    occurrences: usize,
    values: Vec<String>,
}

impl Slot {
    fn record(&mut self, value: Option<String>) {
        self.occurrences = self.occurrences.saturating_add(1);
        if let Some(value) = value {
            self.values.push(value);
        }
    }

    fn unique_value(&self) -> Option<&str> {
        if self.occurrences == 1 {
            self.values.first().map(String::as_str)
        } else {
            None
        }
    }

    fn defective(&self) -> bool {
        self.occurrences > 1 || self.values.len() < self.occurrences
    }

    fn present(&self) -> bool {
        self.occurrences > 0
    }
}

#[derive(Default)]
struct Gathered {
    repo: Slot,
    object_format: Slot,
    base: Slot,
    candidate: Slot,
    repository: Slot,
    ref_name: Slot,
    default_branch_ref: Slot,
    forge: Slot,
    profile: Slot,
    format: Slot,
    index: usize,
    explain_scope: usize,
    lexical_defect: bool,
}

#[must_use]
pub fn parse(argv: &[OsString]) -> Outcome {
    let gathered = gather(argv);
    let Some(format) = output_selection(&gathered.format) else {
        return Outcome::MalformedOutputSelection;
    };
    match classify(&gathered, format) {
        Ok(invocation) => Outcome::Accepted(invocation),
        Err(codes) => Outcome::Rejected { format, codes },
    }
}

fn gather(argv: &[OsString]) -> Gathered {
    let mut gathered = Gathered::default();
    let mut tokens = argv.iter().map(|token| token.to_str()).peekable();

    match tokens.next() {
        Some(Some("check")) => {}
        Some(Some(_) | None) | None => gathered.lexical_defect = true,
    }

    while let Some(token) = tokens.next() {
        let Some(token) = token else {
            gathered.lexical_defect = true;
            continue;
        };
        if !token.starts_with("--") {
            gathered.lexical_defect = true;
            continue;
        }
        match token {
            "--index" => gathered.index = gathered.index.saturating_add(1),
            "--explain-scope" => {
                gathered.explain_scope = gathered.explain_scope.saturating_add(1);
            }
            "--repo"
            | "--object-format"
            | "--base"
            | "--candidate"
            | "--repository"
            | "--ref"
            | "--default-branch-ref"
            | "--forge"
            | "--profile"
            | "--format" => {
                let value = match tokens.peek() {
                    Some(Some(next)) if !next.starts_with("--") => {
                        let owned = (*next).to_owned();
                        tokens.next();
                        Some(owned)
                    }
                    Some(Some(_) | None) | None => None,
                };
                slot_for(&mut gathered, token).record(value);
            }
            _ => gathered.lexical_defect = true,
        }
    }
    gathered
}

fn slot_for<'a>(gathered: &'a mut Gathered, option: &str) -> &'a mut Slot {
    match option {
        "--repo" => &mut gathered.repo,
        "--object-format" => &mut gathered.object_format,
        "--base" => &mut gathered.base,
        "--candidate" => &mut gathered.candidate,
        "--repository" => &mut gathered.repository,
        "--ref" => &mut gathered.ref_name,
        "--default-branch-ref" => &mut gathered.default_branch_ref,
        "--forge" => &mut gathered.forge,
        "--profile" => &mut gathered.profile,
        _ => &mut gathered.format,
    }
}

fn output_selection(format: &Slot) -> Option<OutputFormat> {
    if format.occurrences == 0 {
        return Some(OutputFormat::Human);
    }
    match format.unique_value() {
        Some("human") => Some(OutputFormat::Human),
        Some("json") => Some(OutputFormat::Json),
        Some(_) | None => None,
    }
}

fn classify(gathered: &Gathered, format: OutputFormat) -> Result<Invocation, BTreeSet<Code>> {
    let mut codes: BTreeSet<Code> = BTreeSet::new();
    if gathered.lexical_defect {
        codes.insert(Code::InvalidInvocation);
    }
    let duplicated = gathered.index > 1
        || gathered.explain_scope > 1
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
        ]
        .iter()
        .any(|slot| slot.defective());
    if duplicated {
        codes.insert(Code::InvalidInvocation);
    }
    for required in [
        &gathered.repo,
        &gathered.object_format,
        &gathered.base,
        &gathered.profile,
    ] {
        if !required.present() {
            codes.insert(Code::InvalidInvocation);
        }
    }
    if gathered.candidate.present() == (gathered.index > 0) {
        codes.insert(Code::InvalidInvocation);
    }

    let repo = match gathered.repo.unique_value() {
        Some("") | None => {
            codes.insert(Code::InvalidInvocation);
            None
        }
        Some(path) => Some(PathBuf::from(path)),
    };

    let object_format = match gathered.object_format.unique_value() {
        Some("sha1") => Some(ObjectFormat::Sha1),
        Some("sha256") => Some(ObjectFormat::Sha256),
        Some(_) => {
            codes.insert(Code::InvalidInvocation);
            None
        }
        None => None,
    };

    let base = decode_oid(&mut codes, object_format, &gathered.base);
    let candidate_oid = decode_oid(&mut codes, object_format, &gathered.candidate);
    if let (Some(base), Some(candidate)) = (&base, &candidate_oid)
        && base == candidate
    {
        codes.insert(Code::InvalidInvocation);
    }

    let profile = match gathered.profile.unique_value() {
        Some("observe") => Some(Profile::Observe),
        Some("enforce") => Some(Profile::Enforce),
        Some(_) => {
            codes.insert(Code::InvalidProfile);
            None
        }
        None => None,
    };

    let identity = classify_identity(&mut codes, gathered);
    let forge = classify_forge(&mut codes, gathered, &identity);

    if !codes.is_empty() {
        return Err(codes);
    }
    match (repo, object_format, base, profile, identity) {
        (Some(repo), Some(object_format), Some(base), Some(profile), Ok(identity)) => {
            let candidate = match candidate_oid {
                Some(oid) => CandidateSelector::Commit(oid),
                None => CandidateSelector::Index,
            };
            Ok(Invocation {
                repo,
                object_format,
                base,
                candidate,
                identity,
                forge,
                profile,
                explain_scope: gathered.explain_scope == 1,
                format,
            })
        }
        _ => Err(BTreeSet::from([Code::InvalidInvocation])),
    }
}

/// The dialect law: an explicit `--forge` names a grammar the engine knows
/// and accompanies the identity triple; without the flag the known-host
/// table decides, and an unknown host means no dialect at all. The github
/// dialect cannot match a nested owner, so that pairing is refused rather
/// than left deterministically dead.
fn classify_forge(
    codes: &mut BTreeSet<Code>,
    gathered: &Gathered,
    identity: &Result<Option<ProviderIdentity>, ()>,
) -> Option<ForgeDialect> {
    let declared = match (gathered.forge.present(), gathered.forge.unique_value()) {
        (false, _) => None,
        (true, Some(value)) => match value.parse::<ForgeDialect>() {
            Ok(dialect) => Some(dialect),
            Err(_unknown) => {
                codes.insert(Code::InvalidInvocation);
                return None;
            }
        },
        (true, None) => {
            codes.insert(Code::InvalidInvocation);
            return None;
        }
    };
    match identity {
        Ok(Some(identity)) => {
            let dialect =
                declared.or_else(|| ForgeDialect::default_for_host(&identity.repository.host));
            if matches!(dialect, Some(ForgeDialect::Github | ForgeDialect::Gitea))
                && identity.repository.owner.contains('/')
            {
                codes.insert(Code::InvalidEvent);
                return None;
            }
            dialect
        }
        Ok(None) => {
            if gathered.forge.present() {
                codes.insert(Code::InvalidInvocation);
            }
            None
        }
        Err(()) => None,
    }
}

fn decode_oid(
    codes: &mut BTreeSet<Code>,
    object_format: Option<ObjectFormat>,
    slot: &Slot,
) -> Option<Oid> {
    let (Some(format), Some(raw)) = (object_format, slot.unique_value()) else {
        return None;
    };
    let oid = Oid::new(format, raw.to_owned());
    if oid.is_none() {
        codes.insert(Code::InvalidInvocation);
    }
    oid
}

type IdentityResult = Result<Option<ProviderIdentity>, ()>;

fn classify_identity(codes: &mut BTreeSet<Code>, gathered: &Gathered) -> IdentityResult {
    let present = [
        gathered.repository.present(),
        gathered.ref_name.present(),
        gathered.default_branch_ref.present(),
    ];
    if present == [false, false, false] {
        return Ok(None);
    }
    if present != [true, true, true] {
        codes.insert(Code::InvalidInvocation);
        return Err(());
    }
    let (Some(repository), Some(ref_value), Some(default_value)) = (
        gathered.repository.unique_value(),
        gathered.ref_name.unique_value(),
        gathered.default_branch_ref.unique_value(),
    ) else {
        return Err(());
    };

    let parts: Vec<&str> = repository.split('/').collect();
    if parts.len() < 3 {
        codes.insert(Code::InvalidInvocation);
        return Err(());
    }
    let host = parts.first().copied().unwrap_or_default();
    let name = parts.last().copied().unwrap_or_default();
    let owner = parts
        .get(1..parts.len().saturating_sub(1))
        .unwrap_or_default()
        .join("/");

    let identity = RepositoryIdentity::new(host.to_owned(), owner, name.to_owned());
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
        codes.insert(Code::InvalidEvent);
        Err(())
    }
}
