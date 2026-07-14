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
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Code,
    Template,
    Single,
    Double,
    LineComment,
    BlockComment,
}

/// Decides whether a chunk of embedded JavaScript can end where the parser is
/// standing. This is what makes an opaque interval correct: every `}` is
/// offered as a candidate close, and a `}` inside a string, a template, or a
/// comment must not end the region. The chunk can end when no literal or
/// comment is open and every bracket has been closed.
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

    while let Some(&byte) = bytes.get(at) {
        let next = bytes.get(at.saturating_add(1)).copied();
        let mut step = 1_usize;
        match mode {
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
                b'(' => frames.push(Frame::Paren),
                b'[' => frames.push(Frame::Bracket),
                b'{' => frames.push(Frame::Brace),
                b')' => close(&mut frames, Frame::Paren),
                b']' => close(&mut frames, Frame::Bracket),
                b'}' => {
                    if frames.last() == Some(&Frame::Substitution) {
                        frames.pop();
                        mode = Mode::Template;
                    } else {
                        close(&mut frames, Frame::Brace);
                    }
                }
                _ => {}
            },
            Mode::Template => match byte {
                b'\\' => step = 2,
                b'`' => mode = Mode::Code,
                b'$' if next == Some(b'{') => {
                    frames.push(Frame::Substitution);
                    mode = Mode::Code;
                    step = 2;
                }
                _ => {}
            },
            Mode::Single | Mode::Double => {
                let quote = if mode == Mode::Single { b'\'' } else { b'"' };
                if byte == b'\\' {
                    step = 2;
                } else if byte == quote {
                    mode = Mode::Code;
                }
            }
            Mode::LineComment => {
                if byte == b'\n' {
                    mode = Mode::Code;
                }
            }
            Mode::BlockComment => {
                if byte == b'*' && next == Some(b'/') {
                    mode = Mode::Code;
                    step = 2;
                }
            }
        }
        at = at.saturating_add(step);
    }

    if mode == Mode::Code && frames.is_empty() {
        Completeness::Complete
    } else {
        Completeness::Incomplete
    }
}

fn close(frames: &mut Vec<Frame>, expected: Frame) {
    if frames.last() == Some(&expected) {
        frames.pop();
    }
}
