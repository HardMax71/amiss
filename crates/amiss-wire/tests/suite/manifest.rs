use amiss_wire::controls::{ConstraintPlatform, GitMode};
use amiss_wire::de::ErrorKind;
use amiss_wire::digest::{Digest, hj};
use amiss_wire::json;
use amiss_wire::manifest::{
    BuildSource, DEPENDENCY_LOCK_DOMAIN, DependencyLockFile, DependencyLockInput,
    DependencyLockSchema, EnvironmentContract, ReleaseArtifact, ReleaseManifest,
    ReleaseManifestSchema, RuntimeContract, RuntimeFile, RuntimeRole, canonical_dependency_lock,
    canonical_release_manifest, parse_release_manifest,
};
use amiss_wire::model::{ArtifactId, ObjectFormat, Oid, RepoPathText, RepositoryIdentity};

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn digest(fill: char) -> Digest {
    let raw = format!("sha256:{}", fill.to_string().repeat(64));
    Digest::from_wire(&raw).expect("a wire digest")
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn row(path: &str, role: RuntimeRole, git_mode: GitMode, fill: char) -> RuntimeFile {
    RuntimeFile {
        path: RepoPathText::new(path.to_owned()).expect("a repo path"),
        role,
        git_mode,
        file_sha256: digest(fill),
    }
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn artifact(
    platform: ConstraintPlatform,
    name: &str,
    runtime_files: Vec<RuntimeFile>,
) -> ReleaseArtifact {
    ReleaseArtifact {
        platform,
        artifact_name: ArtifactId::new(name.to_owned()).expect("an artifact id"),
        tree_path: RepoPathText::new("dist/amiss".to_owned()).expect("a repo path"),
        binary_sha256: digest('1'),
        engine_digest: digest('2'),
        runtime_contract: RuntimeContract::Current,
        environment_contract: EnvironmentContract::Current,
        runtime_files,
    }
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn manifest(artifact: ReleaseArtifact) -> ReleaseManifest {
    let dependency_lock = DependencyLockInput {
        schema: DependencyLockSchema::Current,
        files: vec![DependencyLockFile {
            path: RepoPathText::new("Cargo.lock".to_owned()).expect("a repo path"),
            raw_digest: digest('4'),
        }],
    };
    let dependency_lock_digest = canonical_dependency_lock(&dependency_lock)
        .expect("a valid dependency lock")
        .1;
    ReleaseManifest {
        schema: ReleaseManifestSchema::Current,
        engine_version: "0.5.1".to_owned(),
        build_source: BuildSource {
            repository: RepositoryIdentity::new(
                "github.com".to_owned(),
                "hardmax71".to_owned(),
                "amiss".to_owned(),
            )
            .expect("a repository identity"),
            object_format: ObjectFormat::Sha1,
            commit_oid: Oid::new(ObjectFormat::Sha1, "a".repeat(40)).expect("an oid"),
        },
        dependency_lock,
        dependency_lock_digest,
        artifacts: vec![artifact],
    }
}

fn closed() -> Vec<RuntimeFile> {
    vec![row(
        "dist/amiss",
        RuntimeRole::Executable,
        GitMode::ExecutableFile,
        '1',
    )]
}

#[test]
fn the_executable_row_holds_every_clause_of_the_closure_law() {
    let sound = artifact(
        ConstraintPlatform::LinuxX8664,
        "amiss-linux-x86_64",
        closed(),
    );
    assert!(canonical_release_manifest(&manifest(sound)).is_ok());

    let mut doubled = closed();
    doubled.push(row(
        "dist/second",
        RuntimeRole::Executable,
        GitMode::ExecutableFile,
        '1',
    ));
    let cases = [
        (doubled, "a second executable row"),
        (
            vec![row(
                "dist/other",
                RuntimeRole::Executable,
                GitMode::ExecutableFile,
                '1',
            )],
            "a path that is not the tree path",
        ),
        (
            vec![row(
                "dist/amiss",
                RuntimeRole::Executable,
                GitMode::RegularFile,
                '1',
            )],
            "a nonexecutable mode",
        ),
        (
            vec![row(
                "dist/amiss",
                RuntimeRole::Executable,
                GitMode::ExecutableFile,
                '9',
            )],
            "a checksum that is not the binary's",
        ),
        (
            vec![row(
                "Cargo.lock",
                RuntimeRole::RuntimeData,
                GitMode::RegularFile,
                '3',
            )],
            "no executable row at all",
        ),
    ];
    for (files, reason) in cases {
        let broken = artifact(ConstraintPlatform::LinuxX8664, "amiss-linux-x86_64", files);
        let defect = canonical_release_manifest(&manifest(broken)).expect_err(reason);
        assert_eq!(defect.kind, ErrorKind::Inconsistent, "{reason}");
    }
}

#[test]
fn canonical_generation_revalidates_directly_constructed_models() {
    let mut wrong_oid_format = manifest(artifact(
        ConstraintPlatform::LinuxX8664,
        "amiss-linux-x86_64",
        closed(),
    ));
    wrong_oid_format.build_source.object_format = ObjectFormat::Sha256;
    let defect = canonical_release_manifest(&wrong_oid_format).expect_err("mismatched OID format");
    assert_eq!(defect.path, "$.build_source.commit_oid");

    let mut wrong_lock_digest = manifest(artifact(
        ConstraintPlatform::LinuxX8664,
        "amiss-linux-x86_64",
        closed(),
    ));
    wrong_lock_digest.dependency_lock_digest = digest('9');
    let defect =
        canonical_release_manifest(&wrong_lock_digest).expect_err("mismatched lock digest");
    assert_eq!(defect.kind, ErrorKind::DigestMismatch);

    let mut invalid_mode = manifest(artifact(
        ConstraintPlatform::LinuxX8664,
        "amiss-linux-x86_64",
        closed(),
    ));
    invalid_mode.artifacts[0].runtime_files[0].git_mode = GitMode::Tree;
    let defect =
        canonical_release_manifest(&invalid_mode).expect_err("a tree is not a runtime file");
    assert_eq!(defect.path, "$.artifacts[0].runtime_files[0].git_mode");
}

#[test]
fn typed_parsing_revalidates_embedded_repository_fields() {
    let raw = manifest_raw("sha1", &"a".repeat(40), LOCK, &one_artifact())
        .replace(r#""owner":"hardmax71""#, r#""owner":"HardMax71""#);
    let defect = parse_release_manifest(raw.as_bytes()).expect_err("an invalid repository owner");
    assert_eq!(defect.path, "$.build_source.repository");
}

/// Every runtime role in one parsed manifest, so every decoder arm is load-bearing.
#[test]
fn a_complete_manifest_parses_with_every_runtime_role() {
    let manifest = parse_release_manifest(
        manifest_raw("sha1", &"a".repeat(40), LOCK, &one_artifact()).as_bytes(),
    )
    .expect("the closed manifest parses");
    assert_eq!(manifest.engine_version, "0.5.1");
    let artifact = manifest.artifacts.first().expect("one artifact");
    assert_eq!(artifact.runtime_files.len(), 3);
    let mut executables = artifact
        .runtime_files
        .iter()
        .filter(|file| file.role == RuntimeRole::Executable);
    let executable = executables.next().expect("one executable row");
    assert!(executables.next().is_none());
    assert_eq!(executable.path, artifact.tree_path);
    assert_eq!(executable.git_mode, GitMode::ExecutableFile);
    assert_eq!(executable.file_sha256, artifact.binary_sha256);
}

const LOCK: &str = r#"{"schema":"amiss/scanner-dependency-lock-input","files":[{"path":"Cargo.lock","raw_digest":"sha256:4444444444444444444444444444444444444444444444444444444444444444"}]}"#;

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn manifest_raw(object_format: &str, commit_oid: &str, lock: &str, artifacts: &str) -> String {
    let lock_digest = hj(
        DEPENDENCY_LOCK_DOMAIN,
        &json::parse(lock.as_bytes()).expect("the lock template parses"),
    );
    format!(
        concat!(
            r#"{{"schema":"amiss/scanner-release-manifest","engine_version":"0.5.1","#,
            r#""build_source":{{"repository":{{"host":"github.com","owner":"hardmax71","name":"amiss"}},"#,
            r#""object_format":"{object_format}","commit_oid":"{commit_oid}"}},"#,
            r#""dependency_lock":{lock},"dependency_lock_digest":"{lock_digest}","#,
            r#""artifacts":[{artifacts}]}}"#,
        ),
        object_format = object_format,
        commit_oid = commit_oid,
        lock = lock,
        lock_digest = lock_digest,
        artifacts = artifacts,
    )
}

fn artifact_json(platform: &str, name: &str, files: &str) -> String {
    format!(
        concat!(
            r#"{{"platform":"{platform}","artifact_name":"{name}","#,
            r#""tree_path":"dist/amiss","binary_sha256":"{binary}","engine_digest":"{engine}","#,
            r#""runtime_contract":"manifest-closed","environment_contract":"scanner-process-env","#,
            r#""runtime_files":[{files}]}}"#,
        ),
        platform = platform,
        name = name,
        binary = digest('1'),
        engine = digest('2'),
        files = files,
    )
}

fn executable_row() -> String {
    format!(
        r#"{{"path":"dist/amiss","role":"executable","git_mode":"100755","file_sha256":"{}"}}"#,
        digest('1')
    )
}

fn one_artifact() -> String {
    let files = format!(
        concat!(
            r#"{executable},"#,
            r#"{{"path":"dist/data.bin","role":"runtime-data","git_mode":"100644","file_sha256":"{data}"}},"#,
            r#"{{"path":"dist/libdep.so","role":"dynamic-library","git_mode":"100644","file_sha256":"{library}"}}"#,
        ),
        executable = executable_row(),
        data = digest('5'),
        library = digest('6'),
    );
    artifact_json("linux-x86_64", "amiss-linux-x86_64", &files)
}

#[test]
fn a_sha256_build_source_parses() {
    let manifest = parse_release_manifest(
        manifest_raw("sha256", &"a".repeat(64), LOCK, &one_artifact()).as_bytes(),
    )
    .expect("a sha256 build source parses");
    assert_eq!(manifest.build_source.object_format, ObjectFormat::Sha256);
}

fn lock_with(count: usize) -> String {
    let rows: Vec<String> = (0..count)
        .map(|index| {
            format!(
                r#"{{"path":"deps/f{index:02}","raw_digest":"{}"}}"#,
                digest('4')
            )
        })
        .collect();
    format!(
        r#"{{"schema":"amiss/scanner-dependency-lock-input","files":[{}]}}"#,
        rows.join(",")
    )
}

#[test]
fn the_lock_holds_one_to_thirty_two_sorted_files() {
    let full = parse_release_manifest(
        manifest_raw("sha1", &"a".repeat(40), &lock_with(32), &one_artifact()).as_bytes(),
    )
    .expect("thirty-two lock files are within the ceiling");
    assert_eq!(full.dependency_lock.files.len(), 32);

    for (reason, count) in [("an empty lock", 0), ("a lock past the ceiling", 33)] {
        let defect = parse_release_manifest(
            manifest_raw("sha1", &"a".repeat(40), &lock_with(count), &one_artifact()).as_bytes(),
        )
        .expect_err(reason);
        assert_eq!(defect.kind, ErrorKind::LimitExceeded, "{reason}");
    }

    let misordered = lock_with(2).replace("deps/f00", "deps/f09");
    let defect = parse_release_manifest(
        manifest_raw("sha1", &"a".repeat(40), &misordered, &one_artifact()).as_bytes(),
    )
    .expect_err("descending lock files");
    assert_eq!(defect.kind, ErrorKind::UnsortedSet);
}

#[test]
fn artifacts_cover_at_most_the_closed_platform_set() {
    let platforms = [
        "linux-aarch64",
        "linux-x86_64",
        "macos-aarch64",
        "macos-x86_64",
        "windows-aarch64",
        "windows-x86_64",
    ];
    let six: Vec<String> = platforms
        .iter()
        .map(|platform| artifact_json(platform, "amiss", &executable_row()))
        .collect();
    let manifest = parse_release_manifest(
        manifest_raw("sha1", &"a".repeat(40), LOCK, &six.join(",")).as_bytes(),
    )
    .expect("every platform may ship");
    assert_eq!(manifest.artifacts.len(), 6);

    let seven = format!("{},{}", six.join(","), six[0]);
    let defect =
        parse_release_manifest(manifest_raw("sha1", &"a".repeat(40), LOCK, &seven).as_bytes())
            .expect_err("a seventh artifact");
    assert_eq!(defect.kind, ErrorKind::LimitExceeded);

    let defect = parse_release_manifest(manifest_raw("sha1", &"a".repeat(40), LOCK, "").as_bytes())
        .expect_err("no artifacts");
    assert_eq!(defect.kind, ErrorKind::LimitExceeded);
}

#[test]
fn runtime_files_hold_one_to_two_hundred_fifty_six_rows() {
    let files_with = |count: usize| {
        let mut rows = vec![executable_row()];
        rows.extend((0..count.saturating_sub(1)).map(|index| {
            format!(
                r#"{{"path":"dist/f{index:04}","role":"runtime-data","git_mode":"100644","file_sha256":"{}"}}"#,
                digest('5')
            )
        }));
        rows.join(",")
    };
    let full = parse_release_manifest(
        manifest_raw(
            "sha1",
            &"a".repeat(40),
            LOCK,
            &artifact_json("linux-x86_64", "amiss", &files_with(256)),
        )
        .as_bytes(),
    )
    .expect("two hundred fifty-six rows are within the ceiling");
    assert_eq!(
        full.artifacts
            .first()
            .expect("one artifact")
            .runtime_files
            .len(),
        256
    );

    for (reason, files) in [
        ("no runtime files", String::new()),
        ("rows past the ceiling", files_with(257)),
    ] {
        let defect = parse_release_manifest(
            manifest_raw(
                "sha1",
                &"a".repeat(40),
                LOCK,
                &artifact_json("linux-x86_64", "amiss", &files),
            )
            .as_bytes(),
        )
        .expect_err(reason);
        assert_eq!(defect.kind, ErrorKind::LimitExceeded, "{reason}");
    }
}

#[test]
fn runtime_roles_project_distinct_nonempty_spellings() {
    let spellings = [
        RuntimeRole::Executable.as_ref(),
        RuntimeRole::DynamicLibrary.as_ref(),
        RuntimeRole::RuntimeData.as_ref(),
    ];
    assert!(spellings.iter().all(|role| !role.is_empty()));
    let unique: std::collections::BTreeSet<&str> = spellings.iter().copied().collect();
    assert_eq!(unique.len(), spellings.len());
}

#[test]
fn version_strings_hold_the_release_shape() {
    let long_valid = format!("1.2.3-{}", "a".repeat(58));
    let long_invalid = format!("1.2.3-{}", "a".repeat(59));
    let encoded = |version: &str| {
        manifest_raw("sha1", &"a".repeat(40), LOCK, &one_artifact()).replace(
            r#""engine_version":"0.5.1""#,
            &format!(r#""engine_version":"{version}""#),
        )
    };
    for good in [
        "0.0.0",
        "1.2.3",
        "10.20.30",
        "1.2.3-rc.1",
        "0.5.2-a-b.7",
        long_valid.as_str(),
    ] {
        parse_release_manifest(encoded(good).as_bytes()).expect("a valid version");
    }
    for bad in [
        "1.2",
        "1.2.3.4",
        "1..3",
        ".2.3",
        "1.2.x",
        "-1.2.3",
        "1.2.3-",
        "1.2.3-RC",
        long_invalid.as_str(),
    ] {
        let defect = parse_release_manifest(encoded(bad).as_bytes()).expect_err("invalid version");
        assert_eq!(defect.path, "$.engine_version", "{bad}");
    }
}
