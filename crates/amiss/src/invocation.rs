mod arguments;
mod classify;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::PathBuf;

use amiss_wire::controls::Profile;
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepoPath, RepositoryIdentity};
use strum::EnumString;

/// The canonical analysis-error taxonomy used by invocation refusals.
pub use amiss_wire::report::AnalysisErrorCode as Code;

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
             [--forge <github|gitlab|gitea|bitbucket-cloud>]]
            --profile <observe|enforce-introduced|enforce>
            [--explain-scope] [--format <human|json|sarif|codequality>]
amiss fix   --repo <path> --object-format <sha1|sha256>
            --base <full-oid> --index
            [--repository <host>/<owner>/<name>
             --ref refs/heads/<name>
             --default-branch-ref refs/heads/<name>
             [--forge <github|gitlab|gitea|bitbucket-cloud>]]
            --profile <observe|enforce-introduced|enforce>
amiss claim --repo <path> --path <repo-path> --line <n> --name <name>
amiss adopt --repo <path> --object-format <sha1|sha256>
            --base <full-oid> --candidate <full-oid>
            --repository <host>/<owner>/<name>
            --ref refs/heads/<name>
            --default-branch-ref refs/heads/<name>
            [--forge <github|gitlab|gitea|bitbucket-cloud>]
            --floor-digest sha256:<64-hex> --debt-owner <name>
            --debt-reason <text> --created-at <utc-instant>
            --expires-at <utc-instant> --debt-output <path>
amiss external-plan --report <path> [--format <human|json>]
amiss external-assess --plan <path> --evidence <path> [--format <human|json>]
amiss render --report <path> --format <human|sarif|codequality>
amiss refs --report <path>
           (--target <repo-path> | --target-bytes-hex <lower-hex>)
           [--format <human|json>]
amiss --version";

const VERSION_FLAG: &str = "--version";

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub enum Verb {
    Check,
    Fix,
    Adopt,
    Claim,
    ExternalPlan,
    ExternalAssess,
    Render,
    Refs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString)]
#[strum(serialize_all = "lowercase")]
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

/// The report and alternate projection selected by the rendering form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderInvocation {
    pub report: PathBuf,
    pub format: OutputFormat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefsInvocation {
    pub report: PathBuf,
    pub target: RepoPath,
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
    Render(RenderInvocation),
    Refs(RefsInvocation),
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

#[must_use]
pub fn parse(argv: &[OsString]) -> Outcome {
    if let [only] = argv
        && only.to_str() == Some(VERSION_FLAG)
    {
        return Outcome::Version;
    }
    let gathered = arguments::gather(argv);
    let Some(format) = arguments::output_selection(&gathered.format) else {
        return Outcome::MalformedOutputSelection;
    };
    match classify::command(&gathered, format) {
        Ok(command) => Outcome::Accepted(Box::new(command)),
        Err(codes) => Outcome::Rejected { format, codes },
    }
}
