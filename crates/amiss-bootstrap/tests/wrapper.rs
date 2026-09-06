#![expect(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "end-to-end harness over asserted fixture shapes"
)]

#[path = "wrapper/semantic.rs"]
mod semantic;
mod support;

use std::ffi::OsStr;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};

use amiss_bootstrap::result::{BootstrapResult, parse_result};
use amiss_fixtures::CommitChain;
use amiss_fixtures::requests::SealedRequests;
use amiss_wire::controls::{
    ExecutionConstraintDescriptor, canonical_execution_constraint, canonical_trusted_time,
    parse_execution_constraint, parse_trusted_time,
};
use amiss_wire::digest::hb;
use amiss_wire::json::{Value, parse};
use amiss_wire::model::Oid;
use amiss_wire::report::PAYLOAD_SCHEMA;
use amiss_wire::requests::{
    REQUEST_STREAM_BYTES, SEALED_ENGINE_ARGUMENT, commit_candidate_identity_digest,
};

use support::release::{Release, release, release_with_engine};

const INSTANT: &str = "2026-07-12T10:00:00Z";
const VALID_UNTIL: &str = "2026-07-12T10:05:00Z";
const ABSENT_COMMIT: &str = "6666666666666666666666666666666666666666";

/// One binary, two roles: the scenario runner, and the engine the wrapper
/// launches from the validated tree when spawned with the sealed argument.
#[expect(
    clippy::print_stdout,
    reason = "the harness-free test protocol speaks on stdout"
)]
fn main() -> ExitCode {
    if std::env::args_os().nth(1).as_deref() == Some(OsStr::new(SEALED_ENGINE_ARGUMENT)) {
        return engine();
    }
    if std::env::args_os().any(|argument| argument == OsStr::new("--list")) {
        if !std::env::args_os().any(|argument| argument == OsStr::new("--ignored")) {
            println!("wrapper: test");
        }
        return ExitCode::SUCCESS;
    }
    let own = fs::read(std::env::current_exe().expect("own path")).expect("own bytes");
    let staged = release_with_engine(&own, |_root| {});
    let refused = release(|_root| {});
    pass_run(&staged);
    block_run(&staged);
    absent_candidate(&refused);
    silent_engine(&staged);
    garbage_engine(&staged);
    identity_absent(&refused);
    invalid_supplied_controls(&staged);
    semantic::capture(&staged);
    wrong_result_name(&refused);
    #[cfg(unix)]
    symlinked_scratch(&refused);
    request_ceiling(&staged);
    unread_requests(&staged);
    println!("wrapper: every scenario held");
    ExitCode::SUCCESS
}

/// The engine role: drain stdin unless told not to, replay the planted
/// stdout, exit with the planted class. The plants live in the repository
/// the wrapper set as the working directory.
fn engine() -> ExitCode {
    if !Path::new("engine-skip-stdin").exists() {
        let mut sink = Vec::new();
        let _drained = std::io::stdin().lock().read_to_end(&mut sink);
    }
    let planted = fs::read("engine-stdout").unwrap_or_default();
    let mut stdout = std::io::stdout().lock();
    if stdout
        .write_all(&planted)
        .and_then(|()| stdout.flush())
        .is_err()
    {
        return ExitCode::from(9);
    }
    fs::read_to_string("engine-exit")
        .ok()
        .and_then(|raw| raw.trim().parse::<u8>().ok())
        .map_or(ExitCode::from(9), ExitCode::from)
}

fn wrapper_constraint(staged: &Release) -> ExecutionConstraintDescriptor {
    let own = fs::read(env!("CARGO_BIN_EXE_amiss-bootstrap")).unwrap();
    let raw = format!(
        concat!(
            r#"{{"schema":"amiss/scanner-execution-constraint","action_repository":"#,
            r#"{{"host":"git.example.internal","owner":"platform/security","name":"amiss"}},"#,
            r#""action_object_format":"sha1","action_commit_oid":"{commit}","#,
            r#""action_tree_oid":"{tree}","manifest_path":"release-manifest.json","#,
            r#""release_manifest_digest":"{manifest}","selected_platform":"{platform}","#,
            r#""required_status_name":"amiss / assure","#,
            r#""bootstrap_contract":"amiss-action-bootstrap","bootstrap_digest":"{bootstrap}"}}"#,
        ),
        commit = staged.commit,
        tree = staged.tree,
        manifest = staged.manifest_digest,
        platform = staged.platform.as_ref(),
        bootstrap = hb(amiss_bootstrap::BOOTSTRAP_DOMAIN, &own),
    );
    parse_execution_constraint(raw.as_bytes()).unwrap()
}

fn entry<'value>(value: &'value mut Value, key: &str) -> &'value mut Value {
    let Value::Object(members) = value else {
        panic!("not an object");
    };
    members
        .iter_mut()
        .find(|(name, _)| name == key)
        .map(|(_, member)| member)
        .expect("a present member")
}

fn set(value: &mut Value, key: &str, member: Value) {
    let Value::Object(members) = value else {
        panic!("not an object");
    };
    if let Some(slot) = members.iter_mut().find(|(name, _)| name == key) {
        slot.1 = member;
        return;
    }
    let at = members
        .iter()
        .position(|(name, _)| name.as_str() > key)
        .unwrap_or(members.len());
    let mut expanded = std::mem::take(members).into_vec();
    expanded.insert(at, (key.to_owned(), member));
    *members = expanded.into_boxed_slice();
}

fn string(raw: &str) -> Value {
    Value::string(raw)
}

/// One run the wrapper can settle end to end: a pre-acquired repository, a
/// request triple bound to it, and the envelope the engine will replay.
struct Run {
    repository: CommitChain,
    requests: SealedRequests,
    wire: Vec<u8>,
}

fn commit(chain: &CommitChain, position: usize) -> &amiss_fixtures::Commit {
    chain.commits.get(position).expect("a fixture commit")
}

fn chain_trees(chain: &CommitChain) -> (String, String) {
    (commit(chain, 0).tree.clone(), commit(chain, 1).tree.clone())
}

fn sealed_run(staged: &Release) -> Run {
    let repository = amiss_fixtures::commit_chain(&[
        ("base", &[("doc.md", "# base\n")]),
        ("candidate", &[("doc.md", "# candidate\n")]),
    ])
    .expect("a commit chain");
    let mut requests = SealedRequests::new(wrapper_constraint(staged));
    let format = requests.evaluation.object_format;
    requests.evaluation.base_commit = Oid::new(format, commit(&repository, 0).id.clone()).unwrap();
    requests.evaluation.candidate_commit =
        Some(Oid::new(format, commit(&repository, 1).id.clone()).unwrap());
    let trees = chain_trees(&repository);
    let wire = bind_envelope(staged, &mut requests, &trees, 0);
    Run {
        repository,
        requests,
        wire,
    }
}

/// Rebinds the supplied trusted-time statement to the run's identity and
/// returns the bound statement with its digest.
fn bind_statement(
    requests: &mut SealedRequests,
    repository_value: &Value,
    identity: &str,
) -> (Value, String) {
    let target = requests
        .evaluation
        .target_ref
        .as_ref()
        .expect("a target")
        .as_str()
        .to_owned();
    let time = requests
        .controls
        .trusted_time
        .as_mut()
        .expect("supplied time");
    let mut statement = Value::object(Vec::new());
    set(
        &mut statement,
        "schema",
        string("amiss/scanner-trusted-time-statement"),
    );
    set(
        &mut statement,
        "controller",
        string("external-required-check-clock"),
    );
    set(&mut statement, "repository", repository_value.clone());
    set(&mut statement, "ref", string(&target));
    set(
        &mut statement,
        "candidate_identity_digest",
        string(identity),
    );
    set(&mut statement, "provider", string(&time.provider));
    set(
        &mut statement,
        "provider_run_id",
        string(&time.provider_run_id),
    );
    set(
        &mut statement,
        "provider_run_attempt",
        Value::Integer(i64::try_from(time.provider_run_attempt).unwrap()),
    );
    set(&mut statement, "evaluation_instant", string(INSTANT));
    set(&mut statement, "valid_until", string(VALID_UNTIL));
    let parsed = parse_trusted_time(&serde_json_canonicalizer::to_vec(&statement).unwrap())
        .expect("a valid statement fixture");
    let (_, digest) = canonical_trusted_time(&parsed).unwrap();
    time.expected_digest = digest;
    time.value = serde_json::to_value(&statement).expect("a JSON statement");
    (statement, digest.to_string())
}

/// Builds the envelope the engine must print for the wrapper to accept it.
/// The identity digest is computed by the wire crate from the request and
/// the trees, and the envelope mirrors exactly the members that digest
/// covers.
fn bind_envelope(
    staged: &Release,
    requests: &mut SealedRequests,
    trees: &(String, String),
    exit_class: i64,
) -> Vec<u8> {
    let format = requests.evaluation.object_format;
    let base_tree = Oid::new(format, trees.0.clone()).unwrap();
    let candidate_tree = Oid::new(format, trees.1.clone()).unwrap();
    let identity =
        commit_candidate_identity_digest(&requests.evaluation, &base_tree, &candidate_tree)
            .expect("a commit-pair identity")
            .to_string();

    let identity_repository = requests
        .evaluation
        .repository
        .as_ref()
        .expect("an identity");
    let mut repository_value = Value::object(Vec::new());
    set(
        &mut repository_value,
        "host",
        string(identity_repository.host()),
    );
    set(
        &mut repository_value,
        "owner",
        string(identity_repository.owner()),
    );
    set(
        &mut repository_value,
        "name",
        string(identity_repository.name()),
    );

    let (statement, statement_digest) = bind_statement(requests, &repository_value, &identity);

    let mut envelope = example_envelope();
    let payload = entry(&mut envelope, "payload");
    set(
        entry(payload, "engine"),
        "engine_digest",
        string(&staged.engine_digest.to_string()),
    );
    patch_evaluation(payload, requests, &repository_value, trees);
    patch_controls(payload, requests, statement, &statement_digest);
    set(
        entry(payload, "result"),
        "exit_code",
        Value::Integer(exit_class),
    );
    if exit_class == 1 {
        set(entry(payload, "result"), "status", string("block"));
    }
    let digest = hb(
        PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(entry(&mut envelope, "payload")).unwrap(),
    )
    .to_string();
    set(&mut envelope, "payload_digest", string(&digest));
    let mut wire = serde_json_canonicalizer::to_vec(&envelope).unwrap();
    wire.push(b'\n');
    wire
}

fn patch_evaluation(
    payload: &mut Value,
    requests: &SealedRequests,
    repository_value: &Value,
    trees: &(String, String),
) {
    let request = &requests.evaluation;
    let evaluation = entry(payload, "evaluation");
    set(evaluation, "repository", repository_value.clone());
    set(evaluation, "forge", string("gitlab"));
    for (member, reference) in [
        ("candidate_ref", &request.candidate_ref),
        ("target_ref", &request.target_ref),
        ("default_branch_ref", &request.default_branch_ref),
    ] {
        set(
            evaluation,
            member,
            string(reference.as_ref().expect("an identity ref").as_str()),
        );
    }
    let candidate = request.candidate_commit.as_ref().expect("a candidate");
    for (member, commit, tree) in [
        ("base", &request.base_commit, &trees.0),
        ("candidate", candidate, &trees.1),
    ] {
        let snapshot = entry(evaluation, member);
        set(snapshot, "commit_oid", string(commit.as_str()));
        set(snapshot, "tree_oid", string(tree));
    }
    set(evaluation, "evaluation_instant", string(INSTANT));
    set(evaluation, "trusted_time", Value::Bool(true));
}

fn patch_controls(
    payload: &mut Value,
    requests: &SealedRequests,
    statement: Value,
    statement_digest: &str,
) {
    let floor = requests
        .controls
        .organization_floor
        .as_ref()
        .expect("a floor");
    let floor_digest = floor.expected_digest.to_string();
    let floor_source = floor.trust_source.as_ref().to_owned();
    let supplied = requests
        .controls
        .execution_constraint
        .as_ref()
        .expect("a constraint");
    let constraint_source = supplied.trust_source.as_ref().to_owned();
    let constraint_value = parse(&serde_json::to_vec(&supplied.value).expect("constraint JSON"))
        .expect("a constraint value");
    let constraint_digest = canonical_execution_constraint(&requests.constraint)
        .unwrap()
        .1
        .to_string();

    let controls = entry(payload, "controls");
    set(controls, "profile", string("enforce"));
    let mut floor_echo = Value::object(Vec::new());
    set(&mut floor_echo, "status", string("verified"));
    set(&mut floor_echo, "digest", string(&floor_digest));
    set(&mut floor_echo, "trust_source", string(&floor_source));
    set(controls, "organization_floor", floor_echo);
    let mut constraint_echo = Value::object(Vec::new());
    set(&mut constraint_echo, "status", string("verified"));
    set(&mut constraint_echo, "descriptor", constraint_value);
    set(
        &mut constraint_echo,
        "descriptor_digest",
        string(&constraint_digest),
    );
    set(
        &mut constraint_echo,
        "trust_source",
        string(&constraint_source),
    );
    set(controls, "execution_constraint", constraint_echo);
    let mut time_echo = Value::object(Vec::new());
    set(&mut time_echo, "status", string("verified"));
    set(
        &mut time_echo,
        "trust_source",
        string("external-required-check"),
    );
    set(&mut time_echo, "statement", statement);
    set(&mut time_echo, "statement_digest", string(statement_digest));
    set(controls, "trusted_time_source", time_echo);
}

fn example_envelope() -> Value {
    let bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../spec/examples")
            .join("scanner-report.json"),
    )
    .unwrap();
    parse(&bytes).unwrap()
}

struct Invocation {
    output: Output,
    report: PathBuf,
    result: PathBuf,
    _scratch: tempfile::TempDir,
}

fn invoke(staged: &Release, run: &Run, result_name: &str, scratch_link: bool) -> Invocation {
    let scratch = tempfile::tempdir().expect("a scratch root");
    let paths = run.requests.write(scratch.path());
    let report = scratch.path().join("report");
    let result = scratch.path().join(result_name);
    fs::write(&report, b"").unwrap();
    fs::write(&result, b"").unwrap();
    let scratch_argument = if scratch_link {
        let link = scratch.path().with_extension("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(scratch.path(), &link).expect("a scratch symlink");
        link
    } else {
        scratch.path().to_path_buf()
    };
    let report_argument = scratch_argument.join("report");
    let result_argument = scratch_argument.join(result_name);
    let output = Command::new(env!("CARGO_BIN_EXE_amiss-bootstrap"))
        .arg("exec")
        .arg("--action-repository")
        .arg(staged.dir.path())
        .arg("--repository")
        .arg(run.repository.root())
        .arg("--constraint")
        .arg(&paths.constraint)
        .arg("--evaluation-request")
        .arg(&paths.evaluation)
        .arg("--snapshot-request")
        .arg(&paths.snapshot)
        .arg("--controls-request")
        .arg(&paths.controls)
        .arg("--scratch")
        .arg(&scratch_argument)
        .arg("--report")
        .arg(&report_argument)
        .arg("--result")
        .arg(&result_argument)
        .output()
        .expect("the wrapper runs");
    if scratch_link {
        let _removed = fs::remove_file(scratch_argument);
    }
    Invocation {
        output,
        report,
        result,
        _scratch: scratch,
    }
}

fn plant(run: &Run, stdout: &[u8], exit: &str) {
    fs::write(run.repository.root().join("engine-stdout"), stdout).unwrap();
    fs::write(run.repository.root().join("engine-exit"), exit).unwrap();
}

fn settled(invocation: &Invocation) -> Option<BootstrapResult> {
    parse_result(&fs::read(&invocation.result).unwrap())
}

fn stderr_names(invocation: &Invocation, diagnostic: &str, scenario: &str) {
    let stderr = String::from_utf8_lossy(&invocation.output.stderr);
    assert!(
        stderr.contains(diagnostic),
        "{scenario}: stderr {stderr:?} does not name {diagnostic}"
    );
}

fn pass_run(staged: &Release) {
    let run = sealed_run(staged);
    plant(&run, &run.wire, "0");
    let invocation = invoke(staged, &run, "result", false);
    assert_eq!(
        invocation.output.status.code(),
        Some(0),
        "a pass run exits zero"
    );
    assert_eq!(settled(&invocation), Some(BootstrapResult::Pass));
    assert_eq!(
        fs::read(&invocation.report).unwrap(),
        run.wire,
        "the published report is the accepted envelope"
    );
    assert!(
        invocation.output.stderr.is_empty(),
        "a pass run needs no diagnostic"
    );
}

fn block_run(staged: &Release) {
    let mut run = sealed_run(staged);
    let trees = chain_trees(&run.repository);
    run.wire = bind_envelope(staged, &mut run.requests, &trees, 1);
    plant(&run, &run.wire, "1");
    let invocation = invoke(staged, &run, "result", false);
    assert_eq!(
        invocation.output.status.code(),
        Some(1),
        "a block run exits one"
    );
    assert_eq!(settled(&invocation), Some(BootstrapResult::Block));
}

fn absent_candidate(staged: &Release) {
    let mut run = sealed_run(staged);
    let format = run.requests.evaluation.object_format;
    run.requests.evaluation.candidate_commit =
        Some(Oid::new(format, ABSENT_COMMIT.to_owned()).unwrap());
    let trees = chain_trees(&run.repository);
    run.wire = bind_envelope(staged, &mut run.requests, &trees, 0);
    plant(&run, &run.wire, "0");
    let invocation = invoke(staged, &run, "result", false);
    assert_eq!(invocation.output.status.code(), Some(2));
    assert_eq!(settled(&invocation), Some(BootstrapResult::Unavailable));
    stderr_names(
        &invocation,
        "repository-not-pre-acquired",
        "absent candidate",
    );
}

fn silent_engine(staged: &Release) {
    let run = sealed_run(staged);
    plant(&run, b"", "0");
    let invocation = invoke(staged, &run, "result", false);
    assert_eq!(settled(&invocation), Some(BootstrapResult::MissingOutput));
    stderr_names(&invocation, "report-missing", "silent engine");
}

fn garbage_engine(staged: &Release) {
    let run = sealed_run(staged);
    plant(&run, b"not an envelope\n", "0");
    let invocation = invoke(staged, &run, "result", false);
    assert_eq!(settled(&invocation), Some(BootstrapResult::TamperedRuntime));
    stderr_names(&invocation, "report-rejected", "garbage engine");
}

/// The request grammar makes the four identity fields all-or-nothing, so the
/// only sealed-identity gap a parsed request can carry is an absent forge.
fn identity_absent(staged: &Release) {
    let mut run = sealed_run(staged);
    run.requests.evaluation.forge = None;
    plant(&run, &run.wire, "0");
    let invocation = invoke(staged, &run, "result", false);
    assert_eq!(settled(&invocation), Some(BootstrapResult::TamperedRuntime));
    stderr_names(&invocation, "evaluation-identity-absent", "absent forge");
}

fn invalid_supplied_controls(staged: &Release) {
    for (constraint, field, value, diagnostic) in [
        (
            true,
            "required_status_name",
            " bad",
            "execution-constraint-invalid",
        ),
        (false, "provider", "bad provider!", "trusted-time-invalid"),
        (false, "valid_until", INSTANT, "trusted-time-invalid"),
    ] {
        let mut run = sealed_run(staged);
        let controls = &mut run.requests.controls;
        let supplied = if constraint {
            &mut controls.execution_constraint.as_mut().unwrap().value
        } else {
            &mut controls.trusted_time.as_mut().unwrap().value
        };
        supplied[field] = serde_json::json!(value);
        plant(&run, &run.wire, "0");
        let invocation = invoke(staged, &run, "result", false);
        assert_eq!(invocation.output.status.code(), Some(2), "{field}");
        assert_eq!(settled(&invocation), Some(BootstrapResult::TamperedRuntime));
        assert!(fs::read(&invocation.report).unwrap().is_empty(), "{field}");
        stderr_names(&invocation, diagnostic, field);
    }
}

fn invalid_invocation_writes_nothing(
    staged: &Release,
    result_name: &str,
    scratch_link: bool,
    scenario: &str,
) {
    let run = sealed_run(staged);
    plant(&run, &run.wire, "0");
    let invocation = invoke(staged, &run, result_name, scratch_link);
    assert_eq!(invocation.output.status.code(), Some(2), "{scenario}");
    stderr_names(&invocation, "invalid-invocation", scenario);
    assert!(
        fs::read(&invocation.result).unwrap().is_empty(),
        "{scenario}: the result is never written"
    );
    assert!(
        fs::read(&invocation.report).unwrap().is_empty(),
        "{scenario}"
    );
}

fn wrong_result_name(staged: &Release) {
    invalid_invocation_writes_nothing(staged, "result2", false, "wrong result name");
}

#[cfg(unix)]
fn symlinked_scratch(staged: &Release) {
    invalid_invocation_writes_nothing(staged, "result", true, "symlinked scratch");
}

/// Grows the opaque organization-floor value until the canonical controls
/// request is exactly `target` bytes long.
fn inflate_controls(run: &mut Run, target: u64) {
    let padded = |length: usize| serde_json::json!({ "padding": "x".repeat(length) });
    let floor = run
        .requests
        .controls
        .organization_floor
        .as_mut()
        .expect("a floor");
    floor.value = padded(1024);
    let measured = u64::try_from(
        run.requests
            .controls
            .canonical_bytes()
            .expect("controls serialize")
            .len(),
    )
    .unwrap();
    let grow = usize::try_from(target.checked_sub(measured).expect("a growable request")).unwrap();
    let floor = run
        .requests
        .controls
        .organization_floor
        .as_mut()
        .expect("a floor");
    floor.value = padded(grow.checked_add(1024).expect("a bounded pad"));
    let sized = run
        .requests
        .controls
        .canonical_bytes()
        .expect("controls serialize");
    assert_eq!(u64::try_from(sized.len()).unwrap(), target);
}

fn request_ceiling(staged: &Release) {
    let mut run = sealed_run(staged);
    inflate_controls(&mut run, REQUEST_STREAM_BYTES);
    plant(&run, &run.wire, "0");
    let invocation = invoke(staged, &run, "result", false);
    assert_eq!(
        settled(&invocation),
        Some(BootstrapResult::Pass),
        "a controls request exactly at the stream ceiling is read whole"
    );

    let mut over = sealed_run(staged);
    inflate_controls(&mut over, REQUEST_STREAM_BYTES + 1);
    plant(&over, &over.wire, "0");
    let invocation = invoke(staged, &over, "result", false);
    assert_eq!(settled(&invocation), Some(BootstrapResult::TamperedRuntime));
    stderr_names(&invocation, "controls-request-invalid", "over the ceiling");
}

/// An engine that never reads its requests while they overflow the pipe
/// buffer fails the request writer, and a completed engine must not have
/// that failure forgiven.
fn unread_requests(staged: &Release) {
    let mut run = sealed_run(staged);
    inflate_controls(&mut run, REQUEST_STREAM_BYTES);
    plant(&run, &run.wire, "0");
    fs::write(run.repository.root().join("engine-skip-stdin"), b"").unwrap();
    let invocation = invoke(staged, &run, "result", false);
    assert_eq!(settled(&invocation), Some(BootstrapResult::Unavailable));
    stderr_names(&invocation, "engine-collection-failed", "unread requests");
}
