use amiss_wire::model::RepoPath;

/// A spelling a router serves for a page whose source file is named
/// otherwise. Each one was harvested from the router itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spelling {
    Extensionless,
    OutputExtension,
    ReadmeIndex,
}

/// One router's route rule: the spellings it serves for a source file beyond
/// the source path itself. A router that serves none demands the source
/// spelling and adds nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteRule {
    pub name: &'static str,
    pub serves: &'static [Spelling],
}

impl RouteRule {
    #[must_use]
    pub fn serves(&self, spelling: Spelling) -> bool {
        self.serves.contains(&spelling)
    }
}

/// Every router rule the resolver knows. A spelling reaches a source file only
/// when that file is in the tree, so a rule can widen what resolves and can
/// never invent a target.
pub const ROUTERS: [RouteRule; 4] = [
    RouteRule {
        name: "mdbook",
        serves: &[Spelling::OutputExtension, Spelling::ReadmeIndex],
    },
    RouteRule {
        name: "vitepress",
        serves: &[Spelling::Extensionless, Spelling::OutputExtension],
    },
    RouteRule {
        name: "vitepress-readme",
        serves: &[
            Spelling::Extensionless,
            Spelling::OutputExtension,
            Spelling::ReadmeIndex,
        ],
    },
    RouteRule {
        name: "mkdocs",
        serves: &[],
    },
];

/// Every source path a modelled router would serve for this destination, in a
/// fixed order and without the destination itself.
#[must_use]
pub fn candidates(destination: &RepoPath) -> Vec<(Spelling, RepoPath)> {
    let mut out: Vec<(Spelling, RepoPath)> = Vec::new();
    for rule in &ROUTERS {
        for candidate in spellings(rule, destination) {
            if !out.iter().any(|(_, path)| *path == candidate.1) {
                out.push(candidate);
            }
        }
    }
    out
}

/// The source paths one rule would serve, in the order the resolver tries
/// them: the output name before the elided extension, and a directory's
/// README last.
#[must_use]
pub fn spellings(rule: &RouteRule, destination: &RepoPath) -> Vec<(Spelling, RepoPath)> {
    let raw = destination.as_bytes();
    let source = output_extension(raw);
    let mut out: Vec<(Spelling, RepoPath)> = Vec::new();
    let mut push = |spelling: Spelling, bytes: Option<Vec<u8>>| {
        let Some(path) = bytes.and_then(RepoPath::from_bytes) else {
            return;
        };
        if path.as_bytes() != raw && !out.iter().any(|(_, held)| *held == path) {
            out.push((spelling, path));
        }
    };
    if rule.serves(Spelling::OutputExtension) {
        push(Spelling::OutputExtension, source.clone());
    }
    if rule.serves(Spelling::Extensionless) {
        push(Spelling::Extensionless, extensionless(raw));
    }
    if rule.serves(Spelling::ReadmeIndex) {
        push(Spelling::ReadmeIndex, readme_index(raw));
        if rule.serves(Spelling::OutputExtension) {
            push(
                Spelling::ReadmeIndex,
                source.as_deref().and_then(readme_index),
            );
        }
    }
    out
}

fn output_extension(raw: &[u8]) -> Option<Vec<u8>> {
    let stem = raw.strip_suffix(b".html")?;
    (!stem.is_empty()).then(|| [stem, b".md"].concat())
}

fn extensionless(raw: &[u8]) -> Option<Vec<u8>> {
    let last = raw.rsplit(|byte| *byte == b'/').next()?;
    let named = !last.is_empty()
        && !last.ends_with(b".md")
        && !last.ends_with(b".markdown")
        && !last.ends_with(b".html");
    named.then(|| [raw, b".md"].concat())
}

/// Only a directory's index is answered by its README, because that is the
/// shape the harvest covered.
fn readme_index(raw: &[u8]) -> Option<Vec<u8>> {
    let cut = raw.iter().rposition(|byte| *byte == b'/')?;
    let head = raw.get(..=cut)?;
    let tail = raw.get(cut.saturating_add(1)..)?;
    (tail == b"index.md").then(|| [head, b"README.md"].concat())
}
