use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::process::ExitCode;

use amiss::invocation::{self, CandidateSelector, Code, Invocation, Outcome, OutputFormat};
use amiss_wire::ExitClass;
use amiss_wire::digest::hb;
use amiss_wire::report::{self, AnalysisErrorCode, EngineProvenance, ErrorDetail, FatalSerializer};

/// The engine's memory ceiling, held as an address-space limit. It is the
/// blocking sandbox descriptor's figure, applied here as well so that a
/// repository which cannot be scanned under the required check cannot be
/// scanned locally either, and the two runs stay the same run.
#[cfg(unix)]
const SANDBOX_MEMORY_BYTES: u64 = 1_073_741_824;

/// Self-restriction, in safe Rust only: no child processes (the contract's
/// zero repository-process budget), no core dumps (the address space holds
/// repository bytes), and the sandbox's memory ceiling. Failures are
/// tolerated, since a plain process is always self-asserted; the report says
/// so, and the closed provider-verified mechanisms are the controller's to
/// enforce. Network denial is structural: the engine has no network code and
/// no network dependency.
#[cfg(unix)]
fn apply_sandbox() {
    use rustix::process::{Resource, Rlimit, setrlimit};
    let zero = Rlimit {
        current: Some(0),
        maximum: Some(0),
    };
    let _forks = setrlimit(Resource::Nproc, zero);
    let _core = setrlimit(Resource::Core, zero);
    let _memory = setrlimit(
        Resource::As,
        Rlimit {
            current: Some(SANDBOX_MEMORY_BYTES),
            maximum: Some(SANDBOX_MEMORY_BYTES),
        },
    );
}

#[cfg(not(unix))]
const fn apply_sandbox() {}

/// The wire's lowercase hex back to raw bytes; a malformed digit renders as
/// zero rather than failing the human projection, which is not the wire.
fn decode_hex(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|text| u8::from_str_radix(text, 16).ok())
                .unwrap_or(0)
        })
        .collect()
}

#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn main() -> ExitCode {
    apply_sandbox();
    let mut reserve = FatalSerializer::new();
    let argv: Vec<std::ffi::OsString> = env::args_os().skip(1).collect();
    let failure = ExitCode::from(ExitClass::Failure.code());
    match invocation::parse(&argv) {
        Outcome::MalformedOutputSelection => {
            eprint!("{}", invocation::MALFORMED_OUTPUT_LINE);
            failure
        }
        Outcome::Rejected {
            format: OutputFormat::Json,
            codes,
        } => {
            let Some(engine) = engine_provenance() else {
                eprintln!("amiss: {}", AnalysisErrorCode::InternalError.as_str());
                return failure;
            };
            let codes: BTreeSet<AnalysisErrorCode> =
                codes.iter().map(|code| analysis_code(*code)).collect();
            let Some(envelope) = report::invocation_failure_envelope(&engine, &codes) else {
                eprintln!(
                    "amiss: {}",
                    AnalysisErrorCode::ReportConstructionFailed.as_str()
                );
                return failure;
            };
            emit(&mut reserve, &envelope);
            failure
        }
        Outcome::Rejected {
            format: OutputFormat::Human,
            codes,
        } => {
            for code in &codes {
                eprintln!("amiss: {}", code.as_str());
                eprintln!("  {}", code.contract());
            }
            failure
        }
        Outcome::Accepted(invocation) => run(&invocation, &mut reserve),
    }
}

#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn run(invocation: &Invocation, reserve: &mut FatalSerializer) -> ExitCode {
    use amiss_scan::pipeline::{SetupShell, commit_pair};
    use amiss_scan::resolve::ForgeContext;

    let failure = ExitCode::from(ExitClass::Failure.code());
    let Some(engine) = engine_provenance() else {
        eprintln!("amiss: {}", AnalysisErrorCode::InternalError.as_str());
        return failure;
    };
    let repo = match amiss_git::Repository::open(&invocation.repo, invocation.object_format) {
        Ok(repo) => repo,
        Err(defect) => {
            let code = match defect {
                amiss_git::Error::RepositoryUnavailable => {
                    AnalysisErrorCode::GitRepositoryUnavailable
                }
                amiss_git::Error::ObjectMissing => AnalysisErrorCode::GitObjectMissing,
                amiss_git::Error::ObjectWrongKind => AnalysisErrorCode::GitObjectWrongKind,
                amiss_git::Error::ObjectUnreadable | amiss_git::Error::ResourceLimit { .. } => {
                    AnalysisErrorCode::GitObjectUnreadable
                }
                amiss_git::Error::IndexInvalid => AnalysisErrorCode::GitIndexInvalid,
                amiss_git::Error::IndexUnmerged => AnalysisErrorCode::GitIndexUnmerged,
                amiss_git::Error::IntentToAdd => AnalysisErrorCode::GitIntentToAdd,
                amiss_git::Error::SnapshotChanged => AnalysisErrorCode::GitSnapshotChanged,
            };
            return fatal(
                invocation,
                &engine,
                &[ErrorDetail {
                    code,
                    path: None,
                    path_bytes: None,
                    resource: None,
                }],
                reserve,
            );
        }
    };

    let forge = invocation.identity.as_ref().map(|identity| ForgeContext {
        host: identity.repository.host.clone(),
        dialect: amiss_wire::model::ForgeDialect::Github,
        owner: identity.repository.owner.clone(),
        repository: identity.repository.name.clone(),
        candidate_ref: identity.ref_name.as_str().to_owned(),
        default_ref: identity.default_branch_ref.as_str().to_owned(),
        candidate_oid: match &invocation.candidate {
            CandidateSelector::Commit(oid) => Some(oid.as_str().to_owned()),
            CandidateSelector::Index => None,
        },
    });
    let shell = SetupShell {
        engine,
        enforce: matches!(invocation.profile, amiss_wire::controls::Profile::Enforce),
        repository: invocation
            .identity
            .as_ref()
            .map(|identity| identity.repository.clone()),
        candidate_ref: invocation
            .identity
            .as_ref()
            .map(|identity| identity.ref_name.as_str().to_owned()),
        default_branch_ref: invocation
            .identity
            .as_ref()
            .map(|identity| identity.default_branch_ref.as_str().to_owned()),
        // The frozen invocation grammar has no control-supply surface; the
        // required wrapper feeds these when its interop RFC lands.
        floor: None,
        debt: None,
        waiver: None,
        time: None,
        constraint: None,
        requests: amiss_scan::report::RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    };
    let built = match &invocation.candidate {
        CandidateSelector::Commit(candidate_oid) => commit_pair(
            &repo,
            &shell.engine,
            forge.as_ref(),
            &shell,
            &invocation.base,
            candidate_oid,
        ),
        CandidateSelector::Index => amiss_scan::pipeline::staged_index(
            &repo,
            &shell.engine,
            forge.as_ref(),
            &shell,
            &invocation.base,
        ),
    };
    match invocation.format {
        OutputFormat::Json => emit(reserve, &built.envelope),
        OutputFormat::Human => human(&built, invocation.explain_scope),
    }
    exit_class(built.exit_code)
}

fn fatal(
    invocation: &Invocation,
    engine: &EngineProvenance,
    details: &[ErrorDetail],
    reserve: &mut FatalSerializer,
) -> ExitCode {
    use amiss_scan::report::{Setup, SnapshotIdentity, construct_incomplete};

    let identity = |oid: &amiss_wire::model::Oid| SnapshotIdentity {
        object_format: match invocation.object_format {
            amiss_wire::model::ObjectFormat::Sha1 => "sha1",
            amiss_wire::model::ObjectFormat::Sha256 => "sha256",
        },
        commit_oid: oid.as_str().to_owned(),
        tree_oid: oid.as_str().to_owned(),
    };
    let candidate = match &invocation.candidate {
        CandidateSelector::Commit(oid) => amiss_scan::report::CandidateBlock::Commit(identity(oid)),
        CandidateSelector::Index => {
            amiss_scan::report::CandidateBlock::Unavailable(vec!["not-evaluated"])
        }
    };
    let setup = Setup {
        engine: engine.clone(),
        enforce: matches!(invocation.profile, amiss_wire::controls::Profile::Enforce),
        repository: None,
        candidate_ref: None,
        default_branch_ref: None,
        base: identity(&invocation.base),
        candidate,
        policy: amiss_scan::policy::Effects::default(),
        controls_unavailable: None,
        requests: amiss_scan::report::RequestDigests::default(),
    };
    let built = construct_incomplete(&setup, details);
    match invocation.format {
        OutputFormat::Json => emit(reserve, &built.envelope),
        OutputFormat::Human => human(&built, invocation.explain_scope),
    }
    ExitCode::from(ExitClass::Failure.code())
}

#[expect(clippy::print_stderr, reason = "contract diagnostics channel")]
fn emit(reserve: &mut FatalSerializer, envelope: &amiss_wire::json::Value) {
    if reserve.emit(envelope, &mut std::io::stdout()).is_err() {
        eprintln!(
            "amiss: {}",
            AnalysisErrorCode::ReportConstructionFailed.as_str()
        );
    }
}

struct View(Vec<(String, amiss_wire::json::Value)>);

impl View {
    fn of(value: Option<&amiss_wire::json::Value>) -> Self {
        use amiss_wire::json::Value;
        match value {
            Some(Value::Object(members)) => Self(members.clone()),
            Some(
                Value::Null
                | Value::Bool(_)
                | Value::Integer(_)
                | Value::String(_)
                | Value::Array(_),
            )
            | None => Self(Vec::new()),
        }
    }

    fn field(&self, name: &str) -> Option<&amiss_wire::json::Value> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }

    fn view(&self, name: &str) -> Self {
        Self::of(self.field(name))
    }

    fn text(&self, name: &str) -> String {
        use amiss_wire::json::Value;
        match self.field(name) {
            Some(Value::String(value)) => value.clone(),
            Some(
                Value::Null
                | Value::Bool(_)
                | Value::Integer(_)
                | Value::Array(_)
                | Value::Object(_),
            )
            | None => String::new(),
        }
    }

    fn atom_or_dash(&self, name: &str) -> String {
        use amiss_wire::json::Value;
        match self.field(name) {
            Some(Value::String(value)) => amiss_wire::human::atom(value),
            Some(Value::Object(members)) => match members.as_slice() {
                [(key, Value::String(hex))] if key == "bytes_hex" => {
                    amiss_wire::human::atom_bytes(&decode_hex(hex))
                }
                _ => "-".to_owned(),
            },
            Some(Value::Null | Value::Bool(_) | Value::Integer(_) | Value::Array(_)) | None => {
                "-".to_owned()
            }
        }
    }

    fn number(&self, name: &str) -> i64 {
        use amiss_wire::json::Value;
        match self.field(name) {
            Some(Value::Integer(value)) => *value,
            Some(
                Value::Null
                | Value::Bool(_)
                | Value::String(_)
                | Value::Array(_)
                | Value::Object(_),
            )
            | None => 0,
        }
    }

    fn rows(&self, name: &str) -> Vec<Self> {
        use amiss_wire::json::Value;
        match self.field(name) {
            Some(Value::Array(rows)) => rows.iter().map(|row| Self::of(Some(row))).collect(),
            Some(
                Value::Null
                | Value::Bool(_)
                | Value::Integer(_)
                | Value::String(_)
                | Value::Object(_),
            )
            | None => Vec::new(),
        }
    }
}

/// The human projection: a non-wire convenience over the same payload that
/// cannot change facts, ordering, totals, or exit. It prints the result, all
/// retained analysis errors, the first two hundred findings in canonical
/// order, and exact totals; every repository-derived scalar passes through
/// `human-atom-v1`, and no source excerpt, raw destination, or query value
/// appears.
#[expect(clippy::print_stdout, reason = "the human output channel")]
fn human(built: &amiss_scan::report::Built, explain_scope: bool) {
    let envelope = View::of(Some(&built.envelope));
    let payload = envelope.view("payload");
    let result = payload.view("result");
    println!(
        "amiss: {} (findings {}, errors {}, exit {})",
        built.status,
        result.number("finding_count"),
        result.number("error_count"),
        built.exit_code
    );
    if explain_scope {
        explain(&payload);
    }
    for row in payload.rows("errors") {
        println!(
            "error {} {} {}",
            row.text("phase"),
            row.text("code"),
            row.atom_or_dash("path")
        );
    }
    for finding in payload.rows("findings").iter().take(200) {
        println!(
            "{} {} {} {} x{}",
            finding.text("effective_disposition"),
            finding.text("kind"),
            finding.text("attribution"),
            finding.view("location").atom_or_dash("path"),
            finding.view("aggregation").number("member_count").max(1)
        );
    }
    totals(&payload);
}

#[expect(clippy::print_stdout, reason = "the human output channel")]
fn totals(payload: &View) {
    let summary = payload.view("summary");
    let truncated = summary.number("human_details_truncated");
    if truncated > 0 {
        println!("details truncated: {truncated}");
    }
    let documents = summary.view("documents");
    println!(
        "documents: discovered {} scanned {} unsupported {} excluded {} unlinked {}",
        documents.number("discovered"),
        documents.number("scanned"),
        documents.number("unsupported"),
        documents.number("excluded_builtin"),
        documents.number("unlinked"),
    );
    let references = summary.view("references");
    println!(
        "references: extracted {} local {} github {} external {} unsupported {} missing {}",
        references.number("extracted"),
        references.number("explicit_local"),
        references.number("same_repository_github"),
        references.number("external_out_of_scope"),
        references.number("unsupported"),
        references.number("missing"),
    );
    let findings = summary.view("findings");
    println!(
        "findings: total {} fail {} warn {} record {}",
        findings.number("total"),
        findings.number("fail"),
        findings.number("warn"),
        findings.number("record"),
    );
}

/// The deterministic scope explanation the human projection may add: the
/// closed built-in document classes and this run's discovered surface.
#[expect(clippy::print_stdout, reason = "the human output channel")]
fn explain(payload: &View) {
    println!("scope: built-in documents are *.md, *.mdx, *.markdown, six extensionless");
    println!("scope: basenames, and .cursorrules and llms.txt as plain advisory");
    println!("scope: node_modules, vendor, third_party, dist, build, .next, and target");
    println!("scope: trees are excluded unless a repository policy includes them");
    let documents = payload.view("summary").view("documents");
    println!(
        "scope: this run discovered {} candidate documents and scanned {}",
        documents.number("discovered"),
        documents.number("scanned"),
    );
}

fn exit_class(code: i64) -> ExitCode {
    match code {
        0 => ExitCode::from(ExitClass::Success.code()),
        1 => ExitCode::from(ExitClass::BlockingFindings.code()),
        _ => ExitCode::from(ExitClass::Failure.code()),
    }
}

fn engine_provenance() -> Option<EngineProvenance> {
    let exe = env::current_exe().ok()?;
    let bytes = fs::read(exe).ok()?;
    Some(EngineProvenance {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        digest: hb(report::ENGINE_DOMAIN, &bytes),
    })
}

fn analysis_code(code: Code) -> AnalysisErrorCode {
    match code {
        Code::InvalidEvent => AnalysisErrorCode::InvalidEvent,
        Code::InvalidInvocation => AnalysisErrorCode::InvalidInvocation,
        Code::InvalidProfile => AnalysisErrorCode::InvalidProfile,
        Code::UnsupportedProviderHost => AnalysisErrorCode::UnsupportedProviderHost,
    }
}
