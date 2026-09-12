use std::io::{self, Write};

use crate::json::{Value, canonical_length};

use super::FATAL_SCRATCH_BYTES;

/// The streaming fatal-envelope serializer and its fixed scratch space. A
/// binary reserves one before evaluator allocation accounting begins, so a
/// fatal projection is always emittable: emission streams `JCS(envelope)`
/// and the trailing newline through the reserved staging buffer without
/// materializing the wire.
pub struct FatalSerializer {
    staging: Vec<u8>,
}

impl FatalSerializer {
    /// Reserves the staging buffer and serializer scratch.
    #[must_use]
    pub fn new() -> Self {
        Self {
            staging: Vec::with_capacity(FATAL_SCRATCH_BYTES),
        }
    }

    /// Streams the envelope's wire (`JCS(envelope) || LF`) into the writer
    /// through the reserved scratch and returns the byte count.
    ///
    /// # Errors
    ///
    /// The first writer error; the wire is incomplete in that case and the
    /// caller treats the emission as failed.
    pub fn emit(&mut self, envelope: &Value, out: &mut dyn Write) -> io::Result<u64> {
        self.staging.clear();
        let mut sink = StagedSink {
            staging: &mut self.staging,
            out,
            written: 0,
        };
        let written = (|| {
            crate::json::write_to(envelope, &mut sink)?;
            sink.write_all(b"\n")?;
            sink.drain()?;
            Ok(sink.written)
        })();
        self.staging.clear();
        written
    }

    /// The materialized wire for a caller that must inspect the bytes (the
    /// wrapper's acceptance): one counting pass sizes the allocation
    /// exactly, then one streaming pass fills it.
    #[must_use]
    pub fn wire_bytes(&mut self, envelope: &Value) -> Vec<u8> {
        let exact = canonical_length(envelope).saturating_add(1);
        let mut wire = Vec::with_capacity(usize::try_from(exact).unwrap_or(0));
        if self.emit(envelope, &mut wire).is_err() {
            wire.clear();
        }
        wire
    }
}

impl Default for FatalSerializer {
    fn default() -> Self {
        Self::new()
    }
}

struct StagedSink<'a> {
    staging: &'a mut Vec<u8>,
    out: &'a mut dyn Write,
    written: u64,
}

impl StagedSink<'_> {
    fn drain(&mut self) -> io::Result<()> {
        self.out.write_all(self.staging)?;
        self.written = self
            .written
            .saturating_add(u64::try_from(self.staging.len()).unwrap_or(u64::MAX));
        self.staging.clear();
        Ok(())
    }
}

impl Write for StagedSink<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() >= FATAL_SCRATCH_BYTES {
            self.drain()?;
            self.out.write_all(bytes)?;
            self.written = self
                .written
                .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        } else {
            if self.staging.len().saturating_add(bytes.len()) > FATAL_SCRATCH_BYTES {
                self.drain()?;
            }
            self.staging.extend_from_slice(bytes);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.drain()?;
        self.out.flush()
    }
}
