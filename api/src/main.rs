use std::ffi::{OsStr, OsString};
use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use amiss_wire::digest::{hb, hj};
use amiss_wire::json::Value;
use amiss_wire::model::ArtifactId;
use amiss_wire::semantic::record::{Input, InputSchema, Record};

mod context;
mod normalize;
mod tests;

const RUSTDOC_DOMAIN: &str = "amiss/rust-public-api-rustdoc-v1";
const INPUT_DOMAIN: &str = "amiss/rust-public-api-input-v1";
const PRODUCER_IDENTITY: &str = "amiss-rust-public-api";

struct Invocation {
    context: PathBuf,
    rustdoc: PathBuf,
}

#[derive(Debug, thiserror::Error)]
enum Failure {
    #[error("usage: amiss-rust-public-api --context <path> --rustdoc <path>")]
    Invocation,
    #[error("the current directory is unavailable")]
    CurrentDirectory(#[source] std::io::Error),
    #[error("the producer context cannot be read")]
    ContextRead(#[source] std::io::Error),
    #[error("the Rustdoc JSON cannot be read")]
    RustdocRead(#[source] std::io::Error),
    #[error(transparent)]
    Context(#[from] context::Error),
    #[error(transparent)]
    Normalize(#[from] normalize::Error),
    #[error("the fixed producer identity is invalid")]
    ProducerIdentity,
    #[error("the semantic template cannot be produced")]
    Template(#[source] amiss_wire::de::Error),
    #[error("stdout could not be written")]
    Write(#[source] std::io::Error),
}

#[expect(clippy::print_stderr, reason = "producer refusal channel")]
fn main() -> ExitCode {
    match run(std::env::args_os().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Write(error)) if error.kind() == std::io::ErrorKind::BrokenPipe => {
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("amiss-rust-public-api: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: impl Iterator<Item = OsString>) -> Result<(), Failure> {
    let invocation = invocation(arguments)?;
    let root = std::env::current_dir().map_err(Failure::CurrentDirectory)?;
    let context_path = root.join(invocation.context);
    let rustdoc_path = root.join(invocation.rustdoc);
    let context_bytes = amiss_controller_files::read_bounded(&context_path, context::BYTES)
        .map_err(Failure::ContextRead)?;
    let rustdoc_bytes =
        amiss_controller_files::read_bounded(&rustdoc_path, normalize::RUSTDOC_BYTES)
            .map_err(Failure::RustdocRead)?;
    let mut output = produce(&context_bytes, &rustdoc_bytes)?;
    output.push(b'\n');
    std::io::stdout()
        .lock()
        .write_all(&output)
        .map_err(Failure::Write)
}

fn invocation(mut arguments: impl Iterator<Item = OsString>) -> Result<Invocation, Failure> {
    if arguments.next().as_deref() != Some(OsStr::new("--context")) {
        return Err(Failure::Invocation);
    }
    let context = arguments
        .next()
        .map(PathBuf::from)
        .ok_or(Failure::Invocation)?;
    if arguments.next().as_deref() != Some(OsStr::new("--rustdoc")) {
        return Err(Failure::Invocation);
    }
    let rustdoc = arguments
        .next()
        .map(PathBuf::from)
        .ok_or(Failure::Invocation)?;
    if arguments.next().is_some()
        || context.as_os_str().is_empty()
        || rustdoc.as_os_str().is_empty()
    {
        return Err(Failure::Invocation);
    }
    Ok(Invocation { context, rustdoc })
}

fn produce(context_bytes: &[u8], rustdoc_bytes: &[u8]) -> Result<Vec<u8>, Failure> {
    let context = context::parse(context_bytes)?;
    let normalized = normalize::function_declarations(
        rustdoc_bytes,
        context.rustdoc_format,
        &context.target,
        &context.target_triple,
    )?;
    let rustdoc_digest = hb(RUSTDOC_DOMAIN, rustdoc_bytes);
    let input_digest = hj(
        INPUT_DOMAIN,
        &Value::object(vec![
            (
                "context_digest".to_owned(),
                Value::string(context.digest.to_string()),
            ),
            (
                "rustdoc_digest".to_owned(),
                Value::string(rustdoc_digest.to_string()),
            ),
        ]),
    );
    let producer_identity =
        ArtifactId::new(PRODUCER_IDENTITY.to_owned()).ok_or(Failure::ProducerIdentity)?;
    amiss_wire::semantic::record::template(Input {
        schema: InputSchema::Current,
        producer_identity,
        context_digest: context.digest,
        input_digest,
        complete: normalized.complete,
        name: context.name,
        records: normalized
            .records
            .into_iter()
            .map(|(key, value)| Record { key, value })
            .collect(),
    })
    .map_err(Failure::Template)
}
