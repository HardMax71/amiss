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
            --profile <observe|enforce-introduced|enforce>
            [--explain-scope] [--format <human|json|sarif|codequality>]
amiss fix   --repo <path> --object-format <sha1|sha256>
            --base <full-oid> --index
            [--repository <host>/<owner>/<name>
             --ref refs/heads/<name>
             --default-branch-ref refs/heads/<name>
             [--forge <github|gitlab|gitea>]]
            --profile <observe|enforce-introduced|enforce>
amiss claim --repo <path> --path <repo-path> --line <n> --name <name>
amiss adopt --repo <path> --object-format <sha1|sha256>
            --base <full-oid> --candidate <full-oid>
            --repository <host>/<owner>/<name>
            --ref refs/heads/<name>
            --default-branch-ref refs/heads/<name>
            [--forge <github|gitlab|gitea>]
            --floor-digest sha256:<64-hex> --debt-owner <name>
            --debt-reason <text> --created-at <utc-instant>
            --expires-at <utc-instant> --debt-output <path>
amiss external-plan --report <path> [--format <human|json>]
amiss external-assess --plan <path> --evidence <path> [--format <human|json>]
amiss --version";

const VERSION_FLAG: &str = "--version";

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
                 are full refs such as refs/heads/main. A host outside github.com, gitlab.com, \
                 and codeberg.org needs --forge to name its dialect. Forges report the owner \
                 with its original capitals, so a workflow passing ${{ github.repository }} \
                 has to lowercase it first."
            }
            Self::InvalidInvocation => {
                "every option is spelled exactly, appears at most once, and carries a value. \
                 --base and --candidate are distinct full lowercase object IDs, never refs and \
                 never abbreviations. --forge is github, gitlab, or gitea, names a dialect the \
                 engine knows, and accompanies the --repository triple."
            }
            Self::InvalidProfile => "--profile is observe, enforce-introduced, or enforce.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Check,
    Fix,
    Adopt,
    Claim,
    ExternalPlan,
    ExternalAssess,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Human,
    Json,
    Sarif,
    CodeQuality,
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

/// The claim the author wants pinned: where, which line, and its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorInvocation {
    pub repo: PathBuf,
    pub path: String,
    pub line: u64,
    pub name: String,
}

/// The plan form's shape: the report it reads and the projection it prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanInvocation {
    pub report: PathBuf,
    pub format: OutputFormat,
}

/// The assessment form's shape: the plan and evidence it judges and the
/// projection it prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssessInvocation {
    pub plan: PathBuf,
    pub evidence: PathBuf,
    pub format: OutputFormat,
}

/// One accepted command line: a scan-shaped verb, the authoring form, or a
/// report-bound pure form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Scan(Box<Invocation>),
    Author(AuthorInvocation),
    Plan(PlanInvocation),
    Assess(AssessInvocation),
}

/// The adoption metadata the engine cannot know: who owns the recorded
/// debt, why, its instants, the floor it binds to, and where the file goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Adoption {
    pub floor_digest: String,
    pub owner: String,
    pub reason: String,
    pub created_at: String,
    pub expires_at: String,
    pub output: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    pub verb: Verb,
    pub repo: PathBuf,
    pub object_format: ObjectFormat,
    pub base: Oid,
    pub candidate: CandidateSelector,
    pub identity: Option<ProviderIdentity>,
    pub forge: Option<ForgeDialect>,
    pub profile: Profile,
    pub explain_scope: bool,
    pub format: OutputFormat,
    pub adoption: Option<Adoption>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Carries no options, so a second token is an ordinary invalid invocation.
    Version,
    /// Output selection itself is invalid: empty stdout, one fixed stderr
    /// line, exit 2, and no envelope may be chosen by conflicting values.
    MalformedOutputSelection,
    Rejected {
        format: OutputFormat,
        codes: BTreeSet<Code>,
    },
    Accepted(Box<Command>),
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
    verb: Option<Verb>,
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
    floor_digest: Slot,
    debt_owner: Slot,
    debt_reason: Slot,
    created_at: Slot,
    expires_at: Slot,
    debt_output: Slot,
    claim_path: Slot,
    claim_line: Slot,
    claim_name: Slot,
    report: Slot,
    plan: Slot,
    evidence: Slot,
    index: usize,
    explain_scope: usize,
    lexical_defect: bool,
}

#[must_use]
pub fn parse(argv: &[OsString]) -> Outcome {
    if let [only] = argv
        && only.to_str() == Some(VERSION_FLAG)
    {
        return Outcome::Version;
    }
    let gathered = gather(argv);
    let Some(format) = output_selection(&gathered.format) else {
        return Outcome::MalformedOutputSelection;
    };
    match classify(&gathered, format) {
        Ok(command) => Outcome::Accepted(Box::new(command)),
        Err(codes) => Outcome::Rejected { format, codes },
    }
}

fn gather(argv: &[OsString]) -> Gathered {
    let mut gathered = Gathered::default();
    let mut tokens = argv.iter().map(|token| token.to_str()).peekable();

    match tokens.next() {
        Some(Some("check")) => gathered.verb = Some(Verb::Check),
        Some(Some("fix")) => gathered.verb = Some(Verb::Fix),
        Some(Some("adopt")) => gathered.verb = Some(Verb::Adopt),
        Some(Some("claim")) => gathered.verb = Some(Verb::Claim),
        Some(Some("external-plan")) => gathered.verb = Some(Verb::ExternalPlan),
        Some(Some("external-assess")) => gathered.verb = Some(Verb::ExternalAssess),
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
            | "--format"
            | "--floor-digest"
            | "--debt-owner"
            | "--debt-reason"
            | "--created-at"
            | "--expires-at"
            | "--debt-output"
            | "--path"
            | "--line"
            | "--name"
            | "--report"
            | "--plan"
            | "--evidence" => {
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
        "--floor-digest" => &mut gathered.floor_digest,
        "--debt-owner" => &mut gathered.debt_owner,
        "--debt-reason" => &mut gathered.debt_reason,
        "--created-at" => &mut gathered.created_at,
        "--expires-at" => &mut gathered.expires_at,
        "--debt-output" => &mut gathered.debt_output,
        "--path" => &mut gathered.claim_path,
        "--line" => &mut gathered.claim_line,
        "--name" => &mut gathered.claim_name,
        "--report" => &mut gathered.report,
        "--plan" => &mut gathered.plan,
        "--evidence" => &mut gathered.evidence,
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
        Some("sarif") => Some(OutputFormat::Sarif),
        Some("codequality") => Some(OutputFormat::CodeQuality),
        Some(_) | None => None,
    }
}

fn duplicated(gathered: &Gathered) -> bool {
    gathered.index > 1
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
            &gathered.floor_digest,
            &gathered.debt_owner,
            &gathered.debt_reason,
            &gathered.created_at,
            &gathered.expires_at,
            &gathered.debt_output,
            &gathered.claim_path,
            &gathered.claim_line,
            &gathered.claim_name,
            &gathered.report,
            &gathered.plan,
            &gathered.evidence,
        ]
        .iter()
        .any(|slot| slot.defective())
}

fn classify(gathered: &Gathered, format: OutputFormat) -> Result<Command, BTreeSet<Code>> {
    let mut codes: BTreeSet<Code> = BTreeSet::new();
    if gathered.lexical_defect || duplicated(gathered) {
        codes.insert(Code::InvalidInvocation);
    }
    if gathered.verb == Some(Verb::Claim) {
        return classify_claim(codes, gathered).map(Command::Author);
    }
    if gathered.verb == Some(Verb::ExternalPlan) {
        return classify_plan(codes, gathered, format).map(Command::Plan);
    }
    if gathered.verb == Some(Verb::ExternalAssess) {
        return classify_assess(codes, gathered, format).map(Command::Assess);
    }
    for required in [&gathered.repo, &gathered.object_format, &gathered.base] {
        if !required.present() {
            codes.insert(Code::InvalidInvocation);
        }
    }
    verb_rules(&mut codes, gathered);
    if gathered.candidate.present() == (gathered.index > 0) {
        codes.insert(Code::InvalidInvocation);
    }

    let (repo, object_format) = classify_target(&mut codes, gathered);

    let base = decode_oid(&mut codes, object_format, &gathered.base);
    let candidate_oid = decode_oid(&mut codes, object_format, &gathered.candidate);
    if let (Some(base), Some(candidate)) = (&base, &candidate_oid)
        && base == candidate
    {
        codes.insert(Code::InvalidInvocation);
    }

    let profile = if gathered.verb == Some(Verb::Adopt) {
        Some(Profile::Enforce)
    } else {
        match gathered.profile.unique_value() {
            Some("observe") => Some(Profile::Observe),
            Some("enforce-introduced") => Some(Profile::EnforceIntroduced),
            Some("enforce") => Some(Profile::Enforce),
            Some(_) => {
                codes.insert(Code::InvalidProfile);
                None
            }
            None => None,
        }
    };

    let adoption = classify_adoption(&mut codes, gathered);

    let identity = classify_identity(&mut codes, gathered);
    let forge = classify_forge(&mut codes, gathered, &identity);

    if !codes.is_empty() {
        return Err(codes);
    }
    match (gathered.verb, repo, object_format, base, profile, identity) {
        (Some(verb), Some(repo), Some(object_format), Some(base), Some(profile), Ok(identity)) => {
            let candidate = match candidate_oid {
                Some(oid) => CandidateSelector::Commit(oid),
                None => CandidateSelector::Index,
            };
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
        _ => Err(BTreeSet::from([Code::InvalidInvocation])),
    }
}

/// The path refuses the bytes the claim url and the extractor cannot carry.
fn classify_claim(
    mut codes: BTreeSet<Code>,
    gathered: &Gathered,
) -> Result<AuthorInvocation, BTreeSet<Code>> {
    let foreign = [
        &gathered.object_format,
        &gathered.base,
        &gathered.candidate,
        &gathered.repository,
        &gathered.ref_name,
        &gathered.default_branch_ref,
        &gathered.forge,
        &gathered.profile,
        &gathered.format,
        &gathered.floor_digest,
        &gathered.debt_owner,
        &gathered.debt_reason,
        &gathered.created_at,
        &gathered.expires_at,
        &gathered.debt_output,
        &gathered.report,
        &gathered.plan,
        &gathered.evidence,
    ];
    if foreign.iter().any(|slot| slot.present()) || gathered.index > 0 || gathered.explain_scope > 0
    {
        codes.insert(Code::InvalidInvocation);
    }
    let repo = match gathered.repo.unique_value() {
        Some("") | None => {
            codes.insert(Code::InvalidInvocation);
            None
        }
        Some(path) => Some(PathBuf::from(path)),
    };
    let name = gathered.claim_name.unique_value().filter(|value| {
        let mut bytes = value.bytes();
        let head = bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric());
        head && value.len() <= 120
            && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    });
    let line = gathered.claim_line.unique_value().and_then(|value| {
        let lawful = !value.is_empty()
            && value.len() <= 16
            && !value.starts_with('0')
            && value.bytes().all(|byte| byte.is_ascii_digit());
        let ceiling = u64::try_from(amiss_wire::json::MAX_SAFE_INTEGER).ok()?;
        if lawful {
            value.parse::<u64>().ok().filter(|line| *line <= ceiling)
        } else {
            None
        }
    });
    let path = gathered.claim_path.unique_value().filter(|value| {
        amiss_wire::model::RepoPath::new((*value).to_owned())
            .is_some_and(|path| path.as_str().is_some())
            && !value.contains(['&', '<', '>', '"', ' ', '%', '?', '#', '\\'])
    });
    match (repo, name, line, path) {
        (Some(repo), Some(name), Some(line), Some(path)) if codes.is_empty() => {
            Ok(AuthorInvocation {
                repo,
                path: path.to_owned(),
                line,
                name: name.to_owned(),
            })
        }
        (_, _, _, _) => {
            codes.insert(Code::InvalidInvocation);
            Err(codes)
        }
    }
}

/// The pure-form gate: a report-bound verb reads its own path flags and
/// projects as human or JSON; every scan, claim, and adoption option is
/// foreign, as are the other pure forms' paths. Accepts with exactly one
/// path per required slot, in the order given, or carries every code out.
fn classify_pure(
    mut codes: BTreeSet<Code>,
    gathered: &Gathered,
    format: OutputFormat,
    required: &[&Slot],
    foreign_pure: &[&Slot],
) -> Result<Vec<PathBuf>, BTreeSet<Code>> {
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
    ];
    if foreign
        .iter()
        .chain(foreign_pure)
        .any(|slot| slot.present())
        || gathered.index > 0
        || gathered.explain_scope > 0
        || matches!(format, OutputFormat::Sarif | OutputFormat::CodeQuality)
    {
        codes.insert(Code::InvalidInvocation);
    }
    let mut paths = Vec::new();
    for slot in required {
        match slot.unique_value() {
            Some("") | None => {
                codes.insert(Code::InvalidInvocation);
            }
            Some(path) => paths.push(PathBuf::from(path)),
        }
    }
    if codes.is_empty() && paths.len() == required.len() {
        Ok(paths)
    } else {
        codes.insert(Code::InvalidInvocation);
        Err(codes)
    }
}

fn classify_plan(
    codes: BTreeSet<Code>,
    gathered: &Gathered,
    format: OutputFormat,
) -> Result<PlanInvocation, BTreeSet<Code>> {
    let required = [&gathered.report];
    let foreign = [&gathered.plan, &gathered.evidence];
    let mut paths = classify_pure(codes, gathered, format, &required, &foreign)?;
    let report = paths
        .pop()
        .ok_or_else(|| BTreeSet::from([Code::InvalidInvocation]))?;
    Ok(PlanInvocation { report, format })
}

fn classify_assess(
    codes: BTreeSet<Code>,
    gathered: &Gathered,
    format: OutputFormat,
) -> Result<AssessInvocation, BTreeSet<Code>> {
    let required = [&gathered.plan, &gathered.evidence];
    let foreign = [&gathered.report];
    let mut paths = classify_pure(codes, gathered, format, &required, &foreign)?;
    let (Some(evidence), Some(plan)) = (paths.pop(), paths.pop()) else {
        return Err(BTreeSet::from([Code::InvalidInvocation]));
    };
    Ok(AssessInvocation {
        plan,
        evidence,
        format,
    })
}

fn classify_target(
    codes: &mut BTreeSet<Code>,
    gathered: &Gathered,
) -> (Option<PathBuf>, Option<ObjectFormat>) {
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
    (repo, object_format)
}

/// The per-verb shape: adoption bakes enforce so the profile is refused
/// there and required elsewhere, the six adoption values are required there
/// and refused elsewhere, the repair form is staged-only without report
/// flags, and the adoption form is commit-pair only without them.
fn verb_rules(codes: &mut BTreeSet<Code>, gathered: &Gathered) {
    if [&gathered.report, &gathered.plan, &gathered.evidence]
        .iter()
        .any(|slot| slot.present())
    {
        codes.insert(Code::InvalidInvocation);
    }
    if gathered.verb == Some(Verb::Adopt) {
        if gathered.profile.present() {
            codes.insert(Code::InvalidInvocation);
        }
    } else if !gathered.profile.present() {
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
        if adoption_slots.iter().any(|slot| !slot.present()) {
            codes.insert(Code::InvalidInvocation);
        }
    } else if adoption_slots.iter().any(|slot| slot.present()) {
        codes.insert(Code::InvalidInvocation);
    }
    if gathered.verb == Some(Verb::Fix)
        && (gathered.candidate.present() || gathered.format.present() || gathered.explain_scope > 0)
    {
        codes.insert(Code::InvalidInvocation);
    }
    if gathered.verb == Some(Verb::Adopt)
        && (gathered.index > 0
            || gathered.format.present()
            || gathered.explain_scope > 0
            || !gathered.repository.present())
    {
        codes.insert(Code::InvalidInvocation);
    }
}

/// Every adoption value is validated where the grammar can see it: the
/// floor digest by its exact spelling, both instants by the wire's own
/// clock grammar, and the free-text fields by being nonempty.
fn classify_adoption(codes: &mut BTreeSet<Code>, gathered: &Gathered) -> Option<Adoption> {
    if gathered.verb != Some(Verb::Adopt) {
        return None;
    }
    let digest = gathered.floor_digest.unique_value().filter(|value| {
        value.strip_prefix("sha256:").is_some_and(|hex| {
            hex.len() == 64
                && hex
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
    });
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
    let ordered = matches!(
        (
            instant(&gathered.created_at),
            instant(&gathered.expires_at)
        ),
        (Some(created), Some(expires)) if created < expires
    );
    if gathered.verb == Some(Verb::Adopt) && !ordered {
        codes.insert(Code::InvalidInvocation);
    }
    let fields = (
        digest.map(str::to_owned),
        nonempty(&gathered.debt_owner),
        nonempty(&gathered.debt_reason),
        instant(&gathered.created_at),
        instant(&gathered.expires_at),
        nonempty(&gathered.debt_output),
    );
    if let (
        Some(floor_digest),
        Some(owner),
        Some(reason),
        Some(created_at),
        Some(expires_at),
        Some(output),
    ) = fields
    {
        Some(Adoption {
            floor_digest,
            owner,
            reason,
            created_at,
            expires_at,
            output: PathBuf::from(output),
        })
    } else {
        codes.insert(Code::InvalidInvocation);
        None
    }
}

/// The dialect law: an explicit `--forge` names a grammar the engine knows
/// and accompanies the identity triple; without the flag the known-host
/// table decides, and an identity on a host outside the table is refused,
/// since accepting it would silently leave every same-repository URL
/// external. The github dialect cannot match a nested owner, so that
/// pairing is refused rather than left deterministically dead.
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
            let Some(dialect) =
                declared.or_else(|| ForgeDialect::default_for_host(&identity.repository.host))
            else {
                codes.insert(Code::InvalidEvent);
                return None;
            };
            if matches!(dialect, ForgeDialect::Github | ForgeDialect::Gitea)
                && identity.repository.owner.contains('/')
            {
                codes.insert(Code::InvalidEvent);
                return None;
            }
            Some(dialect)
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
