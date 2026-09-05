use std::fs;
use std::path::Path;

use amiss_git::Repository;
use amiss_scan::pipeline::{SetupShell, commit_pair, staged_index};
use amiss_scan::report::{Built, RequestDigests};
use amiss_scan::resolve::ForgeContext;
use amiss_wire::controls::Profile;
use amiss_wire::digest::hb;
use amiss_wire::model::{BranchRef, ForgeDialect, ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::report::{EngineProvenance, FixKind};
use tempfile::TempDir;

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn git(dir: &Path, args: &[&str]) -> String {
    amiss_fixtures::git(dir, args).unwrap()
}

fn engine() -> EngineProvenance {
    EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: hb("amiss/scanner-engine", b"test engine"),
    }
}

fn shell() -> SetupShell {
    SetupShell {
        engine: engine(),
        profile: Profile::Observe,
        repository: None,
        forge: None,
        candidate_ref: None,
        target_ref: None,
        default_branch_ref: None,
        floor: None,
        debt: None,
        waiver: None,
        time: None,
        constraint: None,
        semantic: amiss_scan::semantic::Input::None,
        requests: RequestDigests::default(),
        external_defect: None,
        errors_retained: 64,
    }
}

#[expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "test assertion helper"
)]
fn payload(built: &Built) -> serde_json::Value {
    let wire: serde_json::Value =
        crate::support::generated_report(&amiss_scan::report::wire(built).unwrap()).unwrap();
    wire["payload"].clone()
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn base_commit(root: &Path) -> String {
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    git(root, &["rev-parse", "HEAD"]).trim().to_owned()
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn oid(hex: &str) -> Oid {
    Oid::new(ObjectFormat::Sha1, hex.to_owned()).unwrap()
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn projection_policy(
    root: &Path,
    document: &str,
    name: &str,
    projection: &str,
    source: &serde_json::Value,
) {
    fs::create_dir_all(root.join(".amiss")).unwrap();
    fs::write(
        root.join(".amiss/scanner-policy.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema": "amiss/scanner-policy",
            "document_includes": [],
            "projection_assertions": [{
                "document": document,
                "name": name,
                "projection": projection,
                "sink": "previous-code",
                "source": source,
            }],
            "protected_inventory": [],
            "finding_dispositions": [],
        }))
        .unwrap(),
    )
    .unwrap();
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn stage_projection_index(root: &Path, document: &[u8], source_paths: &[&[u8]]) {
    let document_oid = amiss_fixtures::loose_object(root, "blob", document).unwrap();
    let policy_bytes = fs::read(root.join(".amiss/scanner-policy.json")).unwrap();
    let policy_oid = amiss_fixtures::loose_object(root, "blob", &policy_bytes).unwrap();
    let source_oid = amiss_fixtures::loose_object(root, "blob", b"source").unwrap();
    let mut entries = vec![
        (
            b".amiss/scanner-policy.json".as_slice(),
            policy_oid.as_str(),
        ),
        (b"docs.md".as_slice(), document_oid.as_str()),
    ];
    entries.extend(
        source_paths
            .iter()
            .map(|source_path| (*source_path, source_oid.as_str())),
    );
    amiss_fixtures::index_file(root, &entries).unwrap();
}

#[expect(
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test assertion helper"
)]
fn projection_difference<'a>(
    payload: &'a serde_json::Value,
    context: &str,
) -> &'a serde_json::Value {
    &payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "projection-drift")
        .unwrap_or_else(|| panic!("{context}: {payload}"))["candidate_fact"]["evidence"]["difference"]
}

#[test]
fn resolved_commit_identities_survive_discovery_failures() {
    use amiss_wire::report::AnalysisErrorCode;
    use amiss_wire::report::model::{BaseSnapshot, Evaluation, Snapshot};
    use amiss_wire::requests::CandidateSnapshot;

    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let original = base_commit(root);
    let valid_tree = git(root, &["rev-parse", &format!("{original}^{{tree}}")])
        .trim()
        .to_owned();
    let broken_tree =
        amiss_fixtures::tree_object(root, &[("40000", b"docs", &"9".repeat(40))]).unwrap();

    for (base_broken, candidate_broken) in [(true, false), (false, true), (true, true)] {
        let snapshots =
            [(base_broken, "base"), (candidate_broken, "candidate")].map(|(broken, label)| {
                let tree = if broken { &broken_tree } else { &valid_tree };
                let commit = amiss_fixtures::commit_object(root, tree, &[], label).unwrap();
                (oid(&commit), oid(tree))
            });
        let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
        let built = commit_pair(
            &repo,
            &engine(),
            None,
            &shell(),
            &snapshots[0].0,
            &snapshots[1].0,
        )
        .unwrap();
        crate::support::generated_report(&amiss_scan::report::wire(&built).unwrap()).unwrap();
        let payload = &built.envelope.payload;
        assert!(!payload.result.complete);
        assert!(
            payload
                .errors
                .iter()
                .any(|error| error.code == AnalysisErrorCode::GitObjectMissing),
            "{:?}",
            payload.errors
        );
        let Evaluation::Resolved(evaluation) = &payload.evaluation else {
            panic!("the resolved evaluation must survive a discovery failure");
        };
        let (BaseSnapshot::Git(base), Snapshot::Available(CandidateSnapshot::Git(candidate))) =
            (&evaluation.base, &evaluation.candidate)
        else {
            panic!("both commit identities were resolved before discovery");
        };
        for (actual, (commit, tree)) in [base, candidate].into_iter().zip(&snapshots) {
            assert_eq!(&actual.commit_oid, commit);
            assert_eq!(&actual.tree_oid, tree);
        }
    }
}

#[test]
fn exact_relocation_evidence_requires_one_removed_and_one_added_identity() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(
        root.join("README.md"),
        "[unique](unique-old.bin)\n\n\
         [base-a](base-a.bin)\n\n\
         [base-b](base-b.bin)\n\n\
         [candidate](candidate-old.bin)\n\n\
         [edited](edited-old.bin)\n\n\
         [mode](mode-old.bin)\n\n\
         [retained](retained.bin)\n",
    )
    .unwrap();
    for (path, body) in [
        ("unique-old.bin", "unique"),
        ("base-a.bin", "same-base-identity"),
        ("base-b.bin", "same-base-identity"),
        ("candidate-old.bin", "same-candidate-identity"),
        ("edited-old.bin", "before-edit"),
        ("mode-old.bin", "mode-change"),
        ("retained.bin", "retained-copy"),
    ] {
        fs::write(root.join(path), body).unwrap();
    }
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    fs::rename(root.join("unique-old.bin"), root.join("unique-new.bin")).unwrap();
    fs::remove_file(root.join("base-a.bin")).unwrap();
    fs::remove_file(root.join("base-b.bin")).unwrap();
    fs::write(root.join("base-new.bin"), "same-base-identity").unwrap();
    fs::remove_file(root.join("candidate-old.bin")).unwrap();
    fs::write(root.join("candidate-new-a.bin"), "same-candidate-identity").unwrap();
    fs::write(root.join("candidate-new-b.bin"), "same-candidate-identity").unwrap();
    fs::remove_file(root.join("edited-old.bin")).unwrap();
    fs::write(root.join("edited-new.bin"), "after-edit").unwrap();
    fs::remove_file(root.join("mode-old.bin")).unwrap();
    fs::write(root.join("mode-new.bin"), "mode-change").unwrap();
    fs::write(root.join("retained-copy.bin"), "retained-copy").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["update-index", "--chmod=+x", "mode-new.bin"]);

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let base = oid(&base);
    let staged = payload(&staged_index(&repo, &engine(), None, &shell(), &base).unwrap());
    git(root, &["commit", "-qm", "candidate"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let candidate = oid(&candidate);
    let committed =
        payload(&commit_pair(&repo, &engine(), None, &shell(), &base, &candidate).unwrap());

    for report in [&staged, &committed] {
        let resolution = |path: &str| {
            report["findings"]
                .as_array()
                .unwrap()
                .iter()
                .find(|finding| {
                    finding["kind"] == "explicit-target-missing"
                        && finding["key_input"]["scope"]["normalized_target_intent"]["path"] == path
                })
                .map(|finding| {
                    (
                        &finding["candidate_fact"]["evidence"]["resolution"],
                        &finding["fix"],
                    )
                })
                .unwrap()
        };
        let (unique, fix) = resolution("unique-old.bin");
        assert_eq!(unique["same_object_at"], "unique-new.bin", "{unique}");
        assert!(fix.is_null(), "equal bytes do not prove a repair: {fix}");
        for path in [
            "base-a.bin",
            "base-b.bin",
            "candidate-old.bin",
            "edited-old.bin",
            "mode-old.bin",
        ] {
            let (ambiguous, fix) = resolution(path);
            assert!(ambiguous["same_object_at"].is_null(), "{path}: {ambiguous}");
            assert!(fix.is_null(), "{path}: {fix}");
        }
        assert!(
            report["findings"]
                .as_array()
                .unwrap()
                .iter()
                .all(|finding| {
                    finding["key_input"]["scope"]["normalized_target_intent"]["path"]
                        != "retained.bin"
                }),
            "a copy does not make the retained target missing: {report}"
        );
    }
}

#[test]
fn a_historical_absence_never_borrows_candidate_relocation_evidence() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "history"]);
    let historical = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    fs::write(root.join("old.bin"), "relocated identity").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    fs::rename(root.join("old.bin"), root.join("new.bin")).unwrap();
    fs::write(
        root.join("reader.md"),
        format!("[old](https://github.com/acme/widgets/blob/{historical}/old.bin)\n"),
    )
    .unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let context = ForgeContext {
        host: "github.com".to_owned(),
        dialect: ForgeDialect::Github,
        object_format: ObjectFormat::Sha1,
        owner: "acme".to_owned(),
        repository: "widgets".to_owned(),
        candidate_ref: "refs/heads/main".to_owned(),
        default_ref: "refs/heads/main".to_owned(),
    };
    let mut setup = shell();
    setup.repository = RepositoryIdentity::github("acme".to_owned(), "widgets".to_owned());
    setup.forge = Some(ForgeDialect::Github);
    setup.candidate_ref = BranchRef::new("refs/heads/main".to_owned());
    setup.default_branch_ref = BranchRef::new("refs/heads/main".to_owned());
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        Some(&context),
        &setup,
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let envelope: serde_json::Value =
        crate::support::generated_report(&amiss_scan::report::wire(&built).unwrap()).unwrap();
    let report = &envelope["payload"];
    let observation = report["observations"]
        .as_array()
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("candidate"))
        .unwrap();
    assert_eq!(observation["intent"]["commit_oid"], historical);
    assert_eq!(
        observation["observation_id_input"]["extracted_intent"]["commit_oid"],
        historical
    );
    let finding = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|finding| finding["kind"] == "explicit-target-missing")
        .unwrap();
    assert_eq!(
        finding["key_input"]["scope"]["normalized_target_intent"]["commit_oid"],
        historical
    );
    assert!(finding["candidate_fact"]["evidence"]["resolution"]["same_object_at"].is_null());
    assert!(finding["fix"].is_null());
}

/// A tree path the report cannot spell is disclosed by its raw bytes, which
/// are the only name that entry has.
#[test]
fn an_unrepresentable_tree_path_is_disclosed_by_its_bytes() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    let readme = git(root, &["rev-parse", "HEAD:README.md"])
        .trim()
        .to_owned();
    let blob = amiss_fixtures::loose_object(root, "blob", b"# X\n").unwrap();
    let raw = br"back\slash.md".as_slice();
    let tree = amiss_fixtures::tree_object(
        root,
        &[
            ("100644", b"README.md".as_slice(), readme.as_str()),
            ("100644", raw, blob.as_str()),
        ],
    )
    .unwrap();
    let candidate = amiss_fixtures::commit_object(root, &tree, &[&base], "unspellable").unwrap();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let payload = payload(&built);
    let row = payload["errors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["code"] == "UNREPRESENTABLE_PATH")
        .expect("the walk reports the entry it could not name");
    assert!(row["path"].is_null(), "there is no spelling to print");
    assert_eq!(
        row["path_bytes_hex"].as_str(),
        Some(hex(raw).as_str()),
        "the bytes are the disclosure: {row}"
    );
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::new();
    for byte in bytes {
        let _infallible = std::fmt::Write::write_fmt(&mut text, format_args!("{byte:02x}"));
    }
    text
}

/// The index discloses an unnameable entry up to the raw-path ceiling and not
/// one byte past it.
#[test]
fn an_index_path_is_disclosed_up_to_the_ceiling() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    let blob = amiss_fixtures::loose_object(root, "blob", b"# X\n").unwrap();
    let mut at_ceiling = vec![b'a'; 4095];
    at_ceiling.push(b'\\');
    let mut past_ceiling = vec![b'b'; 4096];
    past_ceiling.push(b'\\');
    amiss_fixtures::index_file(
        root,
        &[
            (at_ceiling.as_slice(), blob.as_str()),
            (past_ceiling.as_slice(), blob.as_str()),
        ],
    )
    .unwrap();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = staged_index(&repo, &engine(), None, &shell(), &oid(&base)).unwrap();
    let payload = payload(&built);
    let disclosed: Vec<Option<&str>> = payload["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["code"] == "UNREPRESENTABLE_PATH")
        .map(|row| row["path_bytes_hex"].as_str())
        .collect();
    assert_eq!(disclosed.len(), 2, "both entries are refused");
    assert!(
        disclosed.contains(&Some(hex(&at_ceiling).as_str())),
        "the entry at the ceiling is disclosed whole"
    );
    assert!(
        disclosed.contains(&None),
        "the entry past the ceiling has no disclosure to make"
    );
}

/// An index the run cannot read names why the candidate is unavailable.
#[test]
fn an_unreadable_index_names_the_reason_it_left() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(root.join(".git/index"), b"not an index at all").unwrap();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = staged_index(&repo, &engine(), None, &shell(), &oid(&base)).unwrap();
    let payload = payload(&built);
    assert_eq!(payload["evaluation"]["candidate"]["kind"], "unavailable");
    assert_eq!(
        payload["evaluation"]["candidate"]["reasons"],
        serde_json::json!(["index-invalid"])
    );
}

/// The staged run reads its policy from both sides, so a raise staged with
/// the work it governs takes effect on the run that stages it.
#[test]
fn a_staged_policy_raises_the_disposition_of_the_run_that_stages_it() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(root.join("guide.md"), "see [gone](missing.md)\n").unwrap();
    fs::create_dir_all(root.join(".amiss")).unwrap();
    fs::write(
        root.join(".amiss/scanner-policy.json"),
        br#"{"schema":"amiss/scanner-policy","document_includes":[],"protected_inventory":[],"finding_dispositions":[{"finding_kind":"explicit-target-missing","disposition":"fail"}]}"#,
    )
    .unwrap();
    git(root, &["add", "."]);

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = staged_index(&repo, &engine(), None, &shell(), &oid(&base)).unwrap();
    let payload = payload(&built);
    let raised: Vec<&serde_json::Value> = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|finding| finding["kind"] == "explicit-target-missing")
        .collect();
    assert_eq!(raised.len(), 1, "the staged document has one bad link");
    assert_eq!(
        raised[0]["effective_disposition"], "fail",
        "the staged policy is the one the run answers to: {}",
        raised[0]
    );
}

/// Two identical destinations mined from one raw-HTML block are two
/// observations with distinct identities: the scan completes and resolves
/// both instead of aborting on a colliding observation id.
#[test]
fn duplicate_html_destinations_scan_cleanly() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(root.join("a.md"), "x\n").unwrap();
    fs::write(
        root.join("dup.md"),
        "<div>\n<a href=\"./a.md\">one</a>\n<a href=\"./a.md\">two</a>\n</div>\n",
    )
    .unwrap();
    git(root, &["add", "."]);

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = staged_index(&repo, &engine(), None, &shell(), &oid(&base)).unwrap();
    let payload = payload(&built);
    assert_eq!(i64::from(built.exit_code), 0, "{payload}");
    assert_eq!(payload["result"]["status"], "pass");
    assert_eq!(payload["errors"].as_array().map(Vec::len), Some(0));
    assert_eq!(payload["summary"]["references"]["extracted"], 2);
    assert_eq!(payload["summary"]["references"]["resolved"], 2);
}

/// The same pair against a missing target reaches the report as two rows:
/// each keeps its own mined address and identity, and the one aggregated
/// finding names both, where the collision once aborted the whole run.
#[test]
fn equal_broken_anchors_are_two_observations_in_one_finding() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(
        root.join("nav.md"),
        "<div>\n<a href=\"./missing.md\">one</a>\n<a href=\"./missing.md\">two</a>\n</div>\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "nav"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let payload = payload(&built);
    assert_eq!(payload["result"]["complete"], true, "{payload}");
    let anchors: Vec<&serde_json::Value> = payload["observations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| &row["candidate"])
        .filter(|side| side["source_construct"] == "html-anchor")
        .collect();
    assert_eq!(anchors.len(), 2, "both mined anchors survive: {payload}");
    assert_ne!(
        anchors[0]["observation_id"], anchors[1]["observation_id"],
        "the mined ordinal keeps the identities apart"
    );
    assert_ne!(
        anchors[0]["observation_id_input"]["structural_address"]["node_path"],
        anchors[1]["observation_id_input"]["structural_address"]["node_path"],
        "each tag carries its own address"
    );
    let missing = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap_or_else(|| panic!("the shared target is reported missing: {payload}"));
    assert_eq!(missing["aggregation"]["member_count"], 2, "{missing}");
    assert_eq!(
        missing["observation_ids"].as_array().map(Vec::len),
        Some(2),
        "the one finding names both occurrences: {missing}"
    );
}

/// A well-formed value claim whose target agrees is attested: no finding,
/// no boundary, and the summary counts it.
#[test]
fn an_attested_value_claim_passes_and_is_counted() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(
        root.join("docs.md"),
        "# Docs\n\n[amiss:v]: <amiss:value?path=README.md&line=L1> \"# R\"\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "claimed"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let payload = payload(&built);
    assert_eq!(built.exit_code, 0, "{payload}");
    assert_eq!(payload["result"]["status"], "pass");
    assert_eq!(payload["summary"]["governed_claims"], 1);
    assert_eq!(payload["summary"]["unattested_claims"], 0);
    assert_eq!(payload["errors"].as_array().map(Vec::len), Some(0));
    assert!(
        payload["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["kind"] != "claim-broken"
                && row["kind"] != "claim-target-missing"
                && row["kind"] != "unsupported-capability"),
        "an attested claim leaves no claim finding behind: {}",
        payload["findings"]
    );
}

#[test]
fn a_code_projection_attests_in_committed_and_staged_snapshots() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(root.join("source.txt"), b"one\r\ntwo\rthree\n").unwrap();
    fs::write(
        root.join("docs.md"),
        b"```text\r\none\r\ntwo\r\nthree\r\n```\r\n[amiss:sample]: <amiss:projection>\r\n",
    )
    .unwrap();
    projection_policy(
        root,
        "docs.md",
        "sample",
        "code-text-v1",
        &serde_json::json!({
            "kind": "blob-lines",
            "path": "source.txt",
            "first_line": 1,
            "last_line": 3,
        }),
    );
    git(root, &["add", "."]);

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let staged = staged_index(&repo, &engine(), None, &shell(), &oid(&base)).unwrap();
    let staged_payload = payload(&staged);
    assert_eq!(staged.exit_code, 0, "{staged_payload}");
    assert_eq!(staged_payload["result"]["status"], "pass");
    assert_eq!(staged_payload["errors"].as_array().map(Vec::len), Some(0));
    assert!(
        staged_payload["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["kind"] != "projection-drift"
                && row["kind"] != "unsupported-capability"),
        "{staged_payload}"
    );

    git(root, &["commit", "-qm", "projected"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let committed = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let committed_payload = payload(&committed);
    assert_eq!(committed.exit_code, 0, "{committed_payload}");
    assert_eq!(committed_payload["result"]["status"], "pass");
    assert_eq!(
        committed_payload["errors"].as_array().map(Vec::len),
        Some(0)
    );
}

#[test]
fn trailing_blank_lines_remain_projection_content() {
    let run = |source: &[u8], document: &[u8], last_line: u64| {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let base = base_commit(root);
        fs::write(root.join("source.txt"), source).unwrap();
        fs::write(root.join("docs.md"), document).unwrap();
        projection_policy(
            root,
            "docs.md",
            "sample",
            "code-text-v1",
            &serde_json::json!({
                "kind": "blob-lines",
                "path": "source.txt",
                "first_line": 1,
                "last_line": last_line,
            }),
        );
        git(root, &["add", "."]);
        git(root, &["commit", "-qm", "projected"]);
        let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
        let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
        payload(
            &commit_pair(
                &repo,
                &engine(),
                None,
                &shell(),
                &oid(&base),
                &oid(&candidate),
            )
            .unwrap(),
        )
    };
    let clean = b"```text\nvalue\n```\n[amiss:sample]: <amiss:projection>\n";
    let blank = b"```text\nvalue\n\n```\n[amiss:sample]: <amiss:projection>\n";
    for (source, document, last_line, drifted) in [
        (b"value\n\n".as_slice(), blank.as_slice(), 2, false),
        (b"value\n".as_slice(), blank.as_slice(), 1, true),
        (b"value\n\n".as_slice(), clean.as_slice(), 2, true),
    ] {
        let payload = run(source, document, last_line);
        let projection_drifted = payload["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["kind"] == "projection-drift");
        assert_eq!(projection_drifted, drifted, "{payload}");
    }
}

#[test]
fn a_changed_projection_reports_the_exact_relation_and_visible_sink() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(root.join("source.txt"), "current\n").unwrap();
    fs::write(
        root.join("docs.md"),
        "```text\nstale\n```\n[amiss:sample]: <amiss:projection>\n",
    )
    .unwrap();
    projection_policy(
        root,
        "docs.md",
        "sample",
        "code-text-v1",
        &serde_json::json!({
            "kind": "blob-lines",
            "path": "source.txt",
            "first_line": 1,
            "last_line": 1,
        }),
    );
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "drifted"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let mut setup = shell();
    setup.profile = Profile::Enforce;
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &setup,
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let payload = payload(&built);
    assert_eq!(built.exit_code, 1, "{payload}");
    let row = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "projection-drift")
        .unwrap_or_else(|| panic!("projection drift is reported: {payload}"));
    assert_eq!(
        row["key_input"]["scope"]["rule_id"],
        "claim/projection/sample"
    );
    assert_eq!(row["location"]["path"], "docs.md");
    assert_eq!(row["location"]["span"]["start_line"], 1);
    let evidence = &row["candidate_fact"]["evidence"];
    assert_eq!(evidence["kind"], "projection");
    assert_eq!(evidence["projection"], "code-text-v1");
    assert_eq!(evidence["sink"], "previous-code");
    assert_eq!(evidence["source"]["kind"], "blob-lines");
    assert_eq!(evidence["source"]["path"], "source.txt");
    assert_eq!(evidence["observed"], "content-differs");
    assert_eq!(evidence["expected_bytes"], 7);
    assert_eq!(evidence["observed_bytes"], 5);
    assert!(evidence["expected_digest"].is_string());
    assert!(evidence["observed_digest"].is_string());
    assert!(row["fix"].is_null());
}

#[test]
fn declared_projection_sink_defects_are_findings_not_silent_boundaries() {
    let run = |document: &str, source: Option<&str>, first_line: u64| {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let base = base_commit(root);
        if let Some(source) = source {
            fs::write(root.join("source.txt"), source).unwrap();
        }
        fs::write(root.join("docs.md"), document).unwrap();
        projection_policy(
            root,
            "docs.md",
            "sample",
            "code-text-v1",
            &serde_json::json!({
                "kind": "blob-lines",
                "path": "source.txt",
                "first_line": first_line,
                "last_line": first_line,
            }),
        );
        git(root, &["add", "."]);
        git(root, &["commit", "-qm", "projected"]);
        let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
        let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
        payload(
            &commit_pair(
                &repo,
                &engine(),
                None,
                &shell(),
                &oid(&base),
                &oid(&candidate),
            )
            .unwrap(),
        )
    };
    let cases = [
        ("# no marker\n", Some("value\n"), 1, "sink-absent"),
        (
            "```\nvalue\n```\n[amiss:sample]: <amiss:projection>\n[amiss:sample]: <amiss:projection>\n",
            Some("value\n"),
            1,
            "sink-ambiguous",
        ),
        (
            "```\nvalue\n```\nprose\n\n[amiss:sample]: <amiss:projection>\n",
            Some("value\n"),
            1,
            "sink-not-adjacent",
        ),
        (
            "```\nvalue\n```\n[amiss:sample]: <amiss:projection>\n",
            None,
            1,
            "source-absent",
        ),
        (
            "```\nvalue\n```\n[amiss:sample]: <amiss:projection>\n",
            Some(
                "version https://git-lfs.github.com/spec/v1\noid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsize 42\n",
            ),
            1,
            "source-lfs-pointer",
        ),
        (
            "```\nvalue\n```\n[amiss:sample]: <amiss:projection>\n",
            Some("value\n"),
            2,
            "source-lines-out-of-range",
        ),
    ];
    for (document, source, first_line, reason) in cases {
        let payload = run(document, source, first_line);
        assert_eq!(payload["result"]["complete"], true, "{reason}: {payload}");
        assert_eq!(payload["errors"].as_array().map(Vec::len), Some(0));
        let rows: Vec<_> = payload["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["kind"] == "projection-drift")
            .collect();
        assert_eq!(rows.len(), 1, "{reason}: {payload}");
        assert_eq!(rows[0]["candidate_fact"]["evidence"]["observed"], reason);
        assert!(
            payload["findings"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row["kind"] != "unsupported-capability"),
            "the declared assertion answers all matching carriers: {payload}"
        );
    }
}

#[test]
fn a_named_region_attests_across_endings_and_ignores_outside_edits() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(
        root.join("source.txt"),
        b"const a = \"// amiss:start\";\r\n// amiss:start\r\none\r\ntwo\r// amiss:end\r\nconst b = \"// amiss:end\";\n",
    )
    .unwrap();
    fs::write(
        root.join("docs.md"),
        "```text\none\ntwo\n```\n[amiss:sample]: <amiss:projection>\n",
    )
    .unwrap();
    projection_policy(
        root,
        "docs.md",
        "sample",
        "code-text-v1",
        &serde_json::json!({
            "kind": "named-region",
            "path": "source.txt",
            "start_marker": "// amiss:start",
            "end_marker": "// amiss:end",
        }),
    );
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "projected"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let committed = payload(
        &commit_pair(
            &repo,
            &engine(),
            None,
            &shell(),
            &oid(&base),
            &oid(&candidate),
        )
        .unwrap(),
    );
    assert!(
        committed["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["kind"] != "projection-drift"),
        "{committed}"
    );

    fs::write(
        root.join("source.txt"),
        b"const changed = \"// amiss:start\";\r\n// amiss:start\r\none\r\ntwo\r// amiss:end\r\nconst b = \"// amiss:end\";\n",
    )
    .unwrap();
    git(root, &["add", "source.txt"]);
    let staged =
        payload(&staged_index(&repo, &engine(), None, &shell(), &oid(&candidate)).unwrap());
    assert_eq!(staged["result"]["status"], "pass", "{staged}");
    assert!(
        staged["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["kind"] != "projection-drift"),
        "{staged}"
    );
}

#[test]
fn a_named_region_mismatch_carries_the_exact_selector() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(
        root.join("source.txt"),
        "// amiss:start\ncurrent\n// amiss:end\n",
    )
    .unwrap();
    fs::write(
        root.join("docs.md"),
        "```text\nstale\n```\n[amiss:sample]: <amiss:projection>\n",
    )
    .unwrap();
    projection_policy(
        root,
        "docs.md",
        "sample",
        "code-text-v1",
        &serde_json::json!({
            "kind": "named-region",
            "path": "source.txt",
            "start_marker": "// amiss:start",
            "end_marker": "// amiss:end",
        }),
    );
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "drifted"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let payload = payload(&built);
    let row = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "projection-drift")
        .unwrap_or_else(|| panic!("named-region drift is reported: {payload}"));
    let source = &row["candidate_fact"]["evidence"]["source"];
    assert_eq!(source["kind"], "named-region");
    assert_eq!(source["path"], "source.txt");
    assert_eq!(source["start_marker"], "// amiss:start");
    assert_eq!(source["end_marker"], "// amiss:end");
}

#[test]
fn named_region_marker_defects_are_typed_projection_drift() {
    let cases: &[(&[u8], &str)] = &[
        (b"// amiss:end\n", "source-start-marker-absent"),
        (
            b"const a = \"// amiss:start\";\nvalue\nconst b = \"// amiss:end\";\n",
            "source-start-marker-absent",
        ),
        (
            b"// amiss:start\nvalue\nconst b = \"// amiss:end\";\n",
            "source-end-marker-absent",
        ),
        (
            b"// amiss:start\nvalue\n// amiss:start\n// amiss:end\n",
            "source-start-marker-ambiguous",
        ),
        (b"// amiss:start\n", "source-end-marker-absent"),
        (
            b"// amiss:start\nvalue\n// amiss:end\n// amiss:end\n",
            "source-end-marker-ambiguous",
        ),
        (
            b"// amiss:end\nvalue\n// amiss:start\n",
            "source-region-order-invalid",
        ),
        (
            b"// amiss:start // amiss:end\n",
            "source-start-marker-absent",
        ),
        (
            b"// amiss:start\n\xff\n// amiss:end\n",
            "source-region-not-utf8",
        ),
    ];
    for (source, reason) in cases {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let base = base_commit(root);
        fs::write(root.join("source.txt"), source).unwrap();
        fs::write(
            root.join("docs.md"),
            "```text\nvalue\n```\n[amiss:sample]: <amiss:projection>\n",
        )
        .unwrap();
        projection_policy(
            root,
            "docs.md",
            "sample",
            "code-text-v1",
            &serde_json::json!({
                "kind": "named-region",
                "path": "source.txt",
                "start_marker": "// amiss:start",
                "end_marker": "// amiss:end",
            }),
        );
        git(root, &["add", "."]);
        git(root, &["commit", "-qm", "defect"]);
        let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
        let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
        let payload = payload(
            &commit_pair(
                &repo,
                &engine(),
                None,
                &shell(),
                &oid(&base),
                &oid(&candidate),
            )
            .unwrap(),
        );
        assert_eq!(payload["result"]["complete"], true, "{reason}: {payload}");
        let rows: Vec<_> = payload["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["kind"] == "projection-drift")
            .collect();
        assert_eq!(rows.len(), 1, "{reason}: {payload}");
        assert_eq!(rows[0]["candidate_fact"]["evidence"]["observed"], *reason);
    }
}

#[test]
fn a_tree_inventory_is_root_relative_and_reuses_the_staged_snapshot() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::create_dir_all(root.join("examples/deep/more")).unwrap();
    for (path, body) in [
        ("examples/a.txt", "a"),
        ("examples/b.md", "b"),
        ("examples/deep/c.txt", "c"),
        ("examples/deep/more/d.txt", "d"),
    ] {
        fs::write(root.join(path), body).unwrap();
    }
    fs::write(
        root.join("docs.md"),
        "```text\na.txt\ndeep/c.txt\n```\n[amiss:inventory]: <amiss:projection>\n",
    )
    .unwrap();
    projection_policy(
        root,
        "docs.md",
        "inventory",
        "sorted-rows-v1",
        &serde_json::json!({
            "kind": "tree-paths",
            "root": "examples",
            "suffix": ".txt",
            "maximum_depth": 2,
        }),
    );
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "inventory"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let committed = payload(
        &commit_pair(
            &repo,
            &engine(),
            None,
            &shell(),
            &oid(&base),
            &oid(&candidate),
        )
        .unwrap(),
    );
    assert!(
        committed["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["kind"] != "projection-drift"),
        "{committed}"
    );

    for index in 0..33 {
        fs::write(root.join(format!("examples/z{index:02}.txt")), "z").unwrap();
    }
    git(root, &["add", "examples"]);
    let built = staged_index(&repo, &engine(), None, &shell(), &oid(&candidate)).unwrap();
    let staged = payload(&built);
    let evidence = &staged["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "projection-drift")
        .unwrap_or_else(|| panic!("the staged inventory drift is reported: {staged}"))["candidate_fact"]
        ["evidence"];
    assert_eq!(evidence["projection"], "sorted-rows-v1");
    assert_eq!(evidence["observed"], "content-differs");
    assert_eq!(evidence["source"]["kind"], "tree-paths");
    assert_eq!(evidence["source"]["root"], "examples");
    assert_eq!(evidence["source"]["suffix"], ".txt");
    assert_eq!(evidence["source"]["maximum_depth"], 2);
    assert!(evidence["expected_digest"].is_string(), "{staged}");
    let difference = &evidence["difference"];
    assert_eq!(difference["expected_records"], 35);
    assert_eq!(difference["observed_records"], 2);
    assert_eq!(difference["missing_records"], 33);
    assert_eq!(difference["extra_records"], 0);
    assert_eq!(difference["ordering_only"], false);
    assert_eq!(
        difference["missing_preview"].as_array().map(Vec::len),
        Some(32)
    );
    assert_eq!(difference["missing_preview"][0], "z00.txt");
    assert_eq!(difference["missing_preview"][31], "z31.txt");
    assert_eq!(difference["missing_omitted"], 1);
}

#[test]
fn a_tree_inventory_distinguishes_a_pure_ordering_defect() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::create_dir_all(root.join("inventory")).unwrap();
    fs::write(root.join("inventory/a.txt"), "a").unwrap();
    fs::write(root.join("inventory/b.txt"), "b").unwrap();
    fs::write(
        root.join("docs.md"),
        "```text\nb.txt\na.txt\n```\n[amiss:inventory]: <amiss:projection>\n",
    )
    .unwrap();
    projection_policy(
        root,
        "docs.md",
        "inventory",
        "sorted-rows-v1",
        &serde_json::json!({
            "kind": "tree-paths",
            "root": "inventory",
            "maximum_depth": 1,
        }),
    );
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "reordered inventory"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let payload = payload(
        &commit_pair(
            &repo,
            &engine(),
            None,
            &shell(),
            &oid(&base),
            &oid(&candidate),
        )
        .unwrap(),
    );
    let difference = &payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "projection-drift")
        .unwrap_or_else(|| panic!("the order-only drift is reported: {payload}"))["candidate_fact"]
        ["evidence"]["difference"];
    assert_eq!(difference["ordering_only"], true, "{payload}");
    assert_eq!(difference["missing_records"], 0, "{payload}");
    assert_eq!(difference["extra_records"], 0, "{payload}");
}

#[test]
fn a_tree_inventory_counts_duplicate_visible_rows_as_extras() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::create_dir_all(root.join("inventory/deep")).unwrap();
    for path in ["inventory/a.txt", "inventory/b.txt", "inventory/deep/c.txt"] {
        fs::write(root.join(path), path).unwrap();
    }
    fs::write(
        root.join("docs.md"),
        "```text\na.txt\nextra.txt\nextra.txt\n```\n[amiss:inventory]: <amiss:projection>\n",
    )
    .unwrap();
    projection_policy(
        root,
        "docs.md",
        "inventory",
        "sorted-rows-v1",
        &serde_json::json!({
            "kind": "tree-paths",
            "root": "inventory",
            "suffix": ".txt",
            "maximum_depth": 2,
        }),
    );
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "drifted inventory"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let payload = payload(
        &commit_pair(
            &repo,
            &engine(),
            None,
            &shell(),
            &oid(&base),
            &oid(&candidate),
        )
        .unwrap(),
    );
    let difference = &payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "projection-drift")
        .unwrap_or_else(|| panic!("the inventory drift is reported: {payload}"))["candidate_fact"]
        ["evidence"]["difference"];
    assert_eq!(difference["expected_records"], 3);
    assert_eq!(difference["observed_records"], 3);
    assert_eq!(difference["missing_records"], 2);
    assert_eq!(difference["extra_records"], 2);
    assert_eq!(
        difference["missing_preview"],
        serde_json::json!(["b.txt", "deep/c.txt"])
    );
    assert_eq!(
        difference["extra_preview"],
        serde_json::json!(["extra.txt", "extra.txt"])
    );
}

#[test]
fn a_tree_inventory_count_uses_one_canonical_decimal() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::create_dir_all(root.join("inventory")).unwrap();
    fs::write(root.join("inventory/a.txt"), "a").unwrap();
    fs::write(root.join("inventory/b.txt"), "b").unwrap();
    fs::write(
        root.join("docs.md"),
        "```text\n2\n```\n[amiss:inventory]: <amiss:projection>\n",
    )
    .unwrap();
    projection_policy(
        root,
        "docs.md",
        "inventory",
        "decimal-count-v1",
        &serde_json::json!({
            "kind": "tree-paths",
            "root": "inventory",
            "suffix": ".txt",
            "maximum_depth": 1,
        }),
    );
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "counted inventory"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let committed = payload(
        &commit_pair(
            &repo,
            &engine(),
            None,
            &shell(),
            &oid(&base),
            &oid(&candidate),
        )
        .unwrap(),
    );
    assert!(
        committed["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["kind"] != "projection-drift"),
        "{committed}"
    );

    for visible in ["02", "9007199254740992"] {
        fs::write(
            root.join("docs.md"),
            format!("```text\n{visible}\n```\n[amiss:inventory]: <amiss:projection>\n"),
        )
        .unwrap();
        git(root, &["add", "docs.md"]);
        let noncanonical =
            payload(&staged_index(&repo, &engine(), None, &shell(), &oid(&candidate)).unwrap());
        let difference = projection_difference(&noncanonical, "the noncanonical count is reported");
        assert_eq!(difference["kind"], "count", "{noncanonical}");
        assert_eq!(difference["expected_count"], 2, "{noncanonical}");
        assert!(difference["observed_count"].is_null(), "{noncanonical}");
    }

    fs::write(
        root.join("docs.md"),
        "```text\n2\n```\n[amiss:inventory]: <amiss:projection>\n",
    )
    .unwrap();
    fs::write(root.join("inventory/c.txt"), "c").unwrap();
    git(root, &["add", "docs.md", "inventory/c.txt"]);
    let stale = payload(&staged_index(&repo, &engine(), None, &shell(), &oid(&candidate)).unwrap());
    let difference = projection_difference(&stale, "the stale count is reported");
    assert_eq!(difference["expected_count"], 3, "{stale}");
    assert_eq!(difference["observed_count"], 2, "{stale}");

    let count_document = b"```text\n1\n```\n[amiss:inventory]: <amiss:projection>\n";
    stage_projection_index(
        root,
        count_document,
        &[b"inventory/non-utf8-\xff.txt".as_slice()],
    );
    let unrenderable =
        payload(&staged_index(&repo, &engine(), None, &shell(), &oid(&candidate)).unwrap());
    assert!(
        unrenderable["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["kind"] != "projection-drift"),
        "the exact count includes an unrenderable path: {unrenderable}"
    );
}

#[test]
fn a_tree_inventory_never_turns_unrepresentable_paths_into_absence() {
    for (source_path, reason) in [
        (
            b"inventory/non-utf8-\xff.txt".as_slice(),
            "source-tree-path-not-utf8",
        ),
        (
            b"inventory/line\nbreak.txt".as_slice(),
            "source-tree-path-not-a-row",
        ),
    ] {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let base = base_commit(root);
        let document = b"```text\nstale\n```\n[amiss:inventory]: <amiss:projection>\n";
        fs::write(root.join("docs.md"), document).unwrap();
        projection_policy(
            root,
            "docs.md",
            "inventory",
            "sorted-rows-v1",
            &serde_json::json!({
                "kind": "tree-paths",
                "root": "inventory",
                "suffix": ".txt",
                "maximum_depth": 2,
            }),
        );
        stage_projection_index(root, document, &[source_path]);
        let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
        let row_payload =
            payload(&staged_index(&repo, &engine(), None, &shell(), &oid(&base)).unwrap());
        let evidence = &row_payload["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["kind"] == "projection-drift")
            .unwrap_or_else(|| panic!("the source refusal is reported: {row_payload}"))["candidate_fact"]
            ["evidence"];
        assert_eq!(evidence["observed"], reason, "{row_payload}");
        assert!(evidence["expected_digest"].is_null(), "{row_payload}");
        assert!(evidence.get("difference").is_none(), "{row_payload}");
    }
}

#[test]
fn a_tree_inventory_requires_an_existing_tree_root() {
    for (root_file, reason) in [
        (None, "source-tree-root-absent"),
        (Some("ordinary file"), "source-tree-root-not-a-tree"),
    ] {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        let base = base_commit(root);
        if let Some(body) = root_file {
            fs::write(root.join("inventory"), body).unwrap();
        }
        fs::write(
            root.join("docs.md"),
            "```text\nstale\n```\n[amiss:inventory]: <amiss:projection>\n",
        )
        .unwrap();
        projection_policy(
            root,
            "docs.md",
            "inventory",
            "sorted-rows-v1",
            &serde_json::json!({
                "kind": "tree-paths",
                "root": "inventory",
                "maximum_depth": 2,
            }),
        );
        git(root, &["add", "."]);
        git(root, &["commit", "-qm", "invalid inventory root"]);
        let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
        let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
        let payload = payload(
            &commit_pair(
                &repo,
                &engine(),
                None,
                &shell(),
                &oid(&base),
                &oid(&candidate),
            )
            .unwrap(),
        );
        let evidence = &payload["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["kind"] == "projection-drift")
            .unwrap_or_else(|| panic!("the invalid tree root is reported: {payload}"))["candidate_fact"]
            ["evidence"];
        assert_eq!(evidence["observed"], reason, "{payload}");
        assert!(evidence["expected_digest"].is_null(), "{payload}");
        assert!(evidence.get("difference").is_none(), "{payload}");
    }
}

#[test]
fn an_unclaimed_projection_marker_stays_inside_the_governed_boundary() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(
        root.join("docs.md"),
        "```\nvalue\n```\n[amiss:sample]: <amiss:projection>\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "unclaimed"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let payload = payload(&built);
    assert_eq!(built.exit_code, 2, "{payload}");
    assert!(
        payload["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["kind"] == "unsupported-capability"),
        "{payload}"
    );
}

#[test]
fn removing_a_projection_assertion_is_policy_weakening() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    fs::write(root.join("source.txt"), "value\n").unwrap();
    fs::write(
        root.join("docs.md"),
        "```\nvalue\n```\n[amiss:sample]: <amiss:projection>\n",
    )
    .unwrap();
    projection_policy(
        root,
        "docs.md",
        "sample",
        "code-text-v1",
        &serde_json::json!({
            "kind": "blob-lines",
            "path": "source.txt",
            "first_line": 1,
            "last_line": 1,
        }),
    );
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "projected"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    fs::write(root.join("docs.md"), "# projection retired\n").unwrap();
    fs::write(
        root.join(".amiss/scanner-policy.json"),
        r#"{"schema":"amiss/scanner-policy","document_includes":[],"projection_assertions":[],"protected_inventory":[],"finding_dispositions":[]}"#,
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "retired"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let payload = payload(&built);
    assert_eq!(
        built.exit_code, 1,
        "policy weakening always blocks: {payload}"
    );
    let weakened = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "policy-weakened")
        .unwrap_or_else(|| panic!("assertion removal is visible: {payload}"));
    assert_eq!(
        weakened["key_input"]["scope"]["rule_id"],
        "policy/projection-assertion-removed/sample"
    );
    assert_eq!(weakened["key_input"]["scope"]["control_path"], "docs.md");
}

/// Two same-subject defects fold into one item annotated at the least
/// location, whatever order the fold visits the findings.
#[test]
fn a_grouped_item_annotates_its_least_location() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    fs::write(root.join("target.md"), "# One\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(
        root.join("README.md"),
        "[b](target.md#also-gone)\n\n[a](target.md#nope)\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "links"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let payload = payload(&built);
    let items = payload["feedback"]["items"].as_array().unwrap();
    let item = items
        .iter()
        .find(|item| item["action"] == "fix" && item["target"] == "target.md")
        .unwrap_or_else(|| panic!("one grouped fix item: {items:?}"));
    assert_eq!(item["location_count"], 2, "{item}");
    assert_eq!(item["annotation"]["path"], "README.md");
    assert_eq!(
        item["annotation"]["span"]["start_line"], 1,
        "the earliest defect owns the annotation: {item}"
    );
}

/// The fix refuses to guess: an observed line the title grammar cannot
/// spell, a missing target, and a grouped name all emit null.
#[test]
fn a_fix_is_emitted_only_when_provable() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "say \"hi\"\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(
        root.join("docs.md"),
        "# Docs\n\n[amiss:v]: <amiss:value?path=README.md&line=L1> \"other\"\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "claimed"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let payload = payload(&built);
    assert_eq!(payload["findings"][0]["kind"], "claim-broken", "{payload}");
    assert!(
        payload["findings"][0]["fix"].is_null(),
        "a quoted observed line cannot be respelled: {payload}"
    );

    let missing = "[amiss:v]: <amiss:value?path=gone.md&line=L1> \"words\"";
    let (_code, payload) = claimed_run(missing, Profile::Observe);
    assert!(
        payload["findings"][0]["fix"].is_null(),
        "a missing target has no derivable content: {payload}"
    );

    let duplicated = "[amiss:v]: <amiss:value?path=README.md&line=L1> \"one\"\n[amiss:v]: <amiss:value?path=README.md&line=L1> \"two\"";
    let (_code, payload) = claimed_run(duplicated, Profile::Observe);
    assert_eq!(payload["findings"][0]["aggregation"]["member_count"], 2);
    assert!(
        payload["findings"][0]["fix"].is_null(),
        "grouped members share one finding but not one edit: {payload}"
    );
}

/// A lone case-drifted anchor carries its fix: the fragment bytes and the
/// published spelling. Aggregated references, bare misses, and spellings the
/// adapter refused to locate all stay bare.
#[test]
fn a_case_drifted_anchor_carries_its_fix() {
    let run = |links: &str| -> serde_json::Value {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        git(root, &["init", "-q"]);
        fs::write(root.join("README.md"), "# R\n").unwrap();
        fs::write(root.join("sections.md"), "# Setup\n").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-qm", "base"]);
        let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
        fs::write(root.join("guide.md"), links).unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-qm", "linked"]);
        let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
        let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
        let built = commit_pair(
            &repo,
            &engine(),
            None,
            &shell(),
            &oid(&base),
            &oid(&candidate),
        )
        .unwrap();
        payload(&built)
    };

    let drifted = run("[s](sections.md#Setup)\n");
    let row = drifted["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    let fix = &row["fix"];
    assert_eq!(fix["path"], "guide.md", "{row}");
    assert_eq!(fix["replacement"], "setup");
    let source = "[s](sections.md#Setup)\n";
    let start = usize::try_from(fix["span"]["start_byte"].as_u64().unwrap()).unwrap();
    let end = usize::try_from(fix["span"]["end_byte"].as_u64().unwrap()).unwrap();
    assert_eq!(&source.as_bytes()[start..end], b"Setup");
    assert_eq!(fix["description"], FixKind::AnchorRespelling.meaning());

    let same_block = run("[a](sections.md#Setup) and [b](sections.md#Setup)\n");
    let row = same_block["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert_eq!(row["aggregation"]["member_count"], 2, "{same_block}");
    assert!(row["fix"].is_null(), "two members share no one edit: {row}");

    let two_blocks = run("[a](sections.md#Setup)\n\n[b](sections.md#Setup)\n");
    let fixes: Vec<(u64, u64)> = two_blocks["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "explicit-target-missing")
        .map(|row| {
            (
                row["fix"]["span"]["start_byte"].as_u64().unwrap(),
                row["fix"]["span"]["end_byte"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(fixes.len(), 2, "each block's reference repairs itself");
    assert_ne!(fixes[0], fixes[1], "each fix names its own bytes");

    let bare = run("[s](sections.md#nothing-close)\n");
    let row = bare["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert!(row["fix"].is_null(), "no neighbor, no edit: {row}");

    let encoded = run("[s](sections.md#S%65tup)\n");
    let row = encoded["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert!(
        row["fix"].is_null(),
        "a percent spelling names no bytes: {row}"
    );
}

/// Separator drift is a fix too: a fragment spelled with the underscore
/// another renderer would have used names the published identity.
#[test]
fn a_separator_drifted_anchor_carries_its_fix() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    fs::write(root.join("sections.md"), "# Setup Config\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base_commit = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(root.join("guide.md"), "[s](sections.md#setup_config)\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "linked"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base_commit),
        &oid(&candidate),
    )
    .unwrap();
    let payload = payload(&built);
    let row = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert_eq!(
        row["fix"]["replacement"], "setup-config",
        "the separator the renderers disagree on folds away: {row}"
    );
}

/// A lone case-drifted path carries its fix: the written path bytes and the
/// tracked spelling's tail. Dot segments break the tail law and stay bare,
/// and a fragment rides untouched beside the replaced path part.
#[test]
fn a_case_drifted_path_carries_its_fix() {
    let run = |link: &str| -> serde_json::Value {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        git(root, &["init", "-q"]);
        fs::write(root.join("README.md"), "# R\n").unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("docs/sections.md"), "# Setup\n").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-qm", "base"]);
        let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
        fs::write(root.join("docs/guide.md"), link).unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-qm", "linked"]);
        let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
        let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
        let built = commit_pair(
            &repo,
            &engine(),
            None,
            &shell(),
            &oid(&base),
            &oid(&candidate),
        )
        .unwrap();
        payload(&built)
    };
    let missing_row = |payload: &serde_json::Value| -> serde_json::Value {
        payload["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["kind"] == "explicit-target-missing")
            .unwrap()
            .clone()
    };

    let drifted = "[s](Sections.md)\n";
    let row = missing_row(&run(drifted));
    let fix = &row["fix"];
    assert_eq!(fix["path"], "docs/guide.md", "{row}");
    assert_eq!(fix["replacement"], "sections.md");
    let start = usize::try_from(fix["span"]["start_byte"].as_u64().unwrap()).unwrap();
    let end = usize::try_from(fix["span"]["end_byte"].as_u64().unwrap()).unwrap();
    assert_eq!(&drifted.as_bytes()[start..end], b"Sections.md");
    assert_eq!(fix["description"], FixKind::PathRespelling.meaning());

    let with_fragment = "[s](Sections.md#setup)\n";
    let row = missing_row(&run(with_fragment));
    let fix = &row["fix"];
    assert_eq!(fix["replacement"], "sections.md");
    let end = usize::try_from(fix["span"]["end_byte"].as_u64().unwrap()).unwrap();
    assert_eq!(
        with_fragment.as_bytes()[end],
        b'#',
        "the fragment rides untouched beside the replaced path part"
    );

    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    fs::write(root.join("sections.md"), "# Setup\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(root.join("guide.md"), "[s](Sections.md)\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "linked"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let at_root = missing_row(&payload(&built));
    assert_eq!(
        at_root["fix"]["replacement"], "sections.md",
        "a root-level reference writes the whole intent, and still repairs: {at_root}"
    );

    let row = missing_row(&run("[s](../Docs/sections.md)\n"));
    assert!(
        row["fix"].is_null(),
        "a dot segment breaks the tail law and stays bare: {row}"
    );
}

/// The rst lane carries the anchor fix too, end to end against a real
/// repository.
#[test]
fn an_rst_reference_carries_the_anchor_fix_too() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    fs::write(root.join("sections.md"), "# Setup\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base_commit = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let rst = "See `s <sections.md#Setup>`_ here.\n";
    fs::write(root.join("guide.rst"), rst).unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "linked"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base_commit),
        &oid(&candidate),
    )
    .unwrap();
    let rst_payload = payload(&built);
    let row = rst_payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "explicit-target-missing")
        .unwrap();
    assert_eq!(
        row["fix"]["replacement"], "setup",
        "the rst lane carries the anchor fix too: {row}"
    );
    let start = usize::try_from(row["fix"]["span"]["start_byte"].as_u64().unwrap()).unwrap();
    let end = usize::try_from(row["fix"]["span"]["end_byte"].as_u64().unwrap()).unwrap();
    assert_eq!(&rst.as_bytes()[start..end], b"Setup");
}

/// A claim only the base holds is invisible: evaluation is candidate-side,
/// so a broken claim the candidate deletes leaves no count and no finding.
#[test]
fn a_base_side_claim_is_not_evaluated() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::write(root.join("README.md"), "# R\n").unwrap();
    fs::write(
        root.join("docs.md"),
        "# Docs\n\n[amiss:v]: <amiss:value?path=README.md&line=L1> \"# Wrong\"\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(root.join("docs.md"), "# Docs\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "stripped"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let payload = payload(&built);
    assert_eq!(built.exit_code, 0, "{payload}");
    assert_eq!(payload["summary"]["governed_claims"], 0);
    assert_eq!(payload["summary"]["unattested_claims"], 0);
    assert!(
        payload["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["kind"] != "claim-broken"
                && row["kind"] != "claim-target-missing"
                && row["kind"] != "unsupported-capability"),
        "the base-side claim leaks nothing: {}",
        payload["findings"]
    );
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn claimed_run(claim_line: &str, profile: Profile) -> (i64, serde_json::Value) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(root.join("docs.md"), format!("# Docs\n\n{claim_line}\n")).unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "claimed"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let mut setup = shell();
    setup.profile = profile;
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &setup,
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let code = i64::from(built.exit_code);
    (code, payload(&built))
}

/// A broken claim warns under observe and fails under enforce, carrying the
/// claim evidence family with both digests.
#[test]
fn a_broken_claim_warns_then_fails_by_profile() {
    let claim = "[amiss:v]: <amiss:value?path=README.md&line=L1> \"# Wrong\"";
    let (code, payload) = claimed_run(claim, Profile::Observe);
    assert_eq!(code, 0, "observe never blocks: {payload}");
    let fix = &payload["findings"][0]["fix"];
    assert_eq!(fix["path"], "docs.md", "{payload}");
    let rewritten = "[amiss:v]: <amiss:value?path=README.md&line=L1> \"# R\"";
    assert_eq!(fix["replacement"], rewritten);
    assert_eq!(
        fix["span"]["start_byte"], 8,
        "the definition follows the heading"
    );
    assert_eq!(
        fix["span"]["end_byte"].as_u64().map(|end| end - 8),
        Some(u64::try_from(claim.len()).unwrap()),
        "the span covers the whole definition: {fix}"
    );
    assert_eq!(fix["description"], FixKind::ClaimValueRewrite.meaning());
    let row = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "claim-broken")
        .expect("the broken claim is found");
    assert_eq!(row["effective_disposition"], "warn");
    assert_eq!(row["attribution"], "not-applicable");
    assert_eq!(payload["summary"]["governed_claims"], 1);
    assert_eq!(payload["summary"]["unattested_claims"], 1);
    let evidence = &row["candidate_fact"]["evidence"];
    assert_eq!(evidence["kind"], "claim");
    assert_eq!(evidence["claim_kind"], "value");
    assert_eq!(evidence["name"], "v");
    assert_eq!(evidence["target_path"], "README.md");
    assert_eq!(evidence["line"], 1);
    assert_eq!(evidence["observed"], "line-differs");
    assert!(evidence["observed_digest"].is_string());
    assert_eq!(
        row["key_input"]["scope"]["rule_id"], "claim/value/v",
        "the claim name heads the rule id"
    );

    let (code, payload) = claimed_run(claim, Profile::Enforce);
    assert_eq!(code, 1, "enforce blocks on a broken claim: {payload}");
}

/// A claim over a target nothing can answer is its own kind, and the reason
/// is named.
#[test]
fn a_claim_target_nothing_answers_is_missing_by_name() {
    let claim = "[amiss:v]: <amiss:value?path=absent.txt&line=L1> \"x\"";
    let (code, payload) = claimed_run(claim, Profile::Observe);
    assert_eq!(code, 0, "{payload}");
    let row = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "claim-target-missing")
        .expect("the missing target is found");
    assert_eq!(
        row["candidate_fact"]["evidence"]["observed"],
        "target-absent"
    );
    assert!(row["candidate_fact"]["evidence"]["observed_digest"].is_null());
    assert_eq!(payload["summary"]["unattested_claims"], 1);
}

/// A document holding one lawful claim and one unknown capability still ends
/// at the boundary, with the claim finding standing beside the refusal.
#[test]
fn an_unknown_capability_beside_a_claim_keeps_the_boundary() {
    let claim = "[amiss:v]: <amiss:value?path=README.md&line=L1> \"# Wrong\"\n[amiss:future]: <amiss:region>";
    let (code, payload) = claimed_run(claim, Profile::Observe);
    assert_eq!(code, 2, "the unknown kind is still a boundary: {payload}");
    assert_eq!(payload["result"]["status"], "incomplete");
    let kinds: Vec<&str> = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["kind"].as_str())
        .collect();
    assert!(kinds.contains(&"unsupported-capability"), "{kinds:?}");
    assert!(kinds.contains(&"claim-broken"), "{kinds:?}");
    let capability = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["kind"] == "unsupported-capability")
        .expect("the unknown form");
    assert_eq!(
        capability["aggregation"]["member_count"], 1,
        "only the unanswered node seeds the boundary"
    );
}

/// Two claims under one name aggregate into one finding with both sources.
#[test]
fn duplicate_claim_names_aggregate() {
    let claim = "[amiss:v]: <amiss:value?path=README.md&line=L1> \"# Wrong\"\n[amiss:v]: <amiss:value?path=README.md&line=L1> \"# Wronger\"";
    let (_code, payload) = claimed_run(claim, Profile::Observe);
    let rows: Vec<&serde_json::Value> = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "claim-broken")
        .collect();
    assert_eq!(rows.len(), 1, "one finding for the shared name: {payload}");
    assert_eq!(rows[0]["aggregation"]["member_count"], 2);
    assert_eq!(payload["summary"]["governed_claims"], 2);
    assert_eq!(payload["summary"]["unattested_claims"], 2);
    assert_eq!(
        rows[0]["candidate_fact"]["evidence"]["sources"]
            .as_array()
            .map(Vec::len),
        Some(2),
        "two distinct sources under one name"
    );
}

/// The staged path attests the same claim the commit path does.
#[test]
fn a_staged_claim_attests_like_a_committed_one() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let base = base_commit(root);
    fs::write(
        root.join("docs.md"),
        "# Docs\n\n[amiss:v]: <amiss:value?path=README.md&line=L1> \"# R\"\n",
    )
    .unwrap();
    git(root, &["add", "."]);

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = staged_index(&repo, &engine(), None, &shell(), &oid(&base)).unwrap();
    let payload = payload(&built);
    assert_eq!(built.exit_code, 0, "{payload}");
    assert_eq!(payload["summary"]["governed_claims"], 1);
    assert_eq!(payload["summary"]["unattested_claims"], 0);
}

/// Duplicates under one name aggregate per outcome kind: a broken member and
/// an unanswered member are two findings, not one blurred verdict.
#[test]
fn mixed_verdicts_under_one_name_split_by_kind() {
    let claim = "[amiss:v]: <amiss:value?path=README.md&line=L1> \"# Wrong\"\n[amiss:v]: <amiss:value?path=absent.txt&line=L1> \"x\"";
    let (_code, payload) = claimed_run(claim, Profile::Observe);
    let kinds: Vec<&str> = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["kind"].as_str())
        .filter(|kind| kind.starts_with("claim-"))
        .collect();
    assert_eq!(kinds, ["claim-broken", "claim-target-missing"], "{payload}");
    assert_eq!(payload["summary"]["governed_claims"], 2);
    assert_eq!(payload["summary"]["unattested_claims"], 2);
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn anchor_pair(root: &Path, reader: &str) -> (String, String) {
    let base = base_commit(root);
    fs::write(root.join("reader.md"), reader).unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "anchors"]);
    (base, git(root, &["rev-parse", "HEAD"]).trim().to_owned())
}

fn missing_targets(payload: &serde_json::Value) -> Vec<String> {
    payload
        .get("findings")
        .and_then(serde_json::Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter(|row| {
                    row.get("kind")
                        .is_some_and(|kind| kind == "explicit-target-missing")
                })
                .filter_map(|row| {
                    row.pointer("/key_input/scope/normalized_target_intent/fragment_digest")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The anchor lane answers identically from the retained in-set identities
/// and from the fallback parse of an excluded-tree target: the published
/// fragment resolves on both routes and only the absent one is missing.
#[test]
fn retained_and_fallback_anchor_routes_agree() {
    let body = "# Alpha\n\n## Shared Section\n\ntext\n";
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("target.md"), body).unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::write(root.join("tests/fixture.md"), body).unwrap();
    let (base, candidate) = {
        let base = base_commit(root);
        fs::write(
            root.join("reader.md"),
            "[a](target.md#shared-section)\n[b](target.md#gone)\n\
             [c](tests/fixture.md#shared-section)\n[d](tests/fixture.md#gone)\n",
        )
        .unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-qm", "anchors"]);
        (base, git(root, &["rev-parse", "HEAD"]).trim().to_owned())
    };
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let payload = payload(&built);
    let documents: Vec<(&str, &str)> = payload["documents"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| {
            Some((
                row["path"].as_str()?,
                row["candidate"]["status"].as_str().unwrap_or(""),
            ))
        })
        .collect();
    assert!(
        documents.contains(&("tests/fixture.md", "excluded-built-in")),
        "the fallback target is discovered but never scanned: {documents:?}"
    );
    assert_eq!(
        missing_targets(&payload).len(),
        2,
        "each route resolves the published fragment and misses the absent one: {}",
        payload["findings"]
    );
}

/// A supported in-set include becomes one complete anchor source, while an
/// excluded target whose relative include cannot be scanned stays partial.
#[test]
fn a_supported_include_expands_without_guessing_an_unscanned_one() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("part.rst"), "extra\n").unwrap();
    fs::write(
        root.join("whole.rst"),
        "Title\n=====\n\n.. include:: part.rst\n",
    )
    .unwrap();
    fs::write(root.join("plain.rst"), "Title\n=====\n\ntext\n").unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::write(
        root.join("tests/carried.rst"),
        "Title\n=====\n\n.. include:: part.rst\n",
    )
    .unwrap();
    let (base, candidate) = anchor_pair(
        root,
        "[a](whole.rst#gone)\n[b](plain.rst#gone)\n[c](tests/carried.rst#gone)\n",
    );
    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let built = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let payload = payload(&built);
    assert_eq!(
        missing_targets(&payload).len(),
        2,
        "the plain and expanded scanned targets both prove absence: {}",
        payload["findings"]
    );
    let unsupported = payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "unsupported-reference-semantics")
        .count();
    assert_eq!(
        unsupported, 1,
        "the excluded target keeps the unsupported boundary: {}",
        payload["findings"]
    );
}

/// The governed channel answers in every structured format: a policy-bound
/// rst-in-txt document and an adoc document each author a value claim, both
/// attest against the true line, and both block with the printed fix when
/// the target drifts.
#[test]
fn rst_and_adoc_claims_attest_and_break_like_markdown() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join(".amiss")).unwrap();
    fs::write(
        root.join(".amiss/scanner-policy.json"),
        r#"{"schema":"amiss/scanner-policy","document_includes":[{"adapter":"rst","kind":"document","path":"manual.txt"}],"protected_inventory":[],"finding_dispositions":[]}"#,
    )
    .unwrap();
    fs::write(root.join("subject.txt"), "alpha\n").unwrap();
    fs::write(
        root.join("manual.txt"),
        ".. [amiss:rst-pin]: <amiss:value?path=subject.txt&line=L1> \"alpha\"\n\nManual\n======\n",
    )
    .unwrap();
    fs::write(
        root.join("guide.adoc"),
        "// [amiss:adoc-pin]: <amiss:value?path=subject.txt&line=L1> \"alpha\"\n= Guide\n",
    )
    .unwrap();
    let base = base_commit(root);
    fs::write(root.join("subject.txt"), "beta\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "drift"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = Repository::open(root, ObjectFormat::Sha1).unwrap();
    let attested = commit_pair(&repo, &engine(), None, &shell(), &oid(&base), &oid(&base)).unwrap();
    let attested_payload = payload(&attested);
    assert_eq!(
        attested_payload["summary"]["governed_claims"], 2,
        "both carriers evaluate: {}",
        attested_payload["summary"]
    );
    assert_eq!(
        attested_payload["summary"]["unattested_claims"], 0,
        "both attest on the true line"
    );

    let broken = commit_pair(
        &repo,
        &engine(),
        None,
        &shell(),
        &oid(&base),
        &oid(&candidate),
    )
    .unwrap();
    let broken_payload = payload(&broken);
    let rows: Vec<(&str, &str)> = broken_payload["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["kind"] == "claim-broken")
        .filter_map(|row| {
            Some((
                row["key_input"]["scope"]["control_path"].as_str()?,
                row["fix"]["replacement"].as_str()?,
            ))
        })
        .collect();
    assert_eq!(
        rows.len(),
        2,
        "both carriers break: {}",
        broken_payload["findings"]
    );
    let respelled: Vec<&str> = rows.iter().map(|(_, replacement)| *replacement).collect();
    assert!(
        respelled.contains(&"// [amiss:adoc-pin]: <amiss:value?path=subject.txt&line=L1> \"beta\"")
            && respelled
                .contains(&".. [amiss:rst-pin]: <amiss:value?path=subject.txt&line=L1> \"beta\""),
        "each fix respells its own carrier whole, marker included: {respelled:?}"
    );
}
