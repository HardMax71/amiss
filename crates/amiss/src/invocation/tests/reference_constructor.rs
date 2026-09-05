use std::ffi::OsString;
use std::fs;
use std::path::Path;

use amiss_git::{GitLimits, GitResources, ObjectKind, Repository, parse_commit};
use amiss_scan::resolve::{ForgeContext, Resolver, TargetCache};
use amiss_scan::{
    DocumentStatus, Intent, Resolution, ScanLimits, ScanResources, SnapshotDiscovery, discover,
};
use amiss_wire::controls::SourceConstruct;
use amiss_wire::model::{Adapter, ForgeDialect, ObjectFormat, Oid, RepoPath};
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::{
    ExternalReference, InvalidReference, Missing, Target, UnsupportedSemantics, VersionScope,
};
use serde_json::Value;
use strum::IntoDiscriminant;

use crate::invocation::{Outcome, parse};

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
        Resolver::new(
            &self.repo,
            &mut self.git_resources,
            &mut self.scan_resources,
            &mut self.cache,
            &self.discovery,
        )
        .resolve(context, Adapter::Markdown, &document, is_image, destination)
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
        object_format: ObjectFormat::Sha1,
        owner: owner.to_owned(),
        repository: name.to_owned(),
        candidate_ref: candidate_ref.to_owned(),
        default_ref: default_ref.to_owned(),
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
        "bitbucket-cloud" => ForgeDialect::BitbucketCloud,
        "bitbucket-data-center" => ForgeDialect::BitbucketDataCenter,
        other => panic!("unknown dialect {other}"),
    }
}

fn split_input(case: &Value) -> (ForgeContext, String) {
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
        "bitbucket-cloud-ref-split" => (
            ForgeDialect::BitbucketCloud,
            "bitbucket.org",
            format!("https://bitbucket.org/acme/widgets/src/{suffix}"),
        ),
        "bitbucket-data-center-ref-query" => {
            let mut url = format!(
                "https://bitbucket.example/bitbucket/projects/ACME/repos/widgets/browse/{suffix}"
            );
            if let Some(query) = case.get("query").and_then(Value::as_str) {
                url.push('?');
                url.push_str(query);
            }
            (ForgeDialect::BitbucketDataCenter, "bitbucket.example", url)
        }
        _ => (
            ForgeDialect::Github,
            "github.com",
            format!("https://github.com/acme/widgets/{form}/{suffix}"),
        ),
    };
    let run_context = context(
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
    (run_context, url)
}

fn assert_split_outcome(intent: &Intent, row: &Resolution, expected: &Value, id: &str) {
    let expected_path = expected.get("path").and_then(Value::as_str);
    let expected_commit = expected.get("commit_oid").and_then(Value::as_str);
    match text(expected, "status") {
        "candidate" => {
            let Resolution::Resolved { target } = &row else {
                panic!("{id}: the candidate ref did not resolve: {row:?}");
            };
            let path = match target {
                Target::Tree { path } => path,
                Target::Blob(blob) => &blob.path,
            };
            assert_eq!(path.as_str(), expected_path, "{id}");
            assert_eq!(
                intent.repository_path.as_ref().and_then(RepoPath::as_str),
                expected_path,
                "{id}"
            );
        }
        "unsupported-version-scope" => {
            let Resolution::UnsupportedVersion { scope } = &row else {
                panic!("{id}: unexpected version-scoped outcome: {row:?}");
            };
            match scope {
                VersionScope::KnownPath { path } => {
                    assert_eq!(path.as_str(), expected_path, "{id}");
                    assert_eq!(expected_commit, None, "{id}");
                }
                VersionScope::KnownCommit { commit_oid, path } => {
                    assert_eq!(Some(commit_oid.as_str()), expected_commit, "{id}");
                    assert_eq!(path.as_str(), expected_path, "{id}");
                }
                VersionScope::UnknownPath => {
                    assert_eq!(expected_path, None, "{id}");
                    assert_eq!(expected_commit, None, "{id}");
                }
            }
        }
        "invalid" => {
            assert!(
                matches!(&row, Resolution::Invalid { .. }),
                "{id}: expected an invalid outcome, got {row:?}"
            );
            assert_eq!(expected_path, None, "{id}");
        }
        other => panic!("{id}: unknown split status {other}"),
    }
}

fn split_case(bed: &mut Bed, case: &Value, id: &str) {
    let (run_context, url) = split_input(case);
    let (intent, row) = bed.run(Some(&run_context), "README.md", false, &url);
    assert_split_outcome(&intent, &row, case.get("expected").unwrap(), id);
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
    } else if text(case, "operation") == "bitbucket-cloud-line-fragment" {
        (
            context(
                ForgeDialect::BitbucketCloud,
                "bitbucket.org",
                "acme",
                "widgets",
                "refs/heads/main",
                "refs/heads/main",
            ),
            format!("https://bitbucket.org/acme/widgets/src/main/docs/a.md#{value}"),
        )
    } else if text(case, "operation") == "bitbucket-data-center-line-fragment" {
        (
            context(
                ForgeDialect::BitbucketDataCenter,
                "bitbucket.example",
                "acme",
                "widgets",
                "refs/heads/main",
                "refs/heads/main",
            ),
            format!(
                "https://bitbucket.example/projects/ACME/repos/widgets/browse/docs/a.md#{value}"
            ),
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
        matches!(&row, Resolution::Resolved { .. })
    } else {
        matches!(
            &row,
            Resolution::Missing(Missing::HeadingAnchorNotFound { .. })
        )
    };
    assert!(
        matches_boundary,
        "{id}: outside the line grammar the fragment is a heading anchor, not a selection"
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
                    | IntentKind::SameRepositoryBitbucketCloud
                    | IntentKind::SameRepositoryBitbucketDataCenter
            ),
            "{id}: got {:?}",
            intent.kind
        ),
        "foreign" => assert!(
            matches!(
                &row,
                Resolution::External {
                    reason: ExternalReference::ForeignRepository
                }
            ),
            "{id}: expected a foreign repository, got {row:?}"
        ),
        "external" => {
            assert!(
                matches!(
                    &row,
                    Resolution::External {
                        reason: ExternalReference::Url
                    }
                ),
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
        ForgeDialect::BitbucketCloud => "bitbucket.org",
        ForgeDialect::BitbucketDataCenter => "bitbucket.example",
    };
    let run_context = context(
        dialect,
        host,
        "acme",
        "widgets",
        "refs/heads/main",
        "refs/heads/main",
    );
    let url = format!("https://{host}/{}", text(case, "suffix"));
    let (intent, row) = bed.run(Some(&run_context), "README.md", false, &url);
    match text(case, "expected") {
        "foreign" => assert!(
            matches!(
                &row,
                Resolution::External {
                    reason: ExternalReference::ForeignRepository
                }
            ),
            "{id}: expected a foreign repository, got {row:?}"
        ),
        "unsupported-version-scope" => assert!(
            matches!(&row, Resolution::UnsupportedVersion { .. }),
            "{id}: expected an unsupported version, got {row:?}"
        ),
        expected => assert_eq!(intent.target_kind.map(Into::into), Some(expected), "{id}"),
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
        intent.target_kind.map(Into::into),
        Some(text(case, "expected")),
        "{id}"
    );
}

fn boundary_case(bed: &mut Bed, case: &Value, id: &str) {
    let target = match text(case, "target_class") {
        "document" => "docs/a.md",
        "document-anchor" => "docs/file.md",
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
        let key = if case
            .get("github_line_fragment")
            .and_then(Value::as_bool)
            .unwrap()
        {
            "line_fragment"
        } else {
            "fragment"
        };
        destination.push('#');
        destination.push_str(case.get(key).and_then(Value::as_str).unwrap_or("L1"));
    }
    let (_intent, row) = bed.run(None, "README.md", false, &destination);
    let expected = case.get("expected").unwrap();
    assert_eq!(row.discriminant().as_ref(), text(expected, "kind"), "{id}");
    let expected_reason = expected.get("reason").and_then(Value::as_str);
    match &row {
        Resolution::Resolved { .. } => assert_eq!(expected_reason, None, "{id}"),
        Resolution::Missing(
            missing @ (Missing::LineFragmentOutOfRange { .. }
            | Missing::HeadingAnchorNotFound { .. }),
        ) => {
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
        | Resolution::DeclaredUntracked(_)
        | Resolution::TypeMismatch { .. }
        | Resolution::UnsupportedTarget(_)
        | Resolution::UnsupportedVersion { .. }
        | Resolution::Invalid { .. }
        | Resolution::External { .. } => panic!("{id}: unexpected boundary outcome: {row:?}"),
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
    if case.pointer("/expected/kind").and_then(Value::as_str) == Some("refused") {
        let Outcome::Rejected { codes, .. } = parse(&argv) else {
            panic!("{id}: expected a refusal");
        };
        assert_eq!(
            codes.iter().map(AsRef::as_ref).collect::<Vec<_>>(),
            vec![
                case.pointer("/expected/code")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            ],
            "{id}"
        );
        return;
    }
    let Outcome::Accepted(command) = parse(&argv) else {
        panic!("{id}: expected acceptance");
    };
    let crate::invocation::Command::Scan(invocation) = *command else {
        panic!("{id}: expected a scan command");
    };
    assert_eq!(
        invocation.forge.map(Into::<&'static str>::into),
        case.get("expected").and_then(Value::as_str),
        "{id}"
    );
}

fn dispatch(bed: &mut Bed, case: &Value) {
    let id = text(case, "id");
    match text(case, "operation") {
        "target-kind" => target_kind_case(bed, case, id),
        "github-line-fragment"
        | "gitlab-line-fragment"
        | "gitea-line-fragment"
        | "bitbucket-cloud-line-fragment"
        | "bitbucket-data-center-line-fragment" => line_fragment_case(bed, case, id),
        "github-ref-split"
        | "gitlab-ref-split"
        | "gitea-branch-split"
        | "gitea-commit-split"
        | "bitbucket-cloud-ref-split"
        | "bitbucket-data-center-ref-query" => split_case(bed, case, id),
        "github-identity" | "forge-identity" => identity_case(bed, case, id),
        "forge-form" => forge_form_case(bed, case, id),
        "forge-dialect-default" => dialect_default_case(case, id),
        "resolution-boundary" => boundary_case(bed, case, id),
        "empty-native-destination" => {
            let source = text(case, "source_document");
            let (_intent, row) = bed.run(None, source, false, "");
            let Resolution::Resolved {
                target: Target::Blob(blob),
            } = &row
            else {
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
            let actual = [
                intent.repository_path.as_ref().and_then(RepoPath::as_str),
                intent.query.as_deref(),
                intent.fragment.as_deref(),
            ];
            let expected = ["path", "query", "fragment"]
                .map(|field| expected.get(field).and_then(Value::as_str));
            assert_eq!(actual, expected, "{id}");
        }
        "native-trailing-slash" => {
            let is_image = text(case, "construct").contains("image");
            let (_intent, row) = bed.run(None, "README.md", is_image, "docs/");
            let expected = case.get("expected").unwrap();
            assert_eq!(row.discriminant().as_ref(), text(expected, "kind"), "{id}");
            let Resolution::Invalid {
                reason: invalid @ InvalidReference::Syntax,
            } = &row
            else {
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
