use std::cmp::Ordering;

pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
const MAX_DEPTH: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(i64),
    String(String),
    Array(Vec<Value>),
    /// Keys sorted by UTF-16 code units and unique; `parse` enforces both.
    Object(Vec<(String, Value)>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    InvalidUtf8,
    ByteOrderMark,
    UnexpectedEnd,
    UnexpectedByte,
    TrailingContent,
    DepthLimit,
    DuplicateKey,
    ControlCharacter,
    InvalidEscape,
    LoneSurrogate,
    NegativeZero,
    FractionOrExponent,
    IntegerOutOfRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[must_use]
pub fn canonical(value: &Value) -> Vec<u8> {
    let mut out = String::new();
    write_value(&mut out, value);
    out.into_bytes()
}

fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

fn write_value(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Integer(n) => out.push_str(&n.to_string()),
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(out, item);
            }
            out.push(']');
        }
        Value::Object(members) => {
            let mut sorted: Vec<&(String, Value)> = members.iter().collect();
            sorted.sort_by(|a, b| utf16_cmp(&a.0, &b.0));
            out.push('{');
            for (i, member) in sorted.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(out, &member.0);
                out.push(':');
                write_value(out, &member.1);
            }
            out.push('}');
        }
    }
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if u32::from(c) < 0x20 => control_escape(out, c),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn control_escape(out: &mut String, c: char) {
    let value = u32::from(c);
    out.push_str("\\u00");
    out.push(hex_digit(value.wrapping_shr(4)));
    out.push(hex_digit(value & 0xF));
}

fn hex_digit(value: u32) -> char {
    char::from_digit(value, 16).unwrap_or('0')
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
        match self.peek() {
            None => Err(self.error(ErrorKind::UnexpectedEnd)),
            Some(b'n') => self.literal(b"null").map(|()| Value::Null),
            Some(b't') => self.literal(b"true").map(|()| Value::Bool(true)),
            Some(b'f') => self.literal(b"false").map(|()| Value::Bool(false)),
            Some(b'"') => self.string().map(Value::String),
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error(ErrorKind::UnexpectedByte)),
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, Error> {
        let depth = self.deeper(depth)?;
        self.expect(b'[')?;
        self.skip_whitespace();
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.advance();
            return Ok(Value::Array(items));
        }
        loop {
            self.skip_whitespace();
            items.push(self.value(depth)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.advance(),
                Some(b']') => {
                    self.advance();
                    return Ok(Value::Array(items));
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
            return Ok(Value::Object(Vec::new()));
        }
        let mut members: Vec<(String, Value, usize)> = Vec::new();
        loop {
            self.skip_whitespace();
            let key_offset = self.pos;
            if self.peek() != Some(b'"') {
                return Err(self.end_or_unexpected());
            }
            let key = self.string()?;
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
            members.into_iter().map(|(k, v, _)| (k, v)).collect(),
        ))
    }

    fn string(&mut self) -> Result<String, Error> {
        self.expect(b'"')?;
        let mut out = String::new();
        let mut segment_start = self.pos;
        loop {
            match self.peek() {
                None => return Err(self.error(ErrorKind::UnexpectedEnd)),
                Some(b'"') => {
                    self.flush(segment_start, &mut out)?;
                    self.advance();
                    return Ok(out);
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
        let mut code = 0_u32;
        for _ in 0_u8..4 {
            let byte = self
                .peek()
                .ok_or_else(|| self.error(ErrorKind::UnexpectedEnd))?;
            let digit = match byte {
                b'0'..=b'9' => u32::from(byte.wrapping_sub(b'0')),
                b'a'..=b'f' => u32::from(byte.wrapping_sub(b'a')).wrapping_add(10),
                b'A'..=b'F' => u32::from(byte.wrapping_sub(b'A')).wrapping_add(10),
                _ => return Err(self.error(ErrorKind::InvalidEscape)),
            };
            code = code.wrapping_shl(4) | digit;
            self.advance();
        }
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
