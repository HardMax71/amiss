#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration assertions over the generated-identity fixtures"
)]

use std::collections::BTreeMap;

use amiss_fixtures::CommitChain;
use amiss_git::Repository;
use amiss_scan::anchor::DECLARATIONS;
use amiss_scan::pipeline::commit_pair;
use amiss_scan::route::{ROUTERS, Spelling};
use amiss_wire::model::{ObjectFormat, Oid, RepoPath};
use amiss_wire::report::model::occurrences;
use amiss_wire::resolution::{Missing, Resolution, Target, UnsupportedSemantics};

use crate::surface::{bare_shell, engine};

type Answers = BTreeMap<(String, u64), Resolution<RepoPath>>;

/// Every candidate occurrence of a one-commit fixture, keyed by the document
/// that writes it and the line it sits on.
fn answers(chain: &CommitChain) -> Answers {
    let commit = chain.commits.first().expect("the fixture holds one commit");
    let oid = Oid::new(ObjectFormat::Sha1, commit.id.clone()).expect("a SHA-1 commit name");
    let repo = Repository::open(chain.root(), ObjectFormat::Sha1).expect("the fixture opens");
    let built =
        commit_pair(&repo, &engine(), None, &bare_shell(), &oid, &oid).expect("the scan runs");
    assert!(
        built.envelope.payload.result.complete,
        "{:?}",
        built.envelope.payload.errors
    );
    built
        .envelope
        .payload
        .observations
        .iter()
        .filter_map(|row| occurrences(row).candidate)
        .map(|occurrence| {
            let document = occurrence
                .observation_id_input
                .document
                .as_str()
                .expect("fixture documents are text")
                .to_owned();
            (
                (document, occurrence.source_span.start_line),
                occurrence.resolution.clone(),
            )
        })
        .collect()
}

fn answer<'a>(answers: &'a Answers, document: &str, line: u64) -> &'a Resolution<RepoPath> {
    answers
        .get(&(document.to_owned(), line))
        .unwrap_or_else(|| panic!("{document}:{line} is an extracted reference"))
}

fn blob(resolution: &Resolution<RepoPath>) -> Option<&str> {
    let Resolution::Resolved {
        target: Target::Blob(blob),
    } = resolution
    else {
        return None;
    };
    blob.path.as_str()
}

/// A page whose body is a generator instruction publishes identities built
/// from something outside the tree, so the set this engine reads is
/// incomplete: the heading the page writes itself still answers, and an
/// anchor only the generator knows is declared rather than reported absent.
/// An ordinary page under the same site still proves absence, an admonition
/// fence names no generator, and the same instruction where no `mkdocs.yml`
/// governs it includes nothing.
#[test]
fn a_generator_instruction_leaves_the_identity_set_incomplete() {
    let chain = amiss_fixtures::mkdocs_generated().expect("the fixture stages");
    let rows = answers(&chain);
    assert_eq!(
        blob(answer(&rows, "README.md", 1)),
        Some("site/docs/api.md"),
        "the page writes its own title"
    );
    assert!(
        matches!(
            answer(&rows, "README.md", 3),
            Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(_))
        ),
        "the generated identity: {:?}",
        answer(&rows, "README.md", 3)
    );
    for (line, place) in [
        (5, "an ordinary page under the same site"),
        (7, "an admonition fence"),
        (9, "an instruction no mkdocs governs"),
    ] {
        assert!(
            matches!(
                answer(&rows, "README.md", line),
                Resolution::Missing(Missing::HeadingAnchorNotFound { .. })
            ),
            "{place}: {:?}",
            answer(&rows, "README.md", line)
        );
    }
}

/// Under a `conf.py`, a `MyST` role is the reference it spells: a docname takes
/// the profile's suffix and resolves like any path, a label resolves against
/// the names the tree declares, a docname or label nothing answers is
/// missing, a source-root docname stays a declared site route, and every
/// other role names a domain inventory outside the tree. The target a page
/// declares is an identity as well as a label, and the same role where no
/// `conf.py` governs it is prose.
#[test]
fn a_myst_role_is_read_only_where_sphinx_is_declared() {
    let chain = amiss_fixtures::sphinx_myst().expect("the fixture stages");
    let rows = answers(&chain);
    let index = "docs/index.md";
    for line in [3, 9, 15] {
        assert_eq!(
            blob(answer(&rows, index, line)),
            Some("docs/quickstart.md"),
            "line {line}"
        );
    }
    assert!(
        matches!(
            answer(&rows, index, 5),
            Resolution::Missing(Missing::PathNotFound { .. })
        ),
        "{:?}",
        answer(&rows, index, 5)
    );
    assert!(
        matches!(
            answer(&rows, index, 11),
            Resolution::Missing(Missing::LabelNotDeclared)
        ),
        "{:?}",
        answer(&rows, index, 11)
    );
    assert!(
        matches!(
            answer(&rows, index, 7),
            Resolution::UnsupportedSemantics(UnsupportedSemantics::SiteRoute)
        ),
        "{:?}",
        answer(&rows, index, 7)
    );
    assert!(
        matches!(
            answer(&rows, index, 13),
            Resolution::UnsupportedSemantics(UnsupportedSemantics::ExternalInventory)
        ),
        "{:?}",
        answer(&rows, index, 13)
    );
    assert!(
        !rows
            .keys()
            .any(|(document, _)| document == "outside/notes.md"),
        "a brace before a code span is prose where no conf.py governs it"
    );
}

/// The `MyST` rows and the route rule that anchors a source-root docname are
/// the same declaration, so the file each names must stay one file.
#[test]
fn the_myst_rows_name_the_file_the_route_table_reads() {
    let routed: Vec<&str> = ROUTERS
        .iter()
        .filter(|rule| rule.serves(Spelling::SourceRoot))
        .flat_map(|rule| rule.declared_by.iter().copied())
        .collect();
    let declared: Vec<&str> = DECLARATIONS
        .iter()
        .filter(|rule| rule.name == "myst-role")
        .flat_map(|rule| rule.declared_by.iter().copied())
        .collect();
    assert_eq!(routed, declared, "the Sphinx declaration is one file");
    assert!(!routed.is_empty(), "the route table declares Sphinx");
}
