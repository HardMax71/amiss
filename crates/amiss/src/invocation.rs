mod arguments;
mod classify;
mod tests;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::PathBuf;

use amiss_wire::controls::{Profile, ScannerPolicy};
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepoPath, RepositoryIdentity};
use strum::EnumString;

/// The canonical analysis-error taxonomy used by invocation refusals.
pub(crate) use amiss_wire::report::AnalysisErrorCode as Code;

pub(crate) const MALFORMED_OUTPUT_LINE: &str = "amiss: invalid invocation\n";

/// The closed grammar, verbatim. Help prints it directly, while a rejected
/// human invocation prints it after the code lines; the documentation
/// contract test keeps the invocation chapter's copy equal to this one.
pub(crate) const GRAMMAR: &str = "amiss check --repo <path> --object-format <sha1|sha256>
            --base <full-oid> (--candidate <full-oid> | --index)
            [--repository <host>/<owner>/<name>
             --ref refs/heads/<name>
             --default-branch-ref refs/heads/<name>
             [--forge <github|gitlab|gitea|bitbucket-cloud|bitbucket-data-center>]]
            --profile <observe|enforce-introduced|enforce>
            [--explain-scope] [--format <human|json|sarif|codequality>]
amiss fix   --repo <path> --object-format <sha1|sha256>
            --base <full-oid> --index
            [--repository <host>/<owner>/<name>
             --ref refs/heads/<name>
             --default-branch-ref refs/heads/<name>
             [--forge <github|gitlab|gitea|bitbucket-cloud|bitbucket-data-center>]]
            --profile <observe|enforce-introduced|enforce>
amiss claim --repo <path> --path <repo-path> --line <n> --name <name>
amiss policy-include --path <repo-path> --suffix <suffix> --adapter <adapter>
                     [--repo <path> --object-format <sha1|sha256> --index]
amiss adopt --repo <path> --object-format <sha1|sha256>
            --base <full-oid> --candidate <full-oid>
            --repository <host>/<owner>/<name>
            --ref refs/heads/<name>
            --default-branch-ref refs/heads/<name>
            [--forge <github|gitlab|gitea|bitbucket-cloud|bitbucket-data-center>]
            --floor-digest sha256:<64-hex> --debt-owner <name>
            --debt-reason <text> --created-at <utc-instant>
            --expires-at <utc-instant> --debt-output <path>
amiss external-plan --report <path> [--format <human|json>]
amiss external-assess --plan <path> --evidence <path> [--format <human|json>]
amiss render --report <path>
             (--format human [--full] | --format <sarif|codequality|junit>)
amiss refs --report <path>
           (--target <repo-path> | --target-bytes-hex <lower-hex>)
           [--format <human|json>]
amiss --help
amiss --version";

const HELP_FLAG: &str = "--help";
const VERSION_FLAG: &str = "--version";

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum Verb {
    Check,
    Fix,
    Adopt,
    Claim,
    ExternalPlan,
    ExternalAssess,
    Render,
    Refs,
    PolicyInclude,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString)]
#[strum(serialize_all = "lowercase")]
pub(crate) enum OutputFormat {
    Human,
    Json,
    Sarif,
    CodeQuality,
    Junit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CandidateSelector {
    Commit(Oid),
    Index,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProviderIdentity {
    pub(crate) repository: RepositoryIdentity,
    pub(crate) ref_name: BranchRef,
    pub(crate) default_branch_ref: BranchRef,
}

/// The claim the author wants pinned: where, which line, and its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AuthorInvocation {
    pub(crate) repo: PathBuf,
    pub(crate) path: String,
    pub(crate) line: u64,
    pub(crate) name: String,
}

/// The plan form's shape: the report it reads and the projection it prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlanInvocation {
    pub(crate) report: PathBuf,
    pub(crate) format: OutputFormat,
}

/// The assessment form's shape: the plan and evidence it judges and the
/// projection it prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AssessInvocation {
    pub(crate) plan: PathBuf,
    pub(crate) evidence: PathBuf,
    pub(crate) format: OutputFormat,
}

/// The report and alternate projection selected by the rendering form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RenderInvocation {
    pub(crate) report: PathBuf,
    pub(crate) format: OutputFormat,
    pub(crate) full: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RefsInvocation {
    pub(crate) report: PathBuf,
    pub(crate) target: RepoPath,
    pub(crate) format: OutputFormat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PolicyIncludePreview {
    pub(crate) repo: PathBuf,
    pub(crate) object_format: ObjectFormat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PolicyIncludeInvocation {
    pub(crate) policy: ScannerPolicy,
    pub(crate) preview: Option<PolicyIncludePreview>,
}

/// One accepted command line: a scan-shaped verb, the authoring form, or a
/// report-bound pure form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Scan(Box<Invocation>),
    Author(AuthorInvocation),
    Plan(PlanInvocation),
    Assess(AssessInvocation),
    Render(RenderInvocation),
    Refs(RefsInvocation),
    PolicyInclude(PolicyIncludeInvocation),
}

/// The adoption metadata the engine cannot know: who owns the recorded
/// debt, why, its instants, the floor it binds to, and where the file goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Adoption {
    pub(crate) floor_digest: String,
    pub(crate) owner: String,
    pub(crate) reason: String,
    pub(crate) created_at: String,
    pub(crate) expires_at: String,
    pub(crate) output: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Invocation {
    pub(crate) verb: Verb,
    pub(crate) repo: PathBuf,
    pub(crate) object_format: ObjectFormat,
    pub(crate) base: Oid,
    pub(crate) candidate: CandidateSelector,
    pub(crate) identity: Option<ProviderIdentity>,
    pub(crate) forge: Option<ForgeDialect>,
    pub(crate) profile: Profile,
    pub(crate) explain_scope: bool,
    pub(crate) format: OutputFormat,
    pub(crate) adoption: Option<Adoption>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Help,
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
pub(crate) fn parse(argv: &[OsString]) -> Outcome {
    if let [only] = argv {
        if only.to_str() == Some(HELP_FLAG) {
            return Outcome::Help;
        }
        if only.to_str() == Some(VERSION_FLAG) {
            return Outcome::Version;
        }
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
