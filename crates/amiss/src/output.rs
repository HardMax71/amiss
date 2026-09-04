use std::io::Write as _;

pub(crate) fn write_json(bytes: &[u8]) -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut output = std::io::BufWriter::new(stdout.lock());
    output.write_all(bytes)?;
    output.write_all(b"\n")?;
    output.flush()
}

pub(crate) fn write_serialized<T: serde::Serialize + ?Sized>(value: &T) -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut output = std::io::BufWriter::new(stdout.lock());
    serde_json::to_writer(&mut output, value)?;
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
