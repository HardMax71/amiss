use crate::json::{Scratch, Sink, Value, canonical_length};

use super::FATAL_SCRATCH_BYTES;

/// The streaming fatal-envelope serializer and its fixed scratch space. A
/// binary reserves one before evaluator allocation accounting begins, so a
/// fatal projection is always emittable: emission streams `JCS(envelope)`
/// and the trailing newline through the reserved staging buffer without
/// materializing the wire.
pub struct FatalSerializer {
    staging: Vec<u8>,
    scratch: Scratch,
}

impl FatalSerializer {
    /// Reserves the staging buffer and serializer scratch.
    #[must_use]
    pub fn new() -> Self {
        Self {
            staging: Vec::with_capacity(FATAL_SCRATCH_BYTES),
            scratch: Scratch::new(),
        }
    }

    /// Streams the envelope's wire (`JCS(envelope) || LF`) into the writer
    /// through the reserved scratch and returns the byte count.
    ///
    /// # Errors
    ///
    /// The first writer error; the wire is incomplete in that case and the
    /// caller treats the emission as failed.
    pub fn emit(&mut self, envelope: &Value, out: &mut dyn std::io::Write) -> std::io::Result<u64> {
        self.staging.clear();
        let mut sink = StagedSink {
            staging: &mut self.staging,
            out,
            written: 0,
            error: None,
        };
        self.scratch.stream(envelope, &mut sink);
        sink.write("\n");
        let written = sink.flush();
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
    out: &'a mut dyn std::io::Write,
    written: u64,
    error: Option<std::io::Error>,
}

impl StagedSink<'_> {
    fn drain(&mut self) {
        if self.error.is_none() {
            match self.out.write_all(self.staging) {
                Ok(()) => {
                    self.written = self
                        .written
                        .saturating_add(u64::try_from(self.staging.len()).unwrap_or(u64::MAX));
                }
                Err(defect) => self.error = Some(defect),
            }
        }
        self.staging.clear();
    }

    fn flush(&mut self) -> std::io::Result<u64> {
        self.drain();
        match self.error.take() {
            Some(defect) => Err(defect),
            None => Ok(self.written),
        }
    }
}

impl Sink for StagedSink<'_> {
    fn write(&mut self, piece: &str) {
        if piece.len() >= FATAL_SCRATCH_BYTES {
            self.drain();
            if self.error.is_none() {
                match self.out.write_all(piece.as_bytes()) {
                    Ok(()) => {
                        self.written = self
                            .written
                            .saturating_add(u64::try_from(piece.len()).unwrap_or(u64::MAX));
                    }
                    Err(defect) => self.error = Some(defect),
                }
            }
            return;
        }
        if self.staging.len().saturating_add(piece.len()) > FATAL_SCRATCH_BYTES {
            self.drain();
        }
        self.staging.extend_from_slice(piece.as_bytes());
    }
}
