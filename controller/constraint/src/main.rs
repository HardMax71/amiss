use std::env;
use std::ffi::OsString;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use amiss_bootstrap::BOOTSTRAP_EXECUTABLE_BYTES;
use amiss_bootstrap::constraint::derive_execution_constraint;
use amiss_controller_files::read_bounded;
use amiss_git::{GitLimits, GitResources, Repository};
use amiss_wire::controls::valid_required_status_name;
use amiss_wire::model::{ObjectFormat, Oid, RepositoryIdentity};

const GRAMMAR: &str = concat!(
    "usage: amiss-constraint --action-repository PATH ",
    "--action-identity HOST/OWNER/NAME --action-commit OID ",
    "--bootstrap PATH --required-status-name NAME --output PATH"
);

#[expect(
    clippy::print_stderr,
    reason = "the provisioning tool's diagnostic channel"
)]
fn main() -> ExitCode {
    let arguments: Vec<OsString> = env::args_os().skip(1).collect();
    let Some(args) = parse_args(&arguments) else {
        eprintln!("amiss-constraint: invalid-invocation");
        eprintln!("{GRAMMAR}");
        return ExitCode::from(2);
    };
    match run(&args) {
        Ok(digest) => {
            let mut output = std::io::stdout().lock();
            let _ignored = writeln!(output, "{digest}");
            ExitCode::SUCCESS
        }
        Err(reason) => {
            eprintln!("amiss-constraint: {reason}");
            ExitCode::from(2)
        }
    }
}

struct Args {
    action_repository: PathBuf,
    action_identity: RepositoryIdentity,
    action_commit_oid: Oid,
    bootstrap: PathBuf,
    required_status_name: String,
    output: PathBuf,
}

struct ResolvedPaths {
    action_repository: PathBuf,
    bootstrap: PathBuf,
    output: PathBuf,
}

fn run(args: &Args) -> Result<String, &'static str> {
    let paths = resolve_paths(args)?;
    let bootstrap = read_bounded(&paths.bootstrap, BOOTSTRAP_EXECUTABLE_BYTES)
        .map_err(|_defect| "bootstrap-unreadable")?;
    let action = Repository::open(&paths.action_repository, ObjectFormat::Sha1)
        .map_err(|_defect| "action-repository-unavailable")?;
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let constraint = derive_execution_constraint(
        &action,
        &mut resources,
        &args.action_identity,
        &args.action_commit_oid,
        &args.required_status_name,
        &bootstrap,
    )
    .map_err(|defect| defect.reason)?;
    let bytes = constraint
        .canonical_bytes()
        .map_err(|_defect| "execution-constraint-invalid")?;
    write_new(&paths.output, &bytes).map_err(|_defect| "output-unavailable")?;
    Ok(constraint.digest.to_string())
}

fn parse_args(argv: &[OsString]) -> Option<Args> {
    let mut action_repository: Option<PathBuf> = None;
    let mut action_identity: Option<String> = None;
    let mut action_commit_oid: Option<String> = None;
    let mut bootstrap: Option<PathBuf> = None;
    let mut required_status_name: Option<String> = None;
    let mut output: Option<PathBuf> = None;
    let mut items = argv.iter();
    while let Some(flag) = items.next() {
        let value = items.next()?;
        match flag.to_str()? {
            "--action-repository" => set_once(&mut action_repository, PathBuf::from(value))?,
            "--action-identity" => {
                set_once(&mut action_identity, value.to_str()?.to_owned())?;
            }
            "--action-commit" => set_once(&mut action_commit_oid, value.to_str()?.to_owned())?,
            "--bootstrap" => set_once(&mut bootstrap, PathBuf::from(value))?,
            "--required-status-name" => {
                set_once(&mut required_status_name, value.to_str()?.to_owned())?;
            }
            "--output" => set_once(&mut output, PathBuf::from(value))?,
            _ => return None,
        }
    }
    let action_repository = action_repository?;
    let bootstrap = bootstrap?;
    let output = output?;
    if ![
        action_repository.as_path(),
        bootstrap.as_path(),
        output.as_path(),
    ]
    .into_iter()
    .all(Path::is_absolute)
    {
        return None;
    }
    let required_status_name = required_status_name?;
    if !valid_required_status_name(&required_status_name) {
        return None;
    }
    let action_commit_oid = Oid::new(ObjectFormat::Sha1, action_commit_oid?)?;
    Some(Args {
        action_repository,
        action_identity: parse_repository(&action_identity?)?,
        action_commit_oid,
        bootstrap,
        required_status_name,
        output,
    })
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Option<()> {
    if slot.is_some() {
        return None;
    }
    *slot = Some(value);
    Some(())
}

fn parse_repository(raw: &str) -> Option<RepositoryIdentity> {
    let (host, path) = raw.split_once('/')?;
    let (owner, name) = path.rsplit_once('/')?;
    RepositoryIdentity::new(host.to_owned(), owner.to_owned(), name.to_owned())
}

fn resolve_paths(args: &Args) -> Result<ResolvedPaths, &'static str> {
    let action_repository = canonical_directory(&args.action_repository)
        .map_err(|_defect| "action-repository-unavailable")?;
    let bootstrap = canonical_file(&args.bootstrap).map_err(|_defect| "bootstrap-unreadable")?;
    let output_name = args.output.file_name().ok_or("output-unavailable")?;
    let output_parent = args
        .output
        .parent()
        .ok_or("output-unavailable")
        .and_then(|parent| canonical_directory(parent).map_err(|_defect| "output-unavailable"))?;
    if bootstrap.starts_with(&action_repository) || output_parent.starts_with(&action_repository) {
        return Err("trust-path-overlap");
    }
    Ok(ResolvedPaths {
        action_repository,
        bootstrap,
        output: output_parent.join(output_name),
    })
}

fn canonical_directory(path: &Path) -> std::io::Result<PathBuf> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(std::io::Error::other("not a real directory"));
    }
    std::fs::canonicalize(path)
}

fn canonical_file(path: &Path) -> std::io::Result<PathBuf> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(std::io::Error::other("not a real file"));
    }
    std::fs::canonicalize(path)
}

fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("output has no parent"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file_mut().flush()?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(path)
        .map_err(|defect| defect.error)?;
    Ok(())
}
