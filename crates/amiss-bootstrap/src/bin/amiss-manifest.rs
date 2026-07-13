use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use amiss_bootstrap::build::{
    StagedArtifact, StagedBuild, StagedFile, action_metadata, build_manifest,
};
use amiss_wire::action::executable_platform;
use amiss_wire::manifest::RuntimeRole;

/// The release-side manifest builder: it reads the staged action tree, hashes
/// the exact bytes, and writes the strict manifest blob plus the canonical
/// `action.yml`. The build pipeline runs this; the bootstrap verifies its
/// output. Every platform row comes from the artifact's own header, so a
/// mislabeled binary cannot enter the manifest.
///
/// `amiss-manifest --tree DIR --version V --owner O --repository R
///  --commit OID --lock PATH [--lock PATH]... --artifact PATH [...]`
#[expect(clippy::print_stderr, reason = "the build tool's diagnostic channel")]
fn main() -> ExitCode {
    let argv: Vec<OsString> = env::args_os().skip(1).collect();
    let Some(parsed) = parse_args(&argv) else {
        eprintln!("amiss-manifest: invalid-invocation");
        return ExitCode::from(2);
    };
    match run(&parsed) {
        Ok(()) => ExitCode::SUCCESS,
        Err(reason) => {
            eprintln!("amiss-manifest: {reason}");
            ExitCode::from(2)
        }
    }
}

struct Args {
    tree: PathBuf,
    version: String,
    owner: String,
    repository: String,
    commit: String,
    locks: Vec<String>,
    artifacts: Vec<String>,
    launcher: Option<String>,
}

fn run(args: &Args) -> Result<(), String> {
    let lock_bytes: Vec<(String, Vec<u8>)> = args
        .locks
        .iter()
        .map(|path| read_at(&args.tree, path).map(|bytes| (path.clone(), bytes)))
        .collect::<Result<_, _>>()?;
    let launcher_bytes = args
        .launcher
        .as_ref()
        .map(|path| read_at(&args.tree, path).map(|bytes| (path.clone(), bytes)))
        .transpose()?;
    let artifact_bytes: Vec<(String, Vec<u8>)> = args
        .artifacts
        .iter()
        .map(|path| read_at(&args.tree, path).map(|bytes| (path.clone(), bytes)))
        .collect::<Result<_, _>>()?;

    let mut staged: Vec<StagedArtifact<'_>> = Vec::with_capacity(artifact_bytes.len());
    for (path, bytes) in &artifact_bytes {
        let platform = executable_platform(bytes)
            .ok_or_else(|| format!("{path}: the executable header names no supported platform"))?;
        let mut files = vec![StagedFile {
            path: path.clone(),
            role: RuntimeRole::Executable,
            executable: true,
            bytes,
        }];
        if let Some((launcher_path, launcher)) = &launcher_bytes {
            files.push(StagedFile {
                path: launcher_path.clone(),
                role: RuntimeRole::Launcher,
                executable: false,
                bytes: launcher,
            });
        }
        staged.push(StagedArtifact {
            platform,
            artifact_name: format!("amiss-{}", platform.as_str()),
            files,
        });
    }

    let build = StagedBuild {
        engine_version: args.version.clone(),
        owner: args.owner.clone(),
        repository: args.repository.clone(),
        object_format: "sha1",
        commit_oid: args.commit.clone(),
        locks: lock_bytes
            .iter()
            .map(|(path, bytes)| (path.clone(), bytes.as_slice()))
            .collect(),
    };
    let (manifest, digest) = build_manifest(&build, &mut staged).map_err(str::to_owned)?;
    std::fs::write(args.tree.join("release-manifest.json"), &manifest)
        .map_err(|defect| format!("release-manifest.json: {defect}"))?;
    if let Some((launcher_path, _bytes)) = &launcher_bytes {
        let metadata = action_metadata(
            "Amiss",
            "Documentation assurance for pull requests.",
            launcher_path,
        );
        std::fs::write(args.tree.join("action.yml"), metadata)
            .map_err(|defect| format!("action.yml: {defect}"))?;
    }
    print_digest(digest.to_string().as_str());
    Ok(())
}

#[expect(clippy::print_stdout, reason = "the release pipeline reads this value")]
fn print_digest(digest: &str) {
    println!("{digest}");
}

fn read_at(tree: &Path, path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(tree.join(path)).map_err(|defect| format!("{path}: {defect}"))
}

fn parse_args(argv: &[OsString]) -> Option<Args> {
    let mut tree: Option<PathBuf> = None;
    let mut version: Option<String> = None;
    let mut owner: Option<String> = None;
    let mut repository: Option<String> = None;
    let mut commit: Option<String> = None;
    let mut launcher: Option<String> = None;
    let mut locks: Vec<String> = Vec::new();
    let mut artifacts: Vec<String> = Vec::new();
    let mut items = argv.iter();
    while let Some(flag) = items.next() {
        let value = items.next()?.to_str()?.to_owned();
        match flag.to_str()? {
            "--tree" => tree = Some(PathBuf::from(value)),
            "--version" => version = Some(value),
            "--owner" => owner = Some(value),
            "--repository" => repository = Some(value),
            "--commit" => commit = Some(value),
            "--launcher" => launcher = Some(value),
            "--lock" => locks.push(value),
            "--artifact" => artifacts.push(value),
            _ => return None,
        }
    }
    if locks.is_empty() || artifacts.is_empty() {
        return None;
    }
    Some(Args {
        tree: tree?,
        version: version?,
        owner: owner?,
        repository: repository?,
        commit: commit?,
        locks,
        artifacts,
        launcher,
    })
}
