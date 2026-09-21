mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Completeness {
    Complete,
    Incomplete,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Frame {
    Paren,
    Bracket,
    Brace,
    Substitution,
    TagExpression(usize),
    TextExpression(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Code,
    Template,
    Single,
    Double,
    LineComment,
    BlockComment,
    Tag,
    TagQuote,
    Text,
}

/// Decides whether a chunk of embedded JavaScript can end where the parser is
/// standing. This is what makes an opaque interval correct: every `}` is
/// offered as a candidate close, and a `}` inside a string, a template, or a
/// comment must not end the region. The chunk can end when no literal or
/// comment is open and every bracket has been closed.
///
/// An expression inside JSX holds elements of its own, so the count of open
/// elements is saved with the frame the expression opens and restored when it
/// closes. JSX is read apart from the code around it, because the text
/// between tags is prose: an apostrophe in `we've` opens no string there, and a quote only
/// bounds an attribute value inside a tag. An element opens where `<` meets a
/// name, a slash or the `>` of a fragment and the byte before it cannot end
/// an expression, which is what keeps `a < b` and `useState<string>` code.
///
/// Only the lexical grammar is read, never the syntax, so this never judges
/// whether the JavaScript is valid. A `/` is always division and never opens a
/// regular expression, which means a `}` inside a regular-expression literal at
/// bracket depth zero would close the region one character early. Telling the
/// two apart needs the token before the slash, and guessing it the other way
/// would swallow the rest of the document, which is the worse failure.
pub(crate) fn completeness(source: &str) -> Completeness {
    let bytes = source.as_bytes();
    let mut frames: Vec<Frame> = Vec::new();
    let mut mode = Mode::Code;
    let mut at = 0_usize;
    let mut depth = 0_usize;
    let mut closing = false;
    let mut quote = 0_u8;
    let mut previous = 0_u8;

    while let Some(&byte) = bytes.get(at) {
        let next = bytes.get(at.saturating_add(1)).copied();
        let mut step = 1_usize;
        match mode {
            Mode::Code | Mode::Text
                if byte == b'<' && (mode == Mode::Text || opens_element(next, previous)) =>
            {
                closing = next == Some(b'/');
                step = if closing { 2 } else { 1 };
                mode = Mode::Tag;
            }
            Mode::Code => match byte {
                b'\'' => mode = Mode::Single,
                b'"' => mode = Mode::Double,
                b'`' => mode = Mode::Template,
                b'/' if next == Some(b'/') => {
                    mode = Mode::LineComment;
                    step = 2;
                }
                b'/' if next == Some(b'*') => {
                    mode = Mode::BlockComment;
                    step = 2;
                }
                b'(' | b'[' | b'{' => frames.push(opened(byte)),
                b')' => close(&mut frames, Frame::Paren),
                b']' => close(&mut frames, Frame::Bracket),
                b'}' => match frames.pop() {
                    Some(Frame::Substitution) => mode = Mode::Template,
                    Some(Frame::TagExpression(outer)) => {
                        depth = outer;
                        mode = Mode::Tag;
                    }
                    Some(Frame::TextExpression(outer)) => {
                        depth = outer;
                        mode = Mode::Text;
                    }
                    popped => reclose(&mut frames, popped),
                },
                _ => {}
            },
            Mode::Tag => match byte {
                b'\'' | b'"' => {
                    quote = byte;
                    mode = Mode::TagQuote;
                }
                b'{' => {
                    frames.push(Frame::TagExpression(depth));
                    depth = 0;
                    mode = Mode::Code;
                }
                b'/' if next == Some(b'>') => {
                    mode = inside(depth);
                    step = 2;
                }
                b'>' => {
                    depth = if closing {
                        depth.saturating_sub(1)
                    } else {
                        depth.saturating_add(1)
                    };
                    mode = inside(depth);
                }
                _ => {}
            },
            Mode::TagQuote => {
                if byte == quote {
                    mode = Mode::Tag;
                }
            }
            Mode::Text => {
                if byte == b'{' {
                    frames.push(Frame::TextExpression(depth));
                    depth = 0;
                    mode = Mode::Code;
                }
            }
            Mode::Template
            | Mode::Single
            | Mode::Double
            | Mode::LineComment
            | Mode::BlockComment => step = literal(&mut mode, &mut frames, byte, next),
        }
        if mode == Mode::Code && !byte.is_ascii_whitespace() {
            previous = byte;
        }
        at = at.saturating_add(step);
    }

    if mode == Mode::Code && frames.is_empty() {
        Completeness::Complete
    } else {
        Completeness::Incomplete
    }
}

/// One byte of a string, a template, or a comment, none of which reads the
/// code inside it. Answers how many bytes to step over.
fn literal(mode: &mut Mode, frames: &mut Vec<Frame>, byte: u8, next: Option<u8>) -> usize {
    match *mode {
        Mode::Template if byte == b'$' && next == Some(b'{') => {
            frames.push(Frame::Substitution);
            *mode = Mode::Code;
            return 2;
        }
        Mode::Template if byte == b'`' => *mode = Mode::Code,
        Mode::Single if byte == b'\'' => *mode = Mode::Code,
        Mode::Double if byte == b'"' => *mode = Mode::Code,
        Mode::LineComment if byte == b'\n' => *mode = Mode::Code,
        Mode::BlockComment if byte == b'*' && next == Some(b'/') => {
            *mode = Mode::Code;
            return 2;
        }
        Mode::Template | Mode::Single | Mode::Double if byte == b'\\' => return 2,
        Mode::Code
        | Mode::Tag
        | Mode::TagQuote
        | Mode::Text
        | Mode::Template
        | Mode::Single
        | Mode::Double
        | Mode::LineComment
        | Mode::BlockComment => {}
    }
    1
}

/// The frame an opening bracket pushes.
fn opened(byte: u8) -> Frame {
    match byte {
        b'(' => Frame::Paren,
        b'[' => Frame::Bracket,
        _ => Frame::Brace,
    }
}

/// A frame a brace did not open is put back, since only the brace it opened
/// closes it.
fn reclose(frames: &mut Vec<Frame>, popped: Option<Frame>) {
    if let Some(frame) = popped.filter(|frame| *frame != Frame::Brace) {
        frames.push(frame);
    }
}

/// Where a finished tag leaves the reading: inside the element it opened, or
/// back in the code that held the outermost one.
fn inside(depth: usize) -> Mode {
    if depth == 0 { Mode::Code } else { Mode::Text }
}

/// Whether a `<` opens a JSX element rather than a comparison: a name, a
/// closing slash or the `>` of a fragment follows it, and the byte before it
/// cannot end an expression, since no element may follow a value.
fn opens_element(next: Option<u8>, previous: u8) -> bool {
    let opens = next.is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'/' || byte == b'>');
    let value = previous.is_ascii_alphanumeric()
        || previous == b'_'
        || previous == b'$'
        || previous == b')'
        || previous == b']';
    opens && !value
}

fn close(frames: &mut Vec<Frame>, expected: Frame) {
    if frames.last() == Some(&expected) {
        frames.pop();
    }
}
