use super::{MAX_SAFE_INTEGER, Text, Value, utf16_cmp};

const MAX_DEPTH: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ErrorKind {
    #[error("invalid UTF-8")]
    InvalidUtf8,
    #[error("byte-order mark is forbidden")]
    ByteOrderMark,
    #[error("unexpected end of input")]
    UnexpectedEnd,
    #[error("unexpected byte")]
    UnexpectedByte,
    #[error("trailing content")]
    TrailingContent,
    #[error("nesting limit exceeded")]
    DepthLimit,
    #[error("duplicate object key")]
    DuplicateKey,
    #[error("unescaped control character")]
    ControlCharacter,
    #[error("invalid escape")]
    InvalidEscape,
    #[error("lone UTF-16 surrogate")]
    LoneSurrogate,
    #[error("negative zero is forbidden")]
    NegativeZero,
    #[error("fractions and exponents are forbidden")]
    FractionOrExponent,
    #[error("integer is outside the safe range")]
    IntegerOutOfRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{kind} at byte {offset}")]
pub struct Error {
    pub kind: ErrorKind,
    pub offset: usize,
}

/// Parses exactly one strict JSON value from the complete input.
///
/// # Errors
///
/// Returns the first defect with its byte offset. Beyond plain syntax
/// errors, the restricted profile rejects a leading BOM, invalid UTF-8,
/// duplicate object keys, lone surrogate escapes, raw control characters,
/// `-0`, fractions and exponents, integers outside `MAX_SAFE_INTEGER`,
/// nesting past the depth limit, and trailing content.
pub fn parse(bytes: &[u8]) -> Result<Value, Error> {
    if bytes.get(..3) == Some(&[0xEF, 0xBB, 0xBF]) {
        return Err(Error {
            kind: ErrorKind::ByteOrderMark,
            offset: 0,
        });
    }
    let text = std::str::from_utf8(bytes).map_err(|invalid| Error {
        kind: ErrorKind::InvalidUtf8,
        offset: invalid.valid_up_to(),
    })?;
    let mut parser = Parser {
        bytes,
        text,
        pos: 0,
    };
    parser.skip_whitespace();
    let value = parser.value(0)?;
    parser.skip_whitespace();
    if parser.pos == bytes.len() {
        Ok(value)
    } else {
        Err(parser.error(ErrorKind::TrailingContent))
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    text: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, kind: ErrorKind) -> Error {
        Error {
            kind,
            offset: self.pos,
        }
    }

    fn end_or_unexpected(&self) -> Error {
        if self.peek().is_none() {
            self.error(ErrorKind::UnexpectedEnd)
        } else {
            self.error(ErrorKind::UnexpectedByte)
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn advance(&mut self) {
        self.pos = self.pos.saturating_add(1);
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.advance();
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), Error> {
        if self.peek() == Some(byte) {
            self.advance();
            Ok(())
        } else {
            Err(self.end_or_unexpected())
        }
    }

    fn literal(&mut self, text: &[u8]) -> Result<(), Error> {
        for &byte in text {
            self.expect(byte)?;
        }
        Ok(())
    }

    fn deeper(&self, depth: usize) -> Result<usize, Error> {
        let next = depth.saturating_add(1);
        if next > MAX_DEPTH {
            Err(self.error(ErrorKind::DepthLimit))
        } else {
            Ok(next)
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, Error> {
        let byte = self
            .peek()
            .ok_or_else(|| self.error(ErrorKind::UnexpectedEnd))?;
        match byte {
            b'n' => self.literal(b"null").map(|()| Value::Null),
            b't' => self.literal(b"true").map(|()| Value::Bool(true)),
            b'f' => self.literal(b"false").map(|()| Value::Bool(false)),
            b'"' => self.string().map(Value::String),
            b'{' => self.object(depth),
            b'[' => self.array(depth),
            b'-' | b'0'..=b'9' => self.number(),
            _ => Err(self.error(ErrorKind::UnexpectedByte)),
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, Error> {
        let depth = self.deeper(depth)?;
        self.expect(b'[')?;
        self.skip_whitespace();
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.advance();
            return Ok(Value::Array(items.into_boxed_slice()));
        }
        loop {
            self.skip_whitespace();
            items.push(self.value(depth)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.advance(),
                Some(b']') => {
                    self.advance();
                    return Ok(Value::Array(items.into_boxed_slice()));
                }
                _ => return Err(self.end_or_unexpected()),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, Error> {
        let depth = self.deeper(depth)?;
        self.expect(b'{')?;
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.advance();
            return Ok(Value::Object(Box::default()));
        }
        let mut members: Vec<(String, Value, usize)> = Vec::new();
        loop {
            self.skip_whitespace();
            let key_offset = self.pos;
            if self.peek() != Some(b'"') {
                return Err(self.end_or_unexpected());
            }
            let key = String::from(self.string()?);
            self.skip_whitespace();
            self.expect(b':')?;
            self.skip_whitespace();
            let value = self.value(depth)?;
            members.push((key, value, key_offset));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.advance(),
                Some(b'}') => {
                    self.advance();
                    break;
                }
                _ => return Err(self.end_or_unexpected()),
            }
        }
        members.sort_by(|a, b| utf16_cmp(&a.0, &b.0));
        for pair in members.windows(2) {
            if let [left, right] = pair
                && left.0 == right.0
            {
                return Err(Error {
                    kind: ErrorKind::DuplicateKey,
                    offset: right.2,
                });
            }
        }
        Ok(Value::Object(
            members
                .into_iter()
                .map(|(key, value, _)| (key, value))
                .collect(),
        ))
    }

    fn string(&mut self) -> Result<Text, Error> {
        self.expect(b'"')?;
        let mut out = String::new();
        let mut segment_start = self.pos;
        loop {
            match self.peek() {
                None => return Err(self.error(ErrorKind::UnexpectedEnd)),
                Some(b'"') => {
                    self.flush(segment_start, &mut out)?;
                    self.advance();
                    return Ok(out.into_boxed_str());
                }
                Some(b'\\') => {
                    self.flush(segment_start, &mut out)?;
                    self.advance();
                    self.escape(&mut out)?;
                    segment_start = self.pos;
                }
                Some(byte) if byte < 0x20 => return Err(self.error(ErrorKind::ControlCharacter)),
                Some(_) => self.advance(),
            }
        }
    }

    fn flush(&self, from: usize, out: &mut String) -> Result<(), Error> {
        let segment = self
            .text
            .get(from..self.pos)
            .ok_or_else(|| self.error(ErrorKind::InvalidUtf8))?;
        out.push_str(segment);
        Ok(())
    }

    fn escape(&mut self, out: &mut String) -> Result<(), Error> {
        let byte = self
            .peek()
            .ok_or_else(|| self.error(ErrorKind::UnexpectedEnd))?;
        self.advance();
        let simple = match byte {
            b'"' => Some('"'),
            b'\\' => Some('\\'),
            b'/' => Some('/'),
            b'b' => Some('\u{8}'),
            b'f' => Some('\u{c}'),
            b'n' => Some('\n'),
            b'r' => Some('\r'),
            b't' => Some('\t'),
            b'u' => None,
            _ => {
                self.pos = self.pos.saturating_sub(1);
                return Err(self.error(ErrorKind::InvalidEscape));
            }
        };
        if let Some(c) = simple {
            out.push(c);
            return Ok(());
        }
        let unit = self.hex4()?;
        let code = match unit {
            0xD800..=0xDBFF => {
                if self.peek() != Some(b'\\') {
                    return Err(self.error(ErrorKind::LoneSurrogate));
                }
                self.advance();
                if self.peek() != Some(b'u') {
                    return Err(self.error(ErrorKind::LoneSurrogate));
                }
                self.advance();
                let low = self.hex4()?;
                if !(0xDC00..=0xDFFF).contains(&low) {
                    return Err(self.error(ErrorKind::LoneSurrogate));
                }
                combine_surrogates(unit, low)
            }
            0xDC00..=0xDFFF => return Err(self.error(ErrorKind::LoneSurrogate)),
            unit => unit,
        };
        let c = char::from_u32(code).ok_or_else(|| self.error(ErrorKind::LoneSurrogate))?;
        out.push(c);
        Ok(())
    }

    fn hex4(&mut self) -> Result<u32, Error> {
        let start = self.pos;
        let end = start.saturating_add(4).min(self.bytes.len());
        let digits = self.bytes.get(start..end).ok_or(Error {
            kind: ErrorKind::UnexpectedEnd,
            offset: self.bytes.len(),
        })?;
        let code = digits
            .iter()
            .copied()
            .enumerate()
            .try_fold(0_u32, |code, (offset, byte)| {
                char::from(byte)
                    .to_digit(16)
                    .map(|digit| code.wrapping_shl(4) | digit)
                    .ok_or(Error {
                        kind: ErrorKind::InvalidEscape,
                        offset: start.saturating_add(offset),
                    })
            })?;
        if digits.len() < 4 {
            return Err(Error {
                kind: ErrorKind::UnexpectedEnd,
                offset: self.bytes.len(),
            });
        }
        self.pos = end;
        Ok(code)
    }

    fn number(&mut self) -> Result<Value, Error> {
        let negative = self.peek() == Some(b'-');
        if negative {
            self.advance();
        }
        let first = self
            .peek()
            .ok_or_else(|| self.error(ErrorKind::UnexpectedEnd))?;
        let mut magnitude: i64 = match first {
            b'0'..=b'9' => i64::from(first.wrapping_sub(b'0')),
            _ => return Err(self.error(ErrorKind::UnexpectedByte)),
        };
        self.advance();
        if magnitude == 0 {
            if matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.error(ErrorKind::UnexpectedByte));
            }
        } else {
            while let Some(byte @ b'0'..=b'9') = self.peek() {
                let digit = i64::from(byte.wrapping_sub(b'0'));
                magnitude = magnitude
                    .checked_mul(10)
                    .and_then(|v| v.checked_add(digit))
                    .ok_or_else(|| self.error(ErrorKind::IntegerOutOfRange))?;
                self.advance();
            }
        }
        if matches!(self.peek(), Some(b'.' | b'e' | b'E')) {
            return Err(self.error(ErrorKind::FractionOrExponent));
        }
        if magnitude > MAX_SAFE_INTEGER {
            return Err(self.error(ErrorKind::IntegerOutOfRange));
        }
        if negative {
            if magnitude == 0 {
                return Err(self.error(ErrorKind::NegativeZero));
            }
            magnitude = magnitude.wrapping_neg();
        }
        Ok(Value::Integer(magnitude))
    }
}

#[expect(clippy::arithmetic_side_effects, reason = "operands are range-checked")]
fn combine_surrogates(high: u32, low: u32) -> u32 {
    0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00)
}
