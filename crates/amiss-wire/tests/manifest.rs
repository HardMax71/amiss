use amiss_wire::controls::{ConstraintPlatform, GitMode};
use amiss_wire::digest::{Digest, hj};
use amiss_wire::json;
use amiss_wire::manifest::{
    DEPENDENCY_LOCK_DOMAIN, ReleaseArtifact, ReleaseManifest, RuntimeFile, RuntimeRole,
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
        runtime_files,
    }
}

fn closed() -> Vec<RuntimeFile> {
    vec![
        row(
            "dist/amiss",
            RuntimeRole::Executable,
            GitMode::ExecutableFile,
            '1',
        ),
        row(
            "dist/launcher.js",
            RuntimeRole::Launcher,
            GitMode::RegularFile,
            '3',
        ),
    ]
}

#[test]
fn the_executable_row_holds_every_clause_of_the_closure_law() {
    let sound = artifact(
        ConstraintPlatform::LinuxX8664,
        "amiss-linux-x86_64",
        closed(),
    );
    assert!(sound.executable().is_some());

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
            vec![
                row(
                    "dist/other",
                    RuntimeRole::Executable,
                    GitMode::ExecutableFile,
                    '1',
                ),
                row(
                    "dist/launcher.js",
                    RuntimeRole::Launcher,
                    GitMode::RegularFile,
                    '3',
                ),
            ],
            "a path that is not the tree path",
        ),
        (
            vec![
                row(
                    "dist/amiss",
                    RuntimeRole::Executable,
                    GitMode::RegularFile,
                    '1',
                ),
                row(
                    "dist/launcher.js",
                    RuntimeRole::Launcher,
                    GitMode::RegularFile,
                    '3',
                ),
            ],
            "a nonexecutable mode",
        ),
        (
            vec![
                row(
                    "dist/amiss",
                    RuntimeRole::Executable,
                    GitMode::ExecutableFile,
                    '9',
                ),
                row(
                    "dist/launcher.js",
                    RuntimeRole::Launcher,
                    GitMode::RegularFile,
                    '3',
                ),
            ],
            "a checksum that is not the binary's",
        ),
        (
            vec![row(
                "dist/launcher.js",
                RuntimeRole::Launcher,
                GitMode::RegularFile,
                '3',
            )],
            "no executable row at all",
        ),
    ];
    for (files, reason) in cases {
        let broken = artifact(ConstraintPlatform::LinuxX8664, "amiss-linux-x86_64", files);
        assert!(broken.executable().is_none(), "{reason}");
    }
}

#[test]
fn the_launcher_row_is_a_single_regular_blob() {
    let sound = artifact(
        ConstraintPlatform::LinuxX8664,
        "amiss-linux-x86_64",
        closed(),
    );
    assert!(sound.launcher().is_some());

    let mut doubled = closed();
    doubled.push(row(
        "dist/second.js",
        RuntimeRole::Launcher,
        GitMode::RegularFile,
        '3',
    ));
    let executable_mode = vec![
        row(
            "dist/amiss",
            RuntimeRole::Executable,
            GitMode::ExecutableFile,
            '1',
        ),
        row(
            "dist/launcher.js",
            RuntimeRole::Launcher,
            GitMode::ExecutableFile,
            '3',
        ),
    ];
    let absent = vec![row(
        "dist/amiss",
        RuntimeRole::Executable,
        GitMode::ExecutableFile,
        '1',
    )];
    for (files, reason) in [
        (doubled, "a second launcher row"),
        (executable_mode, "an executable launcher mode"),
        (absent, "no launcher row at all"),
    ] {
        let broken = artifact(ConstraintPlatform::LinuxX8664, "amiss-linux-x86_64", files);
        assert!(broken.launcher().is_none(), "{reason}");
    }
}

#[test]
fn selection_matches_platform_and_name_together() {
    let linux = artifact(
        ConstraintPlatform::LinuxX8664,
        "amiss-linux-x86_64",
        closed(),
    );
    let mac = artifact(
        ConstraintPlatform::MacosAarch64,
        "amiss-macos-aarch64",
        closed(),
    );
    let manifest = ReleaseManifest {
        digest: digest('a'),
        engine_version: "0.5.1".to_owned(),
        build_source: amiss_wire::manifest::BuildSource {
            repository: RepositoryIdentity::new(
                "github.com".to_owned(),
                "hardmax71".to_owned(),
                "amiss".to_owned(),
            )
            .expect("a repository identity"),
            object_format: ObjectFormat::Sha1,
            commit_oid: Oid::new(ObjectFormat::Sha1, "a".repeat(40)).expect("an oid"),
        },
        dependency_lock: amiss_wire::manifest::DependencyLockInput { files: Vec::new() },
        dependency_lock_digest: digest('b'),
        artifacts: vec![linux.clone(), mac],
    };
    let linux_name = ArtifactId::new("amiss-linux-x86_64".to_owned()).expect("an artifact id");
    let mac_name = ArtifactId::new("amiss-macos-aarch64".to_owned()).expect("an artifact id");
    assert_eq!(
        manifest.select(ConstraintPlatform::LinuxX8664, &linux_name),
        Some(&linux)
    );
    assert!(
        manifest
            .select(ConstraintPlatform::MacosAarch64, &linux_name)
            .is_none(),
        "a name selects only on its own platform"
    );
    assert!(
        manifest
            .select(ConstraintPlatform::LinuxX8664, &mac_name)
            .is_none(),
        "a platform selects only its own name"
    );
    assert!(
        manifest
            .select(ConstraintPlatform::WindowsX8664, &linux_name)
            .is_none(),
        "an unlisted platform selects nothing"
    );
}

/// All four roles in one parsed manifest, so every decoder arm is load-bearing.
#[test]
fn a_complete_manifest_parses_with_every_runtime_role() {
    let lock = r#"{"schema":"amiss/scanner-dependency-lock-input","files":[{"path":"Cargo.lock","raw_digest":"sha256:4444444444444444444444444444444444444444444444444444444444444444"}]}"#;
    let lock_digest = hj(
        DEPENDENCY_LOCK_DOMAIN,
        &json::parse(lock.as_bytes()).expect("the lock template parses"),
    );
    let binary = digest('1');
    let raw = format!(
        concat!(
            r#"{{"schema":"amiss/scanner-release-manifest","engine_version":"0.5.1","#,
            r#""build_source":{{"repository":{{"host":"github.com","owner":"hardmax71","name":"amiss"}},"#,
            r#""object_format":"sha1","commit_oid":"{oid}"}},"#,
            r#""dependency_lock":{lock},"dependency_lock_digest":"{lock_digest}","#,
            r#""artifacts":[{{"platform":"linux-x86_64","artifact_name":"amiss-linux-x86_64","#,
            r#""tree_path":"dist/amiss","binary_sha256":"{binary}","engine_digest":"{engine}","#,
            r#""runtime_contract":"manifest-closed","environment_contract":"scanner-process-env","#,
            r#""runtime_files":["#,
            r#"{{"path":"dist/amiss","role":"executable","git_mode":"100755","file_sha256":"{binary}"}},"#,
            r#"{{"path":"dist/data.bin","role":"runtime-data","git_mode":"100644","file_sha256":"{data}"}},"#,
            r#"{{"path":"dist/launcher.js","role":"launcher","git_mode":"100644","file_sha256":"{launcher}"}},"#,
            r#"{{"path":"dist/libdep.so","role":"dynamic-library","git_mode":"100644","file_sha256":"{library}"}}"#,
            r#"]}}]}}"#,
        ),
        oid = "a".repeat(40),
        lock = lock,
        lock_digest = lock_digest,
        binary = binary,
        engine = digest('2'),
        data = digest('5'),
        launcher = digest('3'),
        library = digest('6'),
    );
    let manifest = ReleaseManifest::parse(raw.as_bytes()).expect("the closed manifest parses");
    assert_eq!(manifest.engine_version, "0.5.1");
    let artifact = manifest.artifacts.first().expect("one artifact");
    assert_eq!(artifact.runtime_files.len(), 4);
    assert!(artifact.executable().is_some());
    assert!(artifact.launcher().is_some());
}

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn version_error_path(version: &str) -> String {
    let raw =
        format!(r#"{{"schema":"amiss/scanner-release-manifest","engine_version":"{version}"}}"#);
    ReleaseManifest::parse(raw.as_bytes())
        .expect_err("a two-field manifest never completes")
        .path
}

/// An accepted shape moves the failure past the version field; a rejected one stops on it.
#[test]
fn version_strings_hold_the_release_shape() {
    let long_valid = format!("1.2.3-{}", "a".repeat(58));
    let long_invalid = format!("1.2.3-{}", "a".repeat(59));
    for good in [
        "0.0.0",
        "1.2.3",
        "10.20.30",
        "1.2.3-rc.1",
        "0.5.2-a-b.7",
        long_valid.as_str(),
    ] {
        assert_ne!(version_error_path(good), "$.engine_version", "{good}");
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
        assert_eq!(version_error_path(bad), "$.engine_version", "{bad}");
    }
}
