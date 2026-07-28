use std::collections::BTreeSet;

/// The literal paths one tracked ignore file names, split by what they name.
/// Only anchored lines without pattern syntax are read, so the set answers
/// membership rather than matching: a wildcard would let one line clear an
/// unbounded number of references.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Declarations {
    files: BTreeSet<Vec<u8>>,
    directories: BTreeSet<Vec<u8>>,
}

const REJECTED: [u8; 5] = *b"*?[]\\";

impl Declarations {
    /// Reads one ignore file's body. A line qualifies when it begins with `/`,
    /// carries no pattern or escape byte, is neither a comment nor a negation,
    /// and spells a path with no empty, `.`, or `..` segment.
    #[must_use]
    pub fn parse(body: &[u8]) -> Self {
        let mut declarations = Self::default();
        for raw in body.split(|byte| *byte == b'\n') {
            let line = raw.strip_suffix(b"\r").unwrap_or(raw);
            let trimmed = trim_trailing_blanks(line);
            let Some(anchored) = qualifying(trimmed) else {
                continue;
            };
            let directory = anchored.ends_with(b"/");
            let spelled = anchored.strip_suffix(b"/").unwrap_or(anchored);
            if !well_formed(spelled) {
                continue;
            }
            if directory {
                declarations.directories.insert(spelled.to_vec());
            } else {
                declarations.files.insert(spelled.to_vec());
            }
        }
        declarations
    }

    /// Whether this file declares `relative`, a path spelled from the
    /// directory the ignore file sits in. A directory line answers for its
    /// descendants, which is the only line that covers more than one target
    /// and costs the repository every path beneath it.
    #[must_use]
    pub fn declares(&self, relative: &[u8]) -> bool {
        if self.files.contains(relative) || self.directories.contains(relative) {
            return true;
        }
        relative
            .iter()
            .enumerate()
            .filter(|(_, byte)| **byte == b'/')
            .any(|(index, _)| {
                relative
                    .get(..index)
                    .is_some_and(|prefix| self.directories.contains(prefix))
            })
    }
}

fn trim_trailing_blanks(line: &[u8]) -> &[u8] {
    let end = line
        .iter()
        .rposition(|byte| *byte != b' ' && *byte != b'\t')
        .map_or(0, |index| index.saturating_add(1));
    line.get(..end).unwrap_or_default()
}

fn qualifying(line: &[u8]) -> Option<&[u8]> {
    if line.first() != Some(&b'/') || line.iter().any(|byte| REJECTED.contains(byte)) {
        return None;
    }
    line.get(1..).filter(|rest| !rest.is_empty())
}

fn well_formed(spelled: &[u8]) -> bool {
    !spelled.is_empty()
        && spelled
            .split(|byte| *byte == b'/')
            .all(|segment| !segment.is_empty() && segment != b"." && segment != b"..")
}
