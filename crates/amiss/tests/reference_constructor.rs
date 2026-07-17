#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "conformance harness over asserted vector shapes"
)]

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use amiss::invocation::{Outcome, parse};
use amiss_git::{GitLimits, GitResources, ObjectKind, Repository, parse_commit};
use amiss_scan::resolve::{ForgeContext, TargetCache};
use amiss_scan::{
    DocumentStatus, Intent, Resolution, ScanLimits, ScanResources, SnapshotDiscovery, discover,
    resolve,
};
use amiss_wire::controls::SourceConstruct;
use amiss_wire::model::{ForgeDialect, ObjectFormat, Oid, RepoPath};
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::{
    ExternalReference, InvalidReference, Missing, Target, UnsupportedSemantics, VersionScope,
};
use serde_json::Value;
use strum::IntoDiscriminant;

struct Bed {
    _pair: amiss_fixtures::CommitPair,
    repo: Repository,
    git_resources: GitResources,
    scan_resources: ScanResources,
    cache: TargetCache,
    discovery: SnapshotDiscovery,
}

impl Bed {
    fn new() -> Self {
        let pair = amiss_fixtures::commit_pair(
            &[
                ("README.md", "# R\n"),
                (
                    "docs/a.md",
                    "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n13\n14\n15\n16\n17\n18\n19\n20\n",
                ),
                ("docs/file.md", "# F\n"),
                ("src/a.scala", "object A\n"),
                ("auto/uri.md", "<https://example.com/a?b#c>\n"),
                ("auto/email.md", "<foo@example.com>\n"),
                ("auto/protocol.md", "visit https://example.com/a now\n"),
                ("auto/www.md", "visit www.example.com/a now\n"),
                ("auto/gfm-email.md", "mail foo.bar@example.com now\n"),
            ],
            &[],
        )
        .unwrap();
        let repo = Repository::open(Path::new(&pair.repo), ObjectFormat::Sha1).unwrap();
        let mut git_resources = GitResources::new(GitLimits::CONTRACT);
        let commit_oid = Oid::new(ObjectFormat::Sha1, pair.candidate.clone()).unwrap();
        let commit_object = repo
            .read_expected(&mut git_resources, &commit_oid, ObjectKind::Commit)
            .unwrap();
        let commit = parse_commit(ObjectFormat::Sha1, &commit_object.body).unwrap();
        let mut scan_resources = ScanResources::new(ScanLimits::CONTRACT);
        let discovery = discover(
            &repo,
            &mut git_resources,
            &mut scan_resources,
            &amiss_scan::Includes::default(),
            &commit.tree,
        )
        .unwrap();
        Self {
            _pair: pair,
            repo,
            git_resources,
            scan_resources,
            cache: TargetCache::default(),
            discovery,
        }
    }

    fn run(
        &mut self,
        context: Option<&ForgeContext>,
        source: &str,
        is_image: bool,
        destination: &str,
    ) -> (Intent, Resolution) {
        let document = RepoPath::new(source.to_owned()).unwrap();
        resolve(
            &self.repo,
            &mut self.git_resources,
            &mut self.scan_resources,
            &mut self.cache,
            &self.discovery,
            context,
            &document,
            is_image,
            destination,
        )
        .unwrap()
    }

    fn autolink_destination(&self, document: &str) -> String {
        let path = RepoPath::new(document.to_owned()).unwrap();
        let record = self
            .discovery
            .documents
            .iter()
            .find(|record| record.path == path)
            .unwrap();
        let DocumentStatus::Scanned(scanned) = &record.status else {
            panic!("{document} is not a scanned document");
        };
        let occurrence = scanned
            .occurrences
            .iter()
            .find(|occurrence| occurrence.occurrence.construct == SourceConstruct::Autolink)
            .unwrap();
        occurrence.occurrence.semantic_destination.clone()
    }
}

fn context(
    dialect: ForgeDialect,
    host: &str,
    owner: &str,
    name: &str,
    candidate_ref: &str,
    default_ref: &str,
) -> ForgeContext {
    ForgeContext {
        host: host.to_owned(),
        dialect,
        owner: owner.to_owned(),
        repository: name.to_owned(),
        candidate_ref: candidate_ref.to_owned(),
        default_ref: default_ref.to_owned(),
        candidate_oid: None,
    }
}

fn text<'a>(case: &'a Value, key: &str) -> &'a str {
    case.get(key).and_then(Value::as_str).unwrap()
}

fn dialect_of(case: &Value) -> ForgeDialect {
    match text(case, "dialect") {
        "github" => ForgeDialect::Github,
        "gitlab" => ForgeDialect::Gitlab,
        "gitea" => ForgeDialect::Gitea,
        other => panic!("unknown dialect {other}"),
    }
}

fn split_case(bed: &mut Bed, case: &Value, id: &str) {
    let operation = text(case, "operation");
    let form_key = if operation == "gitlab-ref-split" {
        "gitlab_form"
    } else {
        "github_form"
    };
    let form = case.get(form_key).and_then(Value::as_str).unwrap_or("blob");
    let suffix = text(case, "encoded_suffix");
    let (dialect, host, url) = match operation {
        "gitlab-ref-split" => (
            ForgeDialect::Gitlab,
            "gitlab.com",
            format!("https://gitlab.com/acme/widgets/-/{form}/{suffix}"),
        ),
        "gitea-branch-split" => (
            ForgeDialect::Gitea,
            "codeberg.org",
            format!("https://codeberg.org/acme/widgets/src/branch/{suffix}"),
        ),
        "gitea-commit-split" => (
            ForgeDialect::Gitea,
            "codeberg.org",
            format!(
                "https://codeberg.org/acme/widgets/src/commit/{}/{suffix}",
                text(case, "oid_segment")
            ),
        ),
        _ => (
            ForgeDialect::Github,
            "github.com",
            format!("https://github.com/acme/widgets/{form}/{suffix}"),
        ),
    };
    let mut run_context = context(
        dialect,
        host,
        "acme",
        "widgets",
        case.get("candidate_ref")
            .and_then(Value::as_str)
            .unwrap_or("refs/heads/main"),
        case.get("default_ref")
            .and_then(Value::as_str)
            .unwrap_or("refs/heads/main"),
    );
    run_context.candidate_oid = case
        .get("candidate_oid")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let (intent, row) = bed.run(Some(&run_context), "README.md", false, &url);
    let expected = case.get("expected").unwrap();
    let expected_path = expected.get("path").and_then(Value::as_str);
    match text(expected, "status") {
        "candidate" => {
            let Resolution::Resolved(target) = &row else {
                panic!("{id}: the candidate ref did not resolve: {row:?}");
            };
            let path = match target {
                Target::Tree { path } => path,
                Target::Blob(blob) => &blob.path,
            };
            assert_eq!(path.as_str(), expected_path, "{id}");
            assert_eq!(
                intent
                    .repository_path
                    .as_ref()
                    .and_then(|path| path.as_str()),
                expected_path,
                "{id}"
            );
        }
        "unsupported-version-scope" => match (&row, expected_path) {
            (
                Resolution::UnsupportedVersion(VersionScope::KnownPath { path }),
                Some(expected_path),
            ) => assert_eq!(path.as_str(), Some(expected_path), "{id}"),
            (Resolution::UnsupportedVersion(VersionScope::UnknownPath), None) => {}
            _ => panic!("{id}: unexpected version-scoped outcome: {row:?}"),
        },
        "invalid" => {
            assert!(
                matches!(&row, Resolution::Invalid(_)),
                "{id}: expected an invalid outcome, got {row:?}"
            );
            assert_eq!(expected_path, None, "{id}");
        }
        other => panic!("{id}: unknown split status {other}"),
    }
}

fn line_fragment_case(bed: &mut Bed, case: &Value, id: &str) {
    let value = text(case, "value");
    let (run_context, url) = if text(case, "operation") == "gitlab-line-fragment" {
        (
            context(
                ForgeDialect::Gitlab,
                "gitlab.com",
                "acme",
                "widgets",
                "refs/heads/main",
                "refs/heads/main",
            ),
            format!("https://gitlab.com/acme/widgets/-/blob/main/docs/a.md#{value}"),
        )
    } else if text(case, "operation") == "gitea-line-fragment" {
        (
            context(
                ForgeDialect::Gitea,
                "codeberg.org",
                "acme",
                "widgets",
                "refs/heads/main",
                "refs/heads/main",
            ),
            format!("https://codeberg.org/acme/widgets/src/branch/main/docs/a.md#{value}"),
        )
    } else {
        (
            context(
                ForgeDialect::Github,
                "github.com",
                "acme",
                "widgets",
                "refs/heads/main",
                "refs/heads/main",
            ),
            format!("https://github.com/acme/widgets/blob/main/docs/a.md#{value}"),
        )
    };
    let (_intent, row) = bed.run(Some(&run_context), "README.md", false, &url);
    let matches_boundary = if case.get("expected").and_then(Value::as_bool).unwrap() {
        matches!(&row, Resolution::Resolved(_))
    } else {
        matches!(
            &row,
            Resolution::UnsupportedSemantics(UnsupportedSemantics::Fragment(_))
        )
    };
    assert!(
        matches_boundary,
        "{id}: a document target classifies the fragment"
    );
}

fn identity_case(bed: &mut Bed, case: &Value, id: &str) {
    let operation = text(case, "operation");
    if operation == "github-identity" {
        let url = format!(
            "https://{}/{}/{}/blob/main/docs/a.md",
            text(case, "host"),
            text(case, "url_owner"),
            text(case, "url_repository")
        );
        let run_context = context(
            ForgeDialect::Github,
            "github.com",
            text(case, "identity_owner"),
            text(case, "identity_repository"),
            "refs/heads/main",
            "refs/heads/main",
        );
        let (intent, _row) = bed.run(Some(&run_context), "README.md", false, &url);
        let expected = case.get("expected").and_then(Value::as_bool).unwrap();
        assert_eq!(
            intent.kind == IntentKind::SameRepositoryGithub,
            expected,
            "{id}"
        );
        return;
    }
    let dialect = dialect_of(case);
    let run_context = context(
        dialect,
        text(case, "identity_host"),
        text(case, "identity_owner"),
        text(case, "identity_name"),
        "refs/heads/main",
        "refs/heads/main",
    );
    let (intent, row) = bed.run(Some(&run_context), "README.md", false, text(case, "url"));
    match text(case, "expected") {
        "same-repository" => assert!(
            matches!(
                intent.kind,
                IntentKind::SameRepositoryGithub
                    | IntentKind::SameRepositoryGitlab
                    | IntentKind::SameRepositoryGitea
            ),
            "{id}: got {:?}",
            intent.kind
        ),
        "foreign" => assert!(
            matches!(
                &row,
                Resolution::External(ExternalReference::ForeignRepository)
            ),
            "{id}: expected a foreign repository, got {row:?}"
        ),
        "external" => {
            assert!(
                matches!(&row, Resolution::External(ExternalReference::Url)),
                "{id}: expected an external URL, got {row:?}"
            );
            assert_eq!(intent.kind, IntentKind::ExternalUrl, "{id}");
        }
        other => panic!("{id}: unknown identity expectation {other}"),
    }
}

fn forge_form_case(bed: &mut Bed, case: &Value, id: &str) {
    let dialect = dialect_of(case);
    let host = match dialect {
        ForgeDialect::Github => "github.com",
        ForgeDialect::Gitlab => "gitlab.com",
        ForgeDialect::Gitea => "codeberg.org",
    };
    let mut run_context = context(
        dialect,
        host,
        "acme",
        "widgets",
        "refs/heads/main",
        "refs/heads/main",
    );
    run_context.candidate_oid = Some("6a66ef14b9b8b174a54ccf8ea4b0dd18f42f9f22".to_owned());
    let url = format!("https://{host}/{}", text(case, "suffix"));
    let (intent, row) = bed.run(Some(&run_context), "README.md", false, &url);
    match text(case, "expected") {
        "foreign" => assert!(
            matches!(
                &row,
                Resolution::External(ExternalReference::ForeignRepository)
            ),
            "{id}: expected a foreign repository, got {row:?}"
        ),
        "unsupported-version-scope" => assert!(
            matches!(&row, Resolution::UnsupportedVersion(_)),
            "{id}: expected an unsupported version, got {row:?}"
        ),
        expected => assert_eq!(
            intent
                .target_kind
                .map(amiss_wire::controls::TargetKind::as_str),
            Some(expected),
            "{id}"
        ),
    }
}

fn target_kind_case(bed: &mut Bed, case: &Value, id: &str) {
    let is_image = text(case, "construct").contains("image");
    let destination = match case.get("github_form").and_then(Value::as_str) {
        Some("tree") => "https://github.com/acme/widgets/tree/main/docs".to_owned(),
        Some(_) => "https://github.com/acme/widgets/blob/main/docs/a.md".to_owned(),
        None => {
            let trailing = case
                .get("trailing_slash")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if trailing {
                "docs/".to_owned()
            } else {
                "docs/a.md".to_owned()
            }
        }
    };
    let run_context = context(
        ForgeDialect::Github,
        "github.com",
        "acme",
        "widgets",
        "refs/heads/main",
        "refs/heads/main",
    );
    let (intent, _row) = bed.run(Some(&run_context), "README.md", is_image, &destination);
    assert_eq!(
        intent
            .target_kind
            .map(amiss_wire::controls::TargetKind::as_str),
        Some(text(case, "expected")),
        "{id}"
    );
}

fn boundary_case(bed: &mut Bed, case: &Value, id: &str) {
    let target = match text(case, "target_class") {
        "document" => "docs/a.md",
        "code" => "src/a.scala",
        other => panic!("{id}: unknown target class {other}"),
    };
    let mut destination = target.to_owned();
    if case.get("query_present").and_then(Value::as_bool).unwrap() {
        destination.push_str("?x");
    }
    if case
        .get("fragment_present")
        .and_then(Value::as_bool)
        .unwrap()
    {
        let line = case
            .get("github_line_fragment")
            .and_then(Value::as_bool)
            .unwrap();
        if line {
            destination.push('#');
            destination.push_str(
                case.get("line_fragment")
                    .and_then(Value::as_str)
                    .unwrap_or("L1"),
            );
        } else {
            destination.push_str("#sec");
        }
    }
    let (_intent, row) = bed.run(None, "README.md", false, &destination);
    let expected = case.get("expected").unwrap();
    assert_eq!(row.discriminant().as_ref(), text(expected, "kind"), "{id}");
    let expected_reason = expected.get("reason").and_then(Value::as_str);
    match &row {
        Resolution::Resolved(_) => assert_eq!(expected_reason, None, "{id}"),
        Resolution::Missing(missing @ Missing::LineFragmentOutOfRange { .. }) => {
            assert_eq!(
                Some(missing.discriminant().as_ref()),
                expected_reason,
                "{id}"
            );
        }
        Resolution::UnsupportedSemantics(semantics) => {
            assert_eq!(
                Some(semantics.discriminant().as_ref()),
                expected_reason,
                "{id}"
            );
        }
        Resolution::Missing(_)
        | Resolution::TypeMismatch(_)
        | Resolution::UnsupportedTarget(_)
        | Resolution::UnsupportedVersion(_)
        | Resolution::Invalid(_)
        | Resolution::External(_) => panic!("{id}: unexpected boundary outcome: {row:?}"),
    }
}

fn dialect_default_case(case: &Value, id: &str) {
    let host = text(case, "host");
    let mut tokens: Vec<String> = [
        "check",
        "--repo",
        ".",
        "--object-format",
        "sha1",
        "--base",
        &"a".repeat(40),
        "--candidate",
        &"b".repeat(40),
        "--profile",
        "observe",
        "--repository",
        &format!("{host}/acme/repo"),
        "--ref",
        "refs/heads/main",
        "--default-branch-ref",
        "refs/heads/main",
    ]
    .iter()
    .map(|token| (*token).to_owned())
    .collect();
    if let Some(flag) = case.get("flag").and_then(Value::as_str) {
        tokens.push("--forge".to_owned());
        tokens.push(flag.to_owned());
    }
    let argv: Vec<OsString> = tokens.iter().map(OsString::from).collect();
    let Outcome::Accepted(invocation) = parse(&argv) else {
        panic!("{id}: expected acceptance");
    };
    assert_eq!(
        invocation.forge.map(ForgeDialect::as_str),
        case.get("expected").and_then(Value::as_str),
        "{id}"
    );
}

fn dispatch(bed: &mut Bed, case: &Value) {
    let id = text(case, "id");
    match text(case, "operation") {
        "target-kind" => target_kind_case(bed, case, id),
        "github-line-fragment" | "gitlab-line-fragment" | "gitea-line-fragment" => {
            line_fragment_case(bed, case, id);
        }
        "github-ref-split" | "gitlab-ref-split" | "gitea-branch-split" | "gitea-commit-split" => {
            split_case(bed, case, id);
        }
        "github-identity" | "forge-identity" => identity_case(bed, case, id),
        "forge-form" => forge_form_case(bed, case, id),
        "forge-dialect-default" => dialect_default_case(case, id),
        "resolution-boundary" => boundary_case(bed, case, id),
        "empty-native-destination" => {
            let source = text(case, "source_document");
            let (_intent, row) = bed.run(None, source, false, "");
            let Resolution::Resolved(Target::Blob(blob)) = &row else {
                panic!("{id}: the empty destination did not resolve to its document: {row:?}");
            };
            assert_eq!(blob.path.as_str(), Some(text(case, "expected")), "{id}");
        }
        "external-scheme" => {
            let destination = format!("{}://example.com/a", text(case, "value"));
            let (intent, _row) = bed.run(None, "README.md", false, &destination);
            assert_eq!(
                intent.external_scheme.as_deref(),
                Some(text(case, "expected")),
                "{id}"
            );
        }
        "network-path" => {
            let (_intent, row) = bed.run(None, "README.md", false, text(case, "value"));
            let expected = case.get("expected").unwrap();
            assert_eq!(row.discriminant().as_ref(), text(expected, "kind"), "{id}");
            let Resolution::UnsupportedSemantics(semantics @ UnsupportedSemantics::NetworkPath) =
                &row
            else {
                panic!("{id}: unexpected network-path outcome: {row:?}");
            };
            assert_eq!(
                semantics.discriminant().as_ref(),
                text(expected, "reason"),
                "{id}"
            );
        }
        "semantic-autolink" => {
            let document = match text(case, "form") {
                "commonmark-uri" => "auto/uri.md",
                "commonmark-email" => "auto/email.md",
                "gfm-protocol" => "auto/protocol.md",
                "gfm-www" => "auto/www.md",
                "gfm-email" => "auto/gfm-email.md",
                other => panic!("{id}: unknown autolink form {other}"),
            };
            assert_eq!(
                bed.autolink_destination(document),
                text(case, "expected"),
                "{id}"
            );
        }
        "uri-components" => {
            let (intent, _row) = bed.run(None, "README.md", false, text(case, "value"));
            let expected = case.get("expected").unwrap();
            assert_eq!(
                intent
                    .repository_path
                    .as_ref()
                    .and_then(|path| path.as_str()),
                expected.get("path").and_then(Value::as_str),
                "{id}"
            );
            assert_eq!(
                intent.query.as_deref(),
                expected.get("query").and_then(Value::as_str),
                "{id}"
            );
            assert_eq!(
                intent.fragment.as_deref(),
                expected.get("fragment").and_then(Value::as_str),
                "{id}"
            );
        }
        "native-trailing-slash" => {
            let is_image = text(case, "construct").contains("image");
            let (_intent, row) = bed.run(None, "README.md", is_image, "docs/");
            let expected = case.get("expected").unwrap();
            assert_eq!(row.discriminant().as_ref(), text(expected, "kind"), "{id}");
            let Resolution::Invalid(invalid @ InvalidReference::Syntax) = &row else {
                panic!("{id}: unexpected trailing-slash outcome: {row:?}");
            };
            assert_eq!(invalid.as_ref(), text(expected, "reason"), "{id}");
        }
        other => panic!("unknown operation {other}: the harness must learn the contract"),
    }
}

/// Every case in the reference-constructor vectors, driven through the
/// public resolver and invocation surfaces. An operation the harness does
/// not know is a panic, so a vector added for a future dialect cannot be
/// silently skipped.
#[test]
fn the_reference_constructor_vectors_hold() {
    let raw = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../spec/examples/reference-constructor-vectors.json"),
    )
    .unwrap();
    let vectors: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(
        vectors.get("schema").and_then(Value::as_str),
        Some("amiss/reference-constructor-vectors")
    );
    assert_eq!(
        vectors.get("contract").and_then(Value::as_str),
        Some("reference-constructor")
    );
    let cases = vectors.get("cases").and_then(Value::as_array).unwrap();
    assert!(cases.len() >= 55, "the vector set only grows");
    let mut bed = Bed::new();
    for case in cases {
        dispatch(&mut bed, case);
    }
}
