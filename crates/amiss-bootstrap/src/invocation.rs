use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use super::{Args, OutputFiles};

pub(super) fn parse_args(argv: &[OsString]) -> Option<Args> {
    let mut action_repository: Option<PathBuf> = None;
    let mut repository: Option<PathBuf> = None;
    let mut constraint: Option<PathBuf> = None;
    let mut evaluation_request: Option<PathBuf> = None;
    let mut snapshot_request: Option<PathBuf> = None;
    let mut controls_request: Option<PathBuf> = None;
    let mut scratch: Option<PathBuf> = None;
    let mut report: Option<PathBuf> = None;
    let mut result: Option<PathBuf> = None;
    let mut items = argv.iter();
    if items.next()? != "exec" {
        return None;
    }
    while let Some(flag) = items.next() {
        let value = items.next()?;
        let slot = match flag.to_str()? {
            "--action-repository" => &mut action_repository,
            "--repository" => &mut repository,
            "--constraint" => &mut constraint,
            "--evaluation-request" => &mut evaluation_request,
            "--snapshot-request" => &mut snapshot_request,
            "--controls-request" => &mut controls_request,
            "--scratch" => &mut scratch,
            "--report" => &mut report,
            "--result" => &mut result,
            _ => return None,
        };
        if slot.is_some() {
            return None;
        }
        *slot = Some(PathBuf::from(value));
    }
    let scratch = scratch?;
    if !scratch.is_absolute()
        || !std::fs::symlink_metadata(&scratch).is_ok_and(|metadata| metadata.file_type().is_dir())
    {
        return None;
    }
    let report = report?;
    let result = result?;
    if !output_path(&report, &scratch, "report") || !output_path(&result, &scratch, "result") {
        return None;
    }
    Some(Args {
        action_repository: action_repository?,
        repository: repository?,
        constraint: constraint?,
        evaluation_request: evaluation_request?,
        snapshot_request: snapshot_request?,
        controls_request: controls_request?,
        scratch,
        report,
        result,
    })
}

fn output_path(path: &Path, scratch: &Path, name: &str) -> bool {
    path.is_absolute()
        && path.parent() == Some(scratch)
        && path.file_name() == Some(OsStr::new(name))
        && std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.file_type().is_file() && metadata.len() == 0)
}

pub(super) fn open_output(args: &Args) -> std::io::Result<OutputFiles> {
    Ok(OutputFiles {
        report: open_output_file(&args.report)?,
        result: open_output_file(&args.result)?,
    })
}

fn open_output_file(path: &Path) -> std::io::Result<File> {
    let file = OpenOptions::new().write(true).open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() != 0 {
        return Err(std::io::Error::other("invalid output file"));
    }
    Ok(file)
}
