#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration harness over asserted fixture shapes"
)]

use std::fs;
use std::path::Path;
use std::process::Command;

use amiss_bootstrap::build::{StagedArtifact, StagedBuild, StagedFile, build_manifest};
use amiss_bootstrap::result::{BootstrapResult, parse_result};
use amiss_bootstrap::{Refusal, validate};
use amiss_fixtures::requests::{RequestPaths, SealedRequests, indent};
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_wire::action::host_platform;
use amiss_wire::controls::{
    ConstraintPlatform, ExecutionConstraintDescriptor, parse_execution_constraint,
};
use amiss_wire::digest::{Digest, hb, sha256};
use amiss_wire::json::{Value, parse as parse_json};
use amiss_wire::manifest::{RuntimeRole, canonical_release_manifest, parse_release_manifest};
use amiss_wire::model::ObjectFormat;
use amiss_wire::requests::SnapshotMaterialization;
use tempfile::TempDir;

mod support;

use amiss_fixtures::executable_bytes as engine_bytes;

use support::release::{ACTION, Release, release};

const BOOTSTRAP: &[u8] = b"the exact protected bootstrap bytes";

fn constraint(release: &Release) -> ExecutionConstraintDescriptor {
    let value = object(vec![
        ("schema", string("amiss/scanner-execution-constraint")),
        (
            "action_repository",
            object(vec![
                ("host", string("git.example.internal")),
                ("owner", string("platform/security")),
                ("name", string("amiss")),
            ]),
        ),
        ("action_object_format", string("sha1")),
        ("action_commit_oid", string(&release.commit)),
        ("action_tree_oid", string(&release.tree)),
        ("manifest_path", string("release-manifest.json")),
        (
            "release_manifest_digest",
            string(&release.manifest_digest.to_string()),
        ),
        ("selected_platform", string(release.platform.as_ref())),
        ("required_status_name", string("amiss / assure")),
        ("bootstrap_contract", string("amiss-action-bootstrap")),
        (
            "bootstrap_digest",
            string(&hb(amiss_bootstrap::BOOTSTRAP_DOMAIN, BOOTSTRAP).to_string()),
        ),
    ]);
    parse_execution_constraint(&serde_json_canonicalizer::to_vec(&value).unwrap())
        .expect("the constraint parses")
}

fn attempt(release: &Release, bootstrap: &[u8]) -> Result<amiss_bootstrap::Validated, Refusal> {
    let repo = Repository::open(release.dir.path(), ObjectFormat::Sha1).expect("open action tree");
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    validate(&repo, &mut resources, &constraint(release), bootstrap)
}

fn string(text: &str) -> Value {
    Value::string(text)
}

fn object(members: Vec<(&str, Value)>) -> Value {
    Value::object(
        members
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

#[test]
fn the_pinned_release_validates_end_to_end() {
    let release = release(|_root| {});
    let validated = attempt(&release, BOOTSTRAP).expect("the staged release validates");
    assert_eq!(validated.platform, release.platform);
    assert_eq!(validated.engine_digest, release.engine_digest);
    assert_eq!(validated.manifest.engine_version, "0.1.0-experimental");
    assert_eq!(
        validated.manifest.build_source.repository.owner(),
        "platform/security"
    );
    assert_eq!(validated.artifact.runtime_files.len(), 2);
    assert!(
        validated.artifact.runtime_files.iter().any(|file| {
            file.role == RuntimeRole::RuntimeData && file.path.as_str() == "action.yml"
        }),
        "the runnable action definition is a pinned closure row"
    );
    assert_eq!(
        validated.manifest.dependency_lock.files.len(),
        1,
        "the lock set carries every build lockfile"
    );
}

#[test]
fn the_generated_manifest_reparses_to_its_pinned_digest() {
    let release = release(|_root| {});
    let bytes = fs::read(release.dir.path().join("release-manifest.json")).unwrap();
    assert_eq!(bytes.last(), Some(&b'\n'), "the manifest blob ends in LF");
    let parsed = parse_release_manifest(&bytes).expect("the generated manifest parses");
    assert_eq!(
        canonical_release_manifest(&parsed).unwrap().1,
        release.manifest_digest
    );
    assert_eq!(
        serde_json_canonicalizer::to_vec(
            &amiss_wire::json::parse(bytes.strip_suffix(b"\n").unwrap()).unwrap()
        )
        .unwrap(),
        bytes.strip_suffix(b"\n").unwrap(),
        "the manifest blob is exactly its own canonicalization"
    );
}

#[test]
fn a_bootstrap_whose_bytes_differ_refuses_before_anything_else() {
    let release = release(|_root| {});
    let outcome = attempt(&release, b"a different bootstrap binary");
    assert_eq!(
        outcome.err(),
        Some(Refusal::Tampered("bootstrap-digest-mismatch"))
    );
}

/// A file that is a symlink, which only a privileged Windows process can
/// create. The directory sides of the same law run on every platform, in
/// `amiss-git`'s `boundary.rs`.
#[cfg(unix)]
#[test]
fn a_symlinked_engine_path_refuses() {
    let release = release(|root| {
        let platform = host_platform().unwrap();
        let staged = root.join(format!("dist/amiss-{}", platform.as_ref()));
        fs::remove_file(&staged).unwrap();
        std::os::unix::fs::symlink("../Cargo.lock", &staged).unwrap();
    });
    let outcome = attempt(&release, BOOTSTRAP);
    assert_eq!(
        outcome.err(),
        Some(Refusal::Tampered("path-not-regular-blob")),
        "a symlink at the artifact path is never followed"
    );
}

fn edit_action_rows(value: &mut Value, edit: &impl Fn(&mut Value) -> bool) {
    match value {
        Value::Array(items) => {
            let mut retained = std::mem::take(items).into_vec();
            retained.retain_mut(|item| !is_action_row(item) || edit(item));
            for item in &mut retained {
                edit_action_rows(item, edit);
            }
            *items = retained.into_boxed_slice();
        }
        Value::Object(members) => {
            for (_key, member) in members.iter_mut() {
                edit_action_rows(member, edit);
            }
        }
        Value::Null | Value::Bool(_) | Value::Integer(_) | Value::String(_) => {}
    }
}

fn with_rewritten_manifest(transform: impl Fn(&mut Value), also: impl FnOnce(&Path)) -> Release {
    let mut digested: Option<Digest> = None;
    let mut release = release(|root| {
        let path = root.join("release-manifest.json");
        let bytes = fs::read(&path).unwrap();
        let mut value = parse_json(bytes.strip_suffix(b"\n").expect("the manifest ends in LF"))
            .expect("the manifest parses");
        transform(&mut value);
        digested = Some(hb(
            amiss_wire::manifest::MANIFEST_DOMAIN,
            &serde_json_canonicalizer::to_vec(&value).unwrap(),
        ));
        let mut out = serde_json_canonicalizer::to_vec(&value).unwrap();
        out.push(b'\n');
        fs::write(&path, out).unwrap();
        also(root);
    });
    release.manifest_digest = digested.expect("the rewritten manifest was digested");
    release
}

fn is_action_row(value: &Value) -> bool {
    let Value::Object(members) = value else {
        return false;
    };
    members.iter().any(|(key, member)| {
        key == "role" && matches!(member, Value::String(role) if role.as_ref() == "runtime-data")
    })
}

/// The action definition is what a `uses:` workflow actually executes, so a
/// manifest whose closure fails to pin it must refuse even when its digest
/// is self-consistent: otherwise the one runnable file at the tree root is
/// the one file nothing checks.
#[test]
fn a_manifest_that_omits_the_action_row_is_refused() {
    let release =
        with_rewritten_manifest(|value| edit_action_rows(value, &|_row| false), |_root| {});

    let outcome = attempt(&release, BOOTSTRAP);
    assert_eq!(
        outcome.err(),
        Some(Refusal::Tampered("action-metadata-invalid")),
        "an unpinned action definition is a refusal, not a runnable file"
    );
}

/// Runtime data alone is not a pin: the closure row must sit at the action
/// metadata path, not merely exist somewhere in the artifact.
#[test]
fn runtime_data_off_the_action_path_is_not_a_pin() {
    let repathed = |row: &mut Value| {
        let Value::Object(members) = row else {
            return true;
        };
        for (key, member) in members.iter_mut() {
            if key == "path" {
                *member = Value::string("assets.yml");
            }
        }
        true
    };
    let release = with_rewritten_manifest(
        |value| edit_action_rows(value, &repathed),
        |root| fs::rename(root.join("action.yml"), root.join("assets.yml")).unwrap(),
    );

    let outcome = attempt(&release, BOOTSTRAP);
    assert_eq!(
        outcome.err(),
        Some(Refusal::Tampered("action-metadata-invalid")),
        "a data row off the action path pins nothing"
    );
}

#[test]
fn a_tampered_runtime_file_refuses_on_its_checksum() {
    let release = release(|root| {
        fs::write(root.join("action.yml"), b"# swapped after staging\n").unwrap();
    });
    let outcome = attempt(&release, BOOTSTRAP);
    assert_eq!(
        outcome.err(),
        Some(Refusal::Tampered("runtime-closure-mismatch"))
    );
}

#[test]
fn a_manifest_from_another_tree_refuses_on_its_digest() {
    let mut release = release(|_root| {});
    release.manifest_digest = hb("amiss/scanner-release-manifest", b"another tree");
    let outcome = attempt(&release, BOOTSTRAP);
    assert_eq!(
        outcome.err(),
        Some(Refusal::Tampered("manifest-digest-mismatch"))
    );
}

#[test]
fn an_engine_whose_header_names_another_platform_refuses() {
    let platform = host_platform().unwrap();
    let other = match platform {
        ConstraintPlatform::LinuxX8664
        | ConstraintPlatform::MacosX8664
        | ConstraintPlatform::MacosAarch64
        | ConstraintPlatform::WindowsX8664
        | ConstraintPlatform::WindowsAarch64 => ConstraintPlatform::LinuxAarch64,
        ConstraintPlatform::LinuxAarch64 => ConstraintPlatform::LinuxX8664,
    };
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    amiss_fixtures::init_repository(root).expect("initialize repository");

    let binary = engine_bytes(other);
    let lock = b"# Cargo.lock fixture\nversion = 4\n".to_vec();
    let binary_path = format!("dist/amiss-{}", platform.as_ref());
    let mut artifacts = vec![StagedArtifact {
        platform,
        artifact_name: format!("amiss-{}", platform.as_ref()),
        files: vec![
            StagedFile {
                path: binary_path.clone(),
                role: RuntimeRole::Executable,
                executable: true,
                bytes: &binary,
            },
            StagedFile {
                path: "action.yml".to_owned(),
                role: RuntimeRole::RuntimeData,
                executable: false,
                bytes: ACTION,
            },
        ],
    }];
    let build = StagedBuild {
        engine_version: "0.1.0-experimental".to_owned(),
        host: "git.example.internal".to_owned(),
        owner: "platform/security".to_owned(),
        repository: "amiss".to_owned(),
        object_format: "sha1",
        commit_oid: "a".repeat(40),
        locks: vec![("Cargo.lock".to_owned(), &lock)],
    };
    let (manifest_bytes, manifest_digest) = build_manifest(&build, &mut artifacts).unwrap();
    fs::create_dir_all(root.join("dist")).unwrap();
    fs::write(root.join("action.yml"), ACTION).unwrap();
    fs::write(root.join("release-manifest.json"), &manifest_bytes).unwrap();
    fs::write(root.join(&binary_path), &binary).unwrap();
    fs::write(root.join("Cargo.lock"), &lock).unwrap();
    let committed = amiss_fixtures::commit_worktree(root, &[&binary_path], "mismatched")
        .expect("commit release");

    let release = Release {
        commit: committed.id,
        tree: committed.tree,
        dir,
        manifest_digest,
        engine_digest: hb(amiss_bootstrap::ENGINE_DOMAIN, &binary),
        platform,
    };
    let outcome = attempt(&release, BOOTSTRAP);
    assert_eq!(
        outcome.err(),
        Some(Refusal::Tampered("platform-binding-mismatch")),
        "the target comes from the executable header, not the manifest label"
    );
}

/// The execution constraint the required workflow protects pins the exact action
/// commit and the exact tree that commit must carry. The bootstrap resolves the
/// commit and refuses unless its tree is the pinned one, which is what stops a
/// verified workflow from being pointed at a commit whose tree was swapped under
/// it. Two ways to miss: a constraint whose tree OID is not the commit's real
/// tree, and a constraint whose commit OID names no object at all. Both are one
/// refusal, `ActionTree`, because both mean the pinned action is not the action
/// on disk.
#[test]
fn a_constraint_whose_commit_or_tree_does_not_match_refuses_on_the_action_tree() {
    let mut with_wrong_tree = release(|_root| {});
    assert_ne!(
        with_wrong_tree.tree,
        "b".repeat(40),
        "the bogus tree is not the real one"
    );
    with_wrong_tree.tree = "b".repeat(40);
    assert_eq!(
        attempt(&with_wrong_tree, BOOTSTRAP).err(),
        Some(Refusal::Tampered("action-tree-mismatch")),
        "the commit is real, but its tree is not the one the constraint pinned"
    );

    let mut with_absent_commit = release(|_root| {});
    with_absent_commit.commit = "c".repeat(40);
    assert_eq!(
        attempt(&with_absent_commit, BOOTSTRAP).err(),
        Some(Refusal::Tampered("action-tree-mismatch")),
        "the constraint pins a commit the action repository does not hold"
    );
}

/// The manifest records every build lockfile by path and raw-byte digest, and
/// its parse binds that set to the set digest, so the recorded numbers cannot
/// disagree with each other. What nothing checked until now is the tree: the
/// shipped Cargo.lock could carry any bytes at all, and validation would echo
/// the manifest's story about it. The lockfile is not executed, but it is the
/// one file that says which dependencies built the engine, so a release whose
/// lock bytes drifted from their recorded digest refuses instead of validating.
#[test]
fn a_tampered_lockfile_refuses_on_its_recorded_digest() {
    let release = release(|root| {
        fs::write(
            root.join("Cargo.lock"),
            b"# a different lock\nversion = 4\n",
        )
        .unwrap();
    });
    assert_eq!(
        attempt(&release, BOOTSTRAP).err(),
        Some(Refusal::Tampered("dependency-lock-mismatch")),
        "the tree's lock bytes do not recompute to the manifest's digest"
    );
}

/// The absent twin: a release tree that dropped the lockfile entirely. The
/// path comes from the manifest, the resolution walks the pinned tree, and an
/// entry that is not there is the same refusal as any other path the closure
/// names and the tree cannot produce.
#[test]
fn a_release_missing_its_lockfile_refuses_on_the_path() {
    let release = release(|root| {
        fs::remove_file(root.join("Cargo.lock")).unwrap();
    });
    assert_eq!(
        attempt(&release, BOOTSTRAP).err(),
        Some(Refusal::Tampered("path-not-regular-blob")),
        "a lockfile the manifest records and the tree lacks is not a lockfile"
    );
}

/// The constraint has to name this exact binary, since the wrapper hashes
/// itself before it reads a request.
fn binary_constraint(staged: &Release) -> ExecutionConstraintDescriptor {
    named_constraint(staged, "amiss / assure")
}

fn named_constraint(staged: &Release, status: &str) -> ExecutionConstraintDescriptor {
    let own = fs::read(env!("CARGO_BIN_EXE_amiss-bootstrap")).unwrap();
    let value = parse_json(
        format!(
            r#"{{"schema":"amiss/scanner-execution-constraint","action_repository":{{"host":"git.example.internal","owner":"platform/security","name":"amiss"}},"action_object_format":"sha1","action_commit_oid":"{}","action_tree_oid":"{}","manifest_path":"release-manifest.json","release_manifest_digest":"{}","selected_platform":"{}","required_status_name":"{}","bootstrap_contract":"amiss-action-bootstrap","bootstrap_digest":"{}"}}"#,
            staged.commit,
            staged.tree,
            staged.manifest_digest,
            staged.platform.as_ref(),
            status,
            hb(amiss_bootstrap::BOOTSTRAP_DOMAIN, &own),
        )
        .as_bytes(),
    )
    .unwrap();
    parse_execution_constraint(&serde_json_canonicalizer::to_vec(&value).unwrap()).unwrap()
}

/// Runs the wrapper over one request triple and reports what it settled to.
/// Every case here is refused while the requests are read, so the engine the
/// wrapper would launch is never reached and needs no fixture.
fn settle(
    staged: &Release,
    requests: &SealedRequests,
    edit: impl FnOnce(&RequestPaths),
) -> Option<BootstrapResult> {
    let root = tempfile::tempdir().unwrap();
    let paths = requests.write(root.path());
    edit(&paths);
    let report = root.path().join("report");
    let result = root.path().join("result");
    fs::write(&report, b"").unwrap();
    fs::write(&result, b"").unwrap();
    Command::new(env!("CARGO_BIN_EXE_amiss-bootstrap"))
        .arg("exec")
        .arg("--action-repository")
        .arg(staged.dir.path())
        .arg("--repository")
        .arg(staged.dir.path())
        .arg("--constraint")
        .arg(&paths.constraint)
        .arg("--evaluation-request")
        .arg(&paths.evaluation)
        .arg("--snapshot-request")
        .arg(&paths.snapshot)
        .arg("--controls-request")
        .arg(&paths.controls)
        .arg("--scratch")
        .arg(root.path())
        .arg("--report")
        .arg(&report)
        .arg("--result")
        .arg(&result)
        .output()
        .unwrap();
    parse_result(&fs::read(result).unwrap())
}

fn refused(staged: &Release, requests: &SealedRequests) -> bool {
    settle(staged, requests, |_paths| {}) == Some(BootstrapResult::TamperedRuntime)
}

/// Each document has to be canonical on its own, so one non-canonical document
/// is refused whichever of the three carries it.
#[test]
fn a_noncanonical_request_is_refused_whichever_document_carries_it() {
    let staged = release(|_root| {});
    let requests = SealedRequests::new(binary_constraint(&staged));

    let picks: [fn(&RequestPaths); 3] = [
        |paths| indent(&paths.evaluation),
        |paths| indent(&paths.snapshot),
        |paths| indent(&paths.controls),
    ];
    for pick in picks {
        assert_eq!(
            settle(&staged, &requests, pick),
            Some(BootstrapResult::TamperedRuntime)
        );
    }
}

/// The evaluation and the snapshot parse alone, so their pairing law is checked
/// only here and a disagreement has to refuse rather than scan. The other half
/// of that law, a commit-pair evaluation carrying no candidate, cannot be built
/// through the wire type at all, so only a hand-written document reaches it.
#[test]
fn an_evaluation_and_snapshot_that_disagree_on_mode_are_refused() {
    let staged = release(|_root| {});

    let mut wrong_mode = SealedRequests::new(binary_constraint(&staged));
    wrong_mode.snapshot.materialization = SnapshotMaterialization::Index;

    assert!(refused(&staged, &wrong_mode));
}

/// The controls carry a constraint and the digest they claim for it, and the
/// host carries one of its own. All three have to be the same constraint.
#[test]
fn an_execution_constraint_that_disagrees_with_its_digest_or_the_host_is_refused() {
    let staged = release(|_root| {});

    let mut wrong_digest = SealedRequests::new(binary_constraint(&staged));
    wrong_digest
        .controls
        .execution_constraint
        .as_mut()
        .unwrap()
        .expected_digest = sha256(b"not the constraint");
    assert!(refused(&staged, &wrong_digest));

    let mut wrong_host = SealedRequests::new(binary_constraint(&staged));
    wrong_host.constraint = named_constraint(&staged, "amiss / other");
    assert!(refused(&staged, &wrong_host));
}

/// The trusted-time statement is bound by four facts at once, and the outer
/// three exist so a statement cannot be lifted from another run.
#[test]
fn a_trusted_time_statement_that_disagrees_on_any_bound_fact_is_refused() {
    let staged = release(|_root| {});

    let breaks: [fn(&mut SealedRequests); 4] = [
        |requests| {
            requests
                .controls
                .trusted_time
                .as_mut()
                .unwrap()
                .expected_digest = sha256(b"not the statement");
        },
        |requests| {
            requests.controls.trusted_time.as_mut().unwrap().provider = "github".to_owned();
        },
        |requests| {
            requests
                .controls
                .trusted_time
                .as_mut()
                .unwrap()
                .provider_run_id = "pipeline/1:job-1".to_owned();
        },
        |requests| {
            let time = requests.controls.trusted_time.as_mut().unwrap();
            time.provider_run_attempt = time.provider_run_attempt.saturating_add(1);
        },
    ];
    for break_one in breaks {
        let mut requests = SealedRequests::new(binary_constraint(&staged));
        break_one(&mut requests);
        assert!(refused(&staged, &requests));
    }
}
