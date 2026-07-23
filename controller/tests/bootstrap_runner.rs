#![expect(
    clippy::unwrap_used,
    reason = "integration fixtures construct known-valid controller inputs"
)]

use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use amiss_bootstrap::BOOTSTRAP_DOMAIN;
use amiss_controller::{
    BootstrapRun, ChangeId, ChangeLocator, ControllerEvaluationId, DeliveryId, DeliveryIdentity,
    Evaluation, HeartbeatOutcome, IntegrationId, OidPair, PolicyControls, ProviderIdentity,
    ProviderInstance, ProviderNamespace, ProviderRunAttempt, ProviderRunId, ProviderRunIdentity,
    RunHeartbeat, RunIdentity, RunRefs, RunRequest, RunnerOutcome, check_binding, check_plan,
    run_bootstrap,
};
use amiss_fixtures::{CommitPair, commit_pair, git};
use amiss_wire::controls::{ExecutionConstraintDescriptor, ExecutionConstraintInput, Profile};
use amiss_wire::digest::{Digest, hb};
use amiss_wire::model::{
    BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity, UtcInstant,
};

const PASS_REPORT: &[u8] = b"{\"runner\":\"pass\"}\n";
const BLOCK_REPORT: &[u8] = b"{\"runner\":\"block\"}\n";
const STARTED_MARKER: &str = "runner-started";
const RENEWAL_GATE: &str = "runner-renewal-gate";
const RESOURCE_RELEASE_TIMEOUT: Duration = Duration::from_secs(2);
const RESOURCE_RELEASE_POLL: Duration = Duration::from_millis(10);

#[path = "bootstrap_runner/acquiring.rs"]
mod acquiring;

struct Heartbeat {
    calls: u64,
    stop_on_call: Option<u64>,
    stop_delay: Duration,
    renew_within: Duration,
    release_when_present: Option<(PathBuf, File)>,
}

impl Heartbeat {
    const fn renewing() -> Self {
        Self::renewing_with(Duration::from_millis(200))
    }

    const fn renewing_with(renew_within: Duration) -> Self {
        Self {
            calls: 0,
            stop_on_call: None,
            stop_delay: Duration::ZERO,
            renew_within,
            release_when_present: None,
        }
    }

    fn stopping_on(call: u64) -> Self {
        Self {
            stop_on_call: Some(call),
            ..Self::renewing()
        }
    }

    fn stopping_after(call: u64, delay: Duration) -> Self {
        Self {
            stop_on_call: Some(call),
            stop_delay: delay,
            renew_within: Duration::from_millis(100),
            ..Self::renewing()
        }
    }

    fn releasing_when(path: PathBuf, renew_within: Duration, gate: File) -> Self {
        Self {
            release_when_present: Some((path, gate)),
            ..Self::renewing_with(renew_within)
        }
    }
}

impl RunHeartbeat for Heartbeat {
    fn renew(&mut self) -> HeartbeatOutcome {
        self.calls = self.calls.saturating_add(1);
        if let Some((_path, gate)) = self
            .release_when_present
            .take_if(|(path, _gate)| path.exists())
            && gate.unlock().is_err()
        {
            return HeartbeatOutcome::Stop;
        }
        if self.stop_on_call == Some(self.calls) {
            std::thread::sleep(self.stop_delay);
            HeartbeatOutcome::Stop
        } else {
            HeartbeatOutcome::Renewed {
                renew_within: self.renew_within,
            }
        }
    }
}

struct Harness {
    repository: CommitPair,
    action: CommitPair,
    scratch: tempfile::TempDir,
    executable: PathBuf,
    request: RunRequest,
    evaluation_instant: UtcInstant,
    valid_until: UtcInstant,
}

impl Harness {
    fn new(mode: &str, bootstrap_digest: Option<Digest>) -> Self {
        let repository =
            commit_pair(&[("README.md", "base\n")], &[("README.md", "candidate\n")]).unwrap();
        let action = commit_pair(
            &[("bootstrap", "release one\n")],
            &[("bootstrap", "release two\n")],
        )
        .unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let executable = PathBuf::from(env!("CARGO_BIN_EXE_amiss-bootstrap-fixture"));
        let digest = bootstrap_digest
            .unwrap_or_else(|| hb(BOOTSTRAP_DOMAIN, &std::fs::read(&executable).unwrap()));
        let request = request(&repository, &action, mode, digest);
        Self {
            repository,
            action,
            scratch,
            executable,
            request,
            evaluation_instant: instant("2026-07-22T20:00:00Z"),
            valid_until: instant("2026-07-22T20:05:00Z"),
        }
    }

    fn run(&self, wall_timeout: Duration, heartbeat: &mut Heartbeat) -> RunnerOutcome {
        self.run_in(self.scratch.path(), wall_timeout, heartbeat)
    }

    fn run_in(
        &self,
        scratch: &std::path::Path,
        wall_timeout: Duration,
        heartbeat: &mut Heartbeat,
    ) -> RunnerOutcome {
        run_bootstrap(
            &self.request,
            BootstrapRun {
                executable: &self.executable,
                repository: self.repository.root(),
                action_repository: self.action.root(),
                scratch,
                evaluation_instant: &self.evaluation_instant,
                valid_until: &self.valid_until,
                wall_timeout,
            },
            heartbeat,
        )
    }

    fn started(&self) -> bool {
        self.repository.root().join(STARTED_MARKER).exists()
    }

    fn escaped(&self) -> bool {
        self.repository.root().join("runner-escaped").exists()
    }

    fn ready(&self) -> bool {
        self.repository.root().join("runner-ready").exists()
    }

    fn descendant_resources_released(&self) -> bool {
        let started = Instant::now();
        loop {
            let released = OpenOptions::new()
                .read(true)
                .write(true)
                .open(self.repository.root().join("runner-lock"))
                .is_ok_and(|lock| lock.try_lock().is_ok());
            if released {
                return true;
            }
            if started.elapsed() >= RESOURCE_RELEASE_TIMEOUT {
                return false;
            }
            std::thread::sleep(RESOURCE_RELEASE_POLL);
        }
    }
}

fn oid(value: &str) -> Oid {
    Oid::new(ObjectFormat::Sha1, value.to_owned()).unwrap()
}

fn tree(pair: &CommitPair, commit: &str) -> Oid {
    let revision = format!("{commit}^{{tree}}");
    oid(git(pair.root(), &["rev-parse", &revision]).unwrap().trim())
}

fn repository_identity() -> RepositoryIdentity {
    RepositoryIdentity::new(
        "gitlab.example.internal".to_owned(),
        "platform/security".to_owned(),
        "docs".to_owned(),
    )
    .unwrap()
}

fn execution(
    action: &CommitPair,
    status: &str,
    bootstrap_digest: Digest,
) -> ExecutionConstraintDescriptor {
    let template = ExecutionConstraintDescriptor::parse(include_bytes!(
        "../../spec/examples/scanner-execution-constraint.json"
    ))
    .unwrap();
    let mut input = ExecutionConstraintInput::from(&template);
    input.action_commit_oid = oid(&action.candidate);
    input.action_tree_oid = tree(action, &action.candidate);
    status.clone_into(&mut input.required_status_name);
    input.bootstrap_digest = bootstrap_digest;
    ExecutionConstraintDescriptor::new(input).unwrap()
}

fn request(
    repository: &CommitPair,
    action: &CommitPair,
    status: &str,
    bootstrap_digest: Digest,
) -> RunRequest {
    let provider = ProviderIdentity {
        namespace: ProviderNamespace::new("gitlab".to_owned()).unwrap(),
        instance: ProviderInstance::new("gitlab.example.internal".to_owned()).unwrap(),
    };
    let plan = Arc::new(
        check_plan(
            Profile::Enforce,
            PolicyControls::default(),
            execution(action, status, bootstrap_digest),
        )
        .unwrap(),
    );
    RunRequest {
        delivery: DeliveryIdentity {
            provider: provider.clone(),
            integration: IntegrationId::new("project-hook/7".to_owned()).unwrap(),
            delivery: DeliveryId::new("webhook/9".to_owned()).unwrap(),
        },
        provider_run: ProviderRunIdentity::new(
            ProviderRunId::new("pipeline/987654321:job-42".to_owned()).unwrap(),
            ProviderRunAttempt::new(1).unwrap(),
            ObjectFormat::Sha1,
            oid(&repository.candidate),
        )
        .unwrap(),
        evaluation_id: ControllerEvaluationId::new("evaluation/11".to_owned()).unwrap(),
        check: check_binding(&plan).unwrap(),
        plan,
        run: RunIdentity::new(
            ChangeLocator {
                provider,
                repository: repository_identity(),
                change: ChangeId::new("merge-request/42".to_owned()).unwrap(),
            },
            RunRefs {
                forge: ForgeDialect::Gitlab,
                candidate: BranchRef::new("refs/heads/topic".to_owned()).unwrap(),
                target: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
                default_branch: BranchRef::new("refs/heads/main".to_owned()).unwrap(),
            },
            ObjectFormat::Sha1,
            OidPair {
                base: oid(&repository.base),
                candidate: oid(&repository.candidate),
            },
            OidPair {
                base: tree(repository, &repository.base),
                candidate: tree(repository, &repository.candidate),
            },
        )
        .unwrap(),
    }
}

fn instant(value: &str) -> UtcInstant {
    UtcInstant::new(value.to_owned()).unwrap()
}

fn run(mode: &str) -> (Harness, RunnerOutcome, Heartbeat) {
    let harness = Harness::new(mode, None);
    let mut heartbeat = Heartbeat::renewing();
    let outcome = harness.run(Duration::from_secs(2), &mut heartbeat);
    (harness, outcome, heartbeat)
}

#[test]
fn pass_and_block_preserve_the_authenticated_identity() {
    let cases = [
        ("runner-pass", Evaluation::Pass, PASS_REPORT),
        ("runner-block", Evaluation::Block, BLOCK_REPORT),
    ];
    for (mode, evaluation, report) in cases {
        let (harness, outcome, heartbeat) = run(mode);
        assert_eq!(
            outcome,
            RunnerOutcome::Complete {
                identity: Box::new(harness.request.run.clone()),
                evaluation,
                report: report.to_vec(),
            }
        );
        assert!(harness.started());
        assert_eq!(heartbeat.calls, 1);
    }
}

#[test]
fn missing_and_malformed_result_records_fail_closed() {
    let cases = [
        ("runner-missing", RunnerOutcome::MissingOutput),
        ("runner-malformed", RunnerOutcome::TamperedRuntime),
    ];
    for (mode, expected) in cases {
        let (harness, outcome, _heartbeat) = run(mode);
        assert_eq!(outcome, expected);
        assert!(harness.started());
    }
}

#[test]
fn replaced_output_paths_cannot_replace_the_controller_handles() {
    let (harness, outcome, _heartbeat) = run("runner-replace-outputs");

    assert_eq!(outcome, RunnerOutcome::MissingOutput);
    assert!(harness.started());
    assert!(harness.repository.root().join("runner-replaced").exists());
}

#[test]
fn an_oversized_report_is_bounded_and_never_accepted() {
    let harness = Harness::new("runner-oversized", None);
    let mut heartbeat = Heartbeat::renewing();
    let outcome = harness.run(Duration::from_secs(20), &mut heartbeat);
    assert_eq!(outcome, RunnerOutcome::OversizedOutput);
    assert!(harness.started());
}

#[test]
fn a_changed_bootstrap_is_rejected_before_launch() {
    let harness = Harness::new("runner-pass", Some(hb("wrong-bootstrap", b"wrong")));
    let mut heartbeat = Heartbeat::renewing();

    assert_eq!(
        harness.run(Duration::from_secs(2), &mut heartbeat),
        RunnerOutcome::TamperedRuntime
    );
    assert!(!harness.started());
    assert_eq!(heartbeat.calls, 0);
}

#[test]
fn bootstrap_receives_no_inherited_environment() {
    let (harness, outcome, _heartbeat) = run("runner-environment");
    assert!(matches!(outcome, RunnerOutcome::Complete { .. }));
    assert!(harness.started());
}

#[test]
fn equivalent_scratch_path_spelling_is_accepted() {
    let harness = Harness::new("runner-pass", None);
    let component = harness.scratch.path().join("component");
    std::fs::create_dir(&component).unwrap();
    let scratch = component.join("..");
    let mut heartbeat = Heartbeat::renewing();

    assert!(matches!(
        harness.run_in(&scratch, Duration::from_secs(2), &mut heartbeat),
        RunnerOutcome::Complete { .. }
    ));
    assert!(harness.started());
}

#[test]
fn short_lease_windows_drive_renewal_before_completion() {
    let harness = Harness::new("runner-renewed-pass", None);
    let gate = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(harness.repository.root().join(RENEWAL_GATE))
        .unwrap();
    gate.lock().unwrap();
    let started = harness.repository.root().join(STARTED_MARKER);
    let mut heartbeat = Heartbeat::releasing_when(started, Duration::from_millis(80), gate);

    assert!(matches!(
        harness.run(Duration::from_secs(2), &mut heartbeat),
        RunnerOutcome::Complete { .. }
    ));
    assert!(harness.started());
}

#[test]
fn an_empty_lease_window_stops_before_launch() {
    let harness = Harness::new("runner-pass", None);
    let mut heartbeat = Heartbeat::renewing_with(Duration::ZERO);

    assert_eq!(
        harness.run(Duration::from_secs(2), &mut heartbeat),
        RunnerOutcome::Unavailable
    );
    assert_eq!(heartbeat.calls, 1);
    assert!(!harness.started());
}

#[test]
fn invalid_time_bounds_stop_before_launch() {
    let harness = Harness::new("runner-pass", None);
    let mut heartbeat = Heartbeat::renewing();

    assert_eq!(
        harness.run(Duration::from_secs(121), &mut heartbeat),
        RunnerOutcome::Unavailable
    );
    assert!(!harness.started());
    assert_eq!(heartbeat.calls, 0);
}

#[test]
fn wall_timeout_stops_contained_descendants() {
    let harness = Harness::new("runner-hang", None);
    let mut heartbeat = Heartbeat::renewing();

    assert_eq!(
        harness.run(Duration::from_millis(300), &mut heartbeat),
        RunnerOutcome::TimedOut
    );
    assert!(harness.started());
    assert!(harness.ready());
    assert!(harness.descendant_resources_released());
    assert!(!harness.escaped());
}

#[test]
fn heartbeat_loss_stops_contained_descendants() {
    let harness = Harness::new("runner-hang", None);
    let mut heartbeat = Heartbeat::stopping_on(2);

    assert_eq!(
        harness.run(Duration::from_secs(2), &mut heartbeat),
        RunnerOutcome::Unavailable
    );
    assert_eq!(heartbeat.calls, 2);
    assert!(harness.started());
    assert!(harness.ready());
    assert!(harness.descendant_resources_released());
    assert!(!harness.escaped());
}

#[test]
fn heartbeat_loss_discards_a_completion_already_waiting() {
    let harness = Harness::new("runner-delayed-pass", None);
    let mut heartbeat = Heartbeat::stopping_after(2, Duration::from_millis(200));

    assert_eq!(
        harness.run(Duration::from_secs(2), &mut heartbeat),
        RunnerOutcome::Unavailable
    );
    assert_eq!(heartbeat.calls, 2);
    assert!(harness.started());
}

#[test]
fn leader_exit_stops_descendants_before_accepting_the_report() {
    let harness = Harness::new("runner-exit-child", None);
    let mut heartbeat = Heartbeat::renewing();
    let outcome = harness.run(Duration::from_millis(300), &mut heartbeat);
    assert_eq!(
        outcome,
        RunnerOutcome::Complete {
            identity: Box::new(harness.request.run.clone()),
            evaluation: Evaluation::Pass,
            report: PASS_REPORT.to_vec(),
        }
    );
    assert_eq!(heartbeat.calls, 1);
    assert!(harness.started());
    assert!(harness.ready());
    assert!(harness.descendant_resources_released());
    assert!(!harness.escaped());
}
