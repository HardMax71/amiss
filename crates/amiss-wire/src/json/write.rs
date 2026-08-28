use std::fmt;

use super::{Value, utf16_cmp};

#[must_use]
pub fn canonical(value: &Value) -> Vec<u8> {
    let mut out = String::new();
    stream(value, &mut out);
    out.into_bytes()
}

/// The exact byte length `canonical` would produce, without materializing
/// it: the counting canonical-serialization pass.
#[must_use]
pub fn canonical_length(value: &Value) -> u64 {
    let mut count = 0_u64;
    stream(
        value,
        &mut Callback(|piece: &str| {
            count = count.saturating_add(u64::try_from(piece.len()).unwrap_or(u64::MAX));
        }),
    );
    count
}

/// A canonicalization output receives the serialization in ordered pieces.
pub trait Sink {
    fn write(&mut self, piece: &str);
}

impl Sink for String {
    fn write(&mut self, piece: &str) {
        self.push_str(piece);
    }
}

pub(crate) struct Callback<F>(pub(crate) F);

impl<F: FnMut(&str)> Sink for Callback<F> {
    fn write(&mut self, piece: &str) {
        self.0(piece);
    }
}

/// Streams the canonical serialization into the sink using its own
/// transient scratch; the fatal lane reuses one reserved scratch instead.
pub fn stream<S: Sink + ?Sized>(value: &Value, sink: &mut S) {
    let mut scratch = Scratch::reserved();
    scratch.stream(value, sink);
}

/// The serializer's entire working memory: one member-order buffer per
/// nesting level, reused across every sibling at that level, and one
/// integer-format buffer. Streaming allocates nothing else, so a reserved
/// `Scratch` makes serialization scratch a fixed space.
pub(crate) struct Scratch {
    order: Vec<Vec<usize>>,
    number: String,
}

impl Scratch {
    #[must_use]
    pub(crate) fn reserved() -> Self {
        Self {
            order: Vec::new(),
            number: String::with_capacity(24),
        }
    }

    pub(crate) fn stream<S: Sink + ?Sized>(&mut self, value: &Value, sink: &mut S) {
        self.write_value(value, sink, 0);
    }

    fn write_value<S: Sink + ?Sized>(&mut self, value: &Value, sink: &mut S, depth: usize) {
        match value {
            Value::Null => sink.write("null"),
            Value::Bool(true) => sink.write("true"),
            Value::Bool(false) => sink.write("false"),
            Value::Integer(n) => {
                self.number.clear();
                let _infallible = fmt::Write::write_fmt(&mut self.number, format_args!("{n}"));
                sink.write(&self.number);
            }
            Value::String(s) => write_string(sink, s.as_ref()),
            Value::Array(items) => {
                sink.write("[");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        sink.write(",");
                    }
                    self.write_value(item, sink, depth.saturating_add(1));
                }
                sink.write("]");
            }
            Value::Object(members) => {
                self.order_members(members, depth);
                sink.write("{");
                let mut position = 0_usize;
                loop {
                    let member = self
                        .order
                        .get(depth)
                        .and_then(|level| level.get(position))
                        .and_then(|&index| members.get(index));
                    let Some((key, value)) = member else { break };
                    if position > 0 {
                        sink.write(",");
                    }
                    write_string(sink, key);
                    sink.write(":");
                    self.write_value(value, sink, depth.saturating_add(1));
                    position = position.saturating_add(1);
                }
                sink.write("}");
            }
        }
    }

    fn order_members(&mut self, members: &[(String, Value)], depth: usize) {
        while self.order.len() <= depth {
            self.order.push(Vec::new());
        }
        let Some(level) = self.order.get_mut(depth) else {
            return;
        };
        level.clear();
        level.extend(0..members.len());
        level.sort_by(|&a, &b| {
            let left = members.get(a).map_or("", |(key, _)| key);
            let right = members.get(b).map_or("", |(key, _)| key);
            utf16_cmp(left, right)
        });
    }
}

/// Writes one string as canonical JSON, including its quotes.
pub fn write_string<S: Sink + ?Sized>(sink: &mut S, s: &str) {
    sink.write("\"");
    let mut plain = 0_usize;
    for (index, c) in s.char_indices() {
        let escape: Option<&str> = match c {
            '"' => Some("\\\""),
            '\\' => Some("\\\\"),
            c if u32::from(c) < 0x20 => Some(control_escape(c)),
            _ => None,
        };
        if let Some(text) = escape {
            if let Some(run) = s.get(plain..index) {
                sink.write(run);
            }
            sink.write(text);
            plain = index.saturating_add(c.len_utf8());
        }
    }
    if let Some(run) = s.get(plain..) {
        sink.write(run);
    }
    sink.write("\"");
}

/// The `\u00xx` form for the raw control characters without a short escape,
/// as a static piece so streaming stays allocation-free.
fn control_escape(c: char) -> &'static str {
    const FORMS: [&str; 32] = [
        "\\u0000", "\\u0001", "\\u0002", "\\u0003", "\\u0004", "\\u0005", "\\u0006", "\\u0007",
        "\\b", "\\t", "\\n", "\\u000b", "\\f", "\\r", "\\u000e", "\\u000f", "\\u0010", "\\u0011",
        "\\u0012", "\\u0013", "\\u0014", "\\u0015", "\\u0016", "\\u0017", "\\u0018", "\\u0019",
        "\\u001a", "\\u001b", "\\u001c", "\\u001d", "\\u001e", "\\u001f",
    ];
    FORMS
        .get(usize::try_from(u32::from(c)).unwrap_or(0))
        .copied()
        .unwrap_or("\\u0000")
}
