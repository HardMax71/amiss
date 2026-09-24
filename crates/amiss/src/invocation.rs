mod arguments;
mod classify;
mod tests;

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::PathBuf;

use amiss_wire::controls::{Profile, ScannerPolicy};
use amiss_wire::model::{
    BranchRef, Digest, ForgeDialect, ObjectFormat, Oid, OwnerId, RepoPath, RepoPathText,
    RepositoryIdentity, UtcInstant,
};
use strum::{AsRefStr, EnumIter, EnumString, IntoStaticStr};

/// The canonical analysis-error taxonomy used by invocation refusals.
pub(crate) use amiss_wire::report::model::AnalysisErrorCode;

/// One refused contract: the wire code and the human line naming the option.
pub(crate) type Refusal = (AnalysisErrorCode, String);

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
            [--semantic-template <path>]
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
amiss record-set --evidence <path>
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
amiss locale-assess --plan <path> --evidence <path> [--format <human|json>]
amiss locale-inventory --repo <path> --plan <path> --context <path>
                       [--format <human|json>]
amiss render --report <path>
             (--format human [--full] | --format <sarif|codequality|junit>)
amiss refs --report <path>
           (--target <repo-path> | --target-bytes-hex <lower-hex>)
           [--format <human|json>]
amiss --help
amiss --version";

const HELP_FLAGS: [&str; 2] = ["--help", "-h"];
const VERSION_FLAGS: [&str; 2] = ["--version", "-V"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumString)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum Verb {
    Check,
    Fix,
    Adopt,
    Claim,
    ExternalPlan,
    ExternalAssess,
    LocaleAssess,
    LocaleInventory,
    Render,
    Refs,
    PolicyInclude,
    RecordSet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumIter, EnumString, IntoStaticStr)]
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
    pub(crate) path: RepoPathText,
    pub(crate) line: u64,
    pub(crate) name: String,
}

/// The inventory form's shape: the checkout to read, the plan that binds its
/// commit, and the locale layout to read the tree under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InventoryInvocation {
    pub(crate) repo: PathBuf,
    pub(crate) plan: PathBuf,
    pub(crate) context: PathBuf,
    pub(crate) format: OutputFormat,
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
    pub(crate) policy_digest: Digest,
    pub(crate) preview: Option<PolicyIncludePreview>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecordSetInvocation {
    pub(crate) input: PathBuf,
}

/// One accepted command line: a scan-shaped verb, the authoring form, or a
/// report-bound pure form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Scan(Box<Invocation>),
    Author(AuthorInvocation),
    Plan(PlanInvocation),
    Assess(AssessInvocation),
    LocaleAssess(AssessInvocation),
    LocaleInventory(InventoryInvocation),
    Render(RenderInvocation),
    Refs(RefsInvocation),
    PolicyInclude(PolicyIncludeInvocation),
    RecordSet(RecordSetInvocation),
}

/// The adoption metadata the engine cannot know: who owns the recorded
/// debt, why, its instants, the floor it binds to, and where the file goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Adoption {
    pub(crate) floor_digest: Digest,
    pub(crate) owner: OwnerId,
    pub(crate) reason: String,
    pub(crate) created_at: UtcInstant,
    pub(crate) expires_at: UtcInstant,
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
    pub(crate) semantic_template: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The whole grammar, or one verb's lines of it.
    Help {
        verb: Option<Verb>,
    },
    /// Carries no options, so a second token is an ordinary invalid invocation.
    Version,
    /// Output selection itself is invalid: empty stdout, one stderr line
    /// carrying the reason, exit 2, and no envelope may be chosen by
    /// conflicting values.
    MalformedOutputSelection {
        reason: String,
    },
    Rejected {
        format: OutputFormat,
        refusals: BTreeSet<Refusal>,
    },
    Accepted(Box<Command>),
}

#[must_use]
pub(crate) fn parse(argv: &[OsString]) -> Outcome {
    let words: Vec<Option<&str>> = argv.iter().map(|token| token.to_str()).collect();
    match words.as_slice() {
        [Some(flag)] if HELP_FLAGS.contains(flag) => return Outcome::Help { verb: None },
        [Some(flag)] if VERSION_FLAGS.contains(flag) => return Outcome::Version,
        [Some(verb), Some(flag)] if HELP_FLAGS.contains(flag) => {
            if let Ok(verb) = verb.parse() {
                return Outcome::Help { verb: Some(verb) };
            }
        }
        _ => {}
    }
    let gathered = arguments::gather(argv);
    let format = match arguments::output_selection(&gathered.format) {
        Ok(format) => format,
        Err(reason) => return Outcome::MalformedOutputSelection { reason },
    };
    match classify::command(&gathered, format) {
        Ok(command) => Outcome::Accepted(Box::new(command)),
        Err(refusals) => Outcome::Rejected { format, refusals },
    }
}

/// The lines of the grammar that spell one verb's form.
#[must_use]
pub(crate) fn verb_grammar(verb: Verb) -> String {
    let mut inside = false;
    GRAMMAR
        .lines()
        .filter(|line| {
            if let Some(form) = line.strip_prefix("amiss ") {
                inside = form.split_whitespace().next() == Some(verb.as_ref());
            }
            inside
        })
        .collect::<Vec<&str>>()
        .join("\n")
}
