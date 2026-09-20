use std::collections::BTreeSet;

/// What one tracked ignore file says its own directory does not keep, split by
/// what each line names. Only lines whose reach the line itself bounds are
/// read: a literal that a slash anchors, and the bare `*` that empties the
/// file's own directory. A pattern reaching wherever the tree happens to match
/// it would let one line clear an unbounded number of references.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Declarations {
    files: BTreeSet<Vec<u8>>,
    directories: BTreeSet<Vec<u8>>,
    kept: BTreeSet<Vec<u8>>,
    whole_directory: bool,
}

const REJECTED: [u8; 5] = *b"*?[]\\";
const EVERYTHING: &[u8] = b"*";

impl Declarations {
    /// Reads one ignore file's body. A line qualifies when a slash anchors it
    /// to this directory, at the front or in the middle, it carries no pattern
    /// or escape byte, it is no comment, and it spells a path with no empty,
    /// `.`, or `..` segment. A line that is bare `*` says the directory holds
    /// nothing tracked, so the negations are read as the counter-evidence they
    /// are: one that names a single literal entry keeps that entry, and one
    /// whose reach this cannot bound drops the claim.
    #[must_use]
    pub fn parse(body: &[u8]) -> Self {
        let mut declarations = Self::default();
        let mut empties = false;
        let mut unbounded = false;
        for raw in body.split(|byte| *byte == b'\n') {
            let line = raw.strip_suffix(b"\r").unwrap_or(raw);
            let trimmed = trim_trailing_blanks(line);
            if trimmed == EVERYTHING {
                empties = true;
                continue;
            }
            if let Some(negated) = trimmed.strip_prefix(b"!") {
                match kept_entry(negated) {
                    Some(entry) => {
                        declarations.kept.insert(entry.to_vec());
                    }
                    None => unbounded = true,
                }
                continue;
            }
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
        declarations.whole_directory = empties && !unbounded;
        declarations
    }

    /// Whether this file declares `relative`, a path spelled from the
    /// directory the ignore file sits in. A directory line answers for its
    /// descendants and the bare `*` for the whole directory, which are the
    /// only lines that cover more than one target and cost the repository
    /// every path beneath them.
    #[must_use]
    pub fn declares(&self, relative: &[u8]) -> bool {
        if self.whole_directory && !re_included(&self.kept, relative) {
            return true;
        }
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
    if line.first() == Some(&b'#') || line.iter().any(|byte| REJECTED.contains(byte)) {
        return None;
    }
    let anchor = line.iter().position(|byte| *byte == b'/')?;
    if anchor >= line.len().saturating_sub(1) {
        return None;
    }
    Some(line.strip_prefix(b"/").unwrap_or(line))
}

/// The one entry a negation puts back, when it names a single literal entry of
/// the emptied directory. An entry deeper than one segment cannot come back
/// past an excluded parent, and a pattern reaches further than this can say.
fn kept_entry(negated: &[u8]) -> Option<&[u8]> {
    let anchored = negated.strip_prefix(b"/").unwrap_or(negated);
    let spelled = anchored.strip_suffix(b"/").unwrap_or(anchored);
    let single = !spelled.contains(&b'/') && !spelled.iter().any(|byte| REJECTED.contains(byte));
    (single && well_formed(spelled)).then_some(spelled)
}

fn re_included(kept: &BTreeSet<Vec<u8>>, relative: &[u8]) -> bool {
    let entry = relative
        .split(|byte| *byte == b'/')
        .next()
        .unwrap_or_default();
    kept.contains(entry)
}

fn well_formed(spelled: &[u8]) -> bool {
    !spelled.is_empty()
        && spelled
            .split(|byte| *byte == b'/')
            .all(|segment| !segment.is_empty() && segment != b"." && segment != b"..")
}
