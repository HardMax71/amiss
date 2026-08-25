use std::io::Write as _;

use amiss_wire::json::{self, Value};

pub(crate) fn write_json(value: &Value) -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut output = std::io::BufWriter::new(stdout.lock());
    output.write_all(&json::canonical(value))?;
    output.write_all(b"\n")?;
    output.flush()
}

pub(crate) fn write_json_array<T>(
    items: impl IntoIterator<Item = T>,
    mut encode: impl FnMut(T) -> Vec<u8>,
) -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut output = std::io::BufWriter::new(stdout.lock());
    output.write_all(b"[")?;
    for (index, item) in items.into_iter().enumerate() {
        if index > 0 {
            output.write_all(b",")?;
        }
        output.write_all(&encode(item))?;
    }
    output.write_all(b"]\n")?;
    output.flush()
}
