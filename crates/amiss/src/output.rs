use std::io::{self, Write};

use amiss_wire::json::{self, Value};
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};

pub(crate) fn write_json(value: &Value) -> io::Result<()> {
    write_stdout(|output| json::write_to(value, output))
}

pub(crate) fn write_json_array<T: Serialize>(items: impl IntoIterator<Item = T>) -> io::Result<()> {
    write_stdout(|output| write_array(items, output))
}

fn write_stdout(write: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> io::Result<()> {
    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    write(&mut output)?;
    output.write_all(b"\n")?;
    output.flush()
}

fn write_array<T: Serialize>(
    items: impl IntoIterator<Item = T>,
    output: &mut dyn Write,
) -> io::Result<()> {
    let mut serializer = serde_json::Serializer::new(output);
    let mut sequence = serializer.serialize_seq(None)?;
    for item in items {
        sequence.serialize_element(&item)?;
    }
    sequence.end().map_err(Into::into)
}

mod tests;
