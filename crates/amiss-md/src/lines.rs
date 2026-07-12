pub struct Lines<'a> {
    source: &'a [u8],
    at: usize,
}

/// The shared line scanner of `source-span accounting`: CRLF is a single
/// ending, and a bare CR or LF is also an ending. A trailing fragment with no
/// ending is still a line, marked `terminated = false`.
#[must_use]
pub const fn scan(source: &[u8]) -> Lines<'_> {
    Lines { source, at: 0 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    pub start: usize,
    pub content_end: usize,
    pub end: usize,
    pub terminated: bool,
}

impl Line {
    #[must_use]
    pub fn content<'a>(&self, source: &'a [u8]) -> &'a [u8] {
        source.get(self.start..self.content_end).unwrap_or_default()
    }

    /// A line is blank exactly when its content bytes are zero or only ASCII
    /// space and tab.
    #[must_use]
    pub fn is_blank(&self, source: &[u8]) -> bool {
        self.content(source)
            .iter()
            .all(|byte| matches!(*byte, b' ' | b'\t'))
    }
}

impl Iterator for Lines<'_> {
    type Item = Line;

    fn next(&mut self) -> Option<Line> {
        let start = self.at;
        if start >= self.source.len() {
            return None;
        }
        let mut cursor = start;
        while let Some(byte) = self.source.get(cursor) {
            let end = match *byte {
                b'\n' => cursor.saturating_add(1),
                b'\r' => {
                    let after = cursor.saturating_add(1);
                    if self.source.get(after) == Some(&b'\n') {
                        after.saturating_add(1)
                    } else {
                        after
                    }
                }
                _ => {
                    cursor = cursor.saturating_add(1);
                    continue;
                }
            };
            self.at = end;
            return Some(Line {
                start,
                content_end: cursor,
                end,
                terminated: true,
            });
        }
        self.at = self.source.len();
        Some(Line {
            start,
            content_end: self.source.len(),
            end: self.source.len(),
            terminated: false,
        })
    }
}
