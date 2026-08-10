use std::fs;
use std::path::Path;

use amiss_fixtures::directory_link;
use amiss_git::{Error, GitLimits, GitResources, Repository};
use amiss_wire::model::{ObjectFormat, Oid};
use tempfile::TempDir;

/// The handle/no-follow boundary is one law with one wording on every
/// platform, so it gets one test file that runs on every platform. Unix
/// refuses the reparse point in the open itself, with `O_NOFOLLOW`. Windows
/// opens the reparse point rather than its target and refuses it by its
/// attribute. The fixture links are symlinks on unix and junctions on
/// Windows, and a junction is what an unprivileged Windows process can
/// actually create, which is what keeps these assertions running on an
/// ordinary CI runner instead of being quietly skipped.
#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn repository(at: &Path) {
    fs::create_dir_all(at.join(".git/objects")).unwrap();
}

#[test]
fn an_ordinary_repository_opens_through_the_boundary() {
    let dir = TempDir::new().unwrap();
    repository(dir.path());
    assert!(
        Repository::open(dir.path(), ObjectFormat::Sha1).is_ok(),
        "an ordinary root, .git, and objects directory open"
    );
}

#[test]
fn a_reparse_point_at_the_root_is_refused() {
    let dir = TempDir::new().unwrap();
    let real = dir.path().join("real");
    repository(&real);
    let alias = dir.path().join("alias");
    directory_link(&real, &alias).unwrap();
    assert_eq!(
        Repository::open(&alias, ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "the root's final entry is never followed"
    );
}

#[test]
fn a_reparse_point_at_the_git_directory_is_refused() {
    let dir = TempDir::new().unwrap();
    let store = dir.path().join("store");
    fs::create_dir_all(store.join("objects")).unwrap();
    let root = dir.path().join("root");
    fs::create_dir_all(&root).unwrap();
    directory_link(&store, &root.join(".git")).unwrap();
    assert_eq!(
        Repository::open(&root, ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "the .git child is never followed"
    );
}

#[test]
fn a_reparse_point_at_the_objects_directory_is_refused() {
    let dir = TempDir::new().unwrap();
    let store = dir.path().join("store");
    fs::create_dir_all(&store).unwrap();
    let root = dir.path().join("root");
    fs::create_dir_all(root.join(".git")).unwrap();
    directory_link(&store, &root.join(".git/objects")).unwrap();
    assert_eq!(
        Repository::open(&root, ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "the objects directory is never followed"
    );
}

#[test]
fn a_reparse_point_in_the_object_path_is_unreadable_not_absent() {
    let dir = TempDir::new().unwrap();
    repository(dir.path());
    let store = dir.path().join("store");
    fs::create_dir_all(&store).unwrap();
    directory_link(&store, &dir.path().join(".git/objects/aa")).unwrap();
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let oid = Oid::new(ObjectFormat::Sha1, format!("aa{}", "b".repeat(38))).unwrap();
    assert_eq!(
        repo.read_object(&mut resources, &oid).unwrap_err(),
        Error::ObjectUnreadable,
        "a refused reparse point is never mistaken for an absent object"
    );
}

#[test]
fn a_directory_at_the_loose_object_path_is_not_an_object() {
    let dir = TempDir::new().unwrap();
    repository(dir.path());
    fs::create_dir_all(
        dir.path()
            .join(format!(".git/objects/aa/{}", "b".repeat(38))),
    )
    .unwrap();
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let oid = Oid::new(ObjectFormat::Sha1, format!("aa{}", "b".repeat(38))).unwrap();
    assert_eq!(
        repo.has_object(&mut resources, &oid),
        Ok(false),
        "an entry that is not an ordinary file is not an object"
    );
}

/// The `.git` file grammar is closed: `gitdir: `, one nonempty single-line
/// UTF-8 path, one optional trailing newline. Everything else, and every
/// pointer that resolves to nothing shaped like a git directory, refuses.
#[test]
fn a_malformed_or_dangling_gitdir_pointer_is_refused() {
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("missing prefix", b"elsewhere\n".to_vec()),
        ("wrong prefix spacing", b"gitdir:elsewhere\n".to_vec()),
        ("empty path", b"gitdir: \n".to_vec()),
        ("second line", b"gitdir: elsewhere\nextra\n".to_vec()),
        ("carriage return", b"gitdir: elsewhere\r\n".to_vec()),
        ("dangling target", b"gitdir: elsewhere\n".to_vec()),
        ("non-utf8 path", b"gitdir: else\xffwhere\n".to_vec()),
        (
            "oversized pointer",
            [b"gitdir: ".as_slice(), &vec![b'a'; 17_000], b"\n"].concat(),
        ),
    ];
    for (reason, pointer) in cases {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(".git"), pointer).unwrap();
        assert_eq!(
            Repository::open(dir.path(), ObjectFormat::Sha1).unwrap_err(),
            Error::RepositoryUnavailable,
            "{reason}"
        );
    }
}

/// Each grammar clause observed against a target that would resolve if the
/// clause were dropped, so a refusal cannot hide behind a dangling path.
#[test]
fn a_grammar_clause_refuses_even_when_the_loose_path_would_resolve() {
    let neutral: Vec<(&str, String)> = vec![
        ("prefix missing", "elsewhere\n".to_owned()),
        ("prefix unspaced", "gitdir:elsewhere\n".to_owned()),
        ("empty path resolving to the root", "gitdir: \n".to_owned()),
    ];
    for (reason, pointer) in neutral {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("elsewhere/objects")).unwrap();
        fs::create_dir_all(dir.path().join("objects")).unwrap();
        fs::write(dir.path().join(".git"), pointer).unwrap();
        assert_eq!(
            Repository::open(dir.path(), ObjectFormat::Sha1).unwrap_err(),
            Error::RepositoryUnavailable,
            "{reason}"
        );
    }
    #[cfg(unix)]
    {
        let cases: Vec<(&str, &str, &str)> = vec![
            ("carriage return", "gitdir: else\r\n", "else\r"),
            ("second line", "gitdir: else\nwhere\n", "else\nwhere"),
        ];
        for (reason, pointer, literal_target) in cases {
            let dir = TempDir::new().unwrap();
            fs::create_dir_all(dir.path().join(literal_target).join("objects")).unwrap();
            fs::write(dir.path().join(".git"), pointer).unwrap();
            assert_eq!(
                Repository::open(dir.path(), ObjectFormat::Sha1).unwrap_err(),
                Error::RepositoryUnavailable,
                "{reason}"
            );
        }
    }
}

/// A relative pointer resolves against the checkout root, never the process
/// working directory, proven by a target only the root-join reaches.
#[test]
fn a_relative_gitdir_pointer_resolves_against_the_checkout_root() {
    let dir = TempDir::new().unwrap();
    let common = dir.path().join("main/.git");
    fs::create_dir_all(common.join("objects")).unwrap();
    let private = common.join("worktrees/wt");
    fs::create_dir_all(&private).unwrap();
    fs::write(private.join("commondir"), "../..\n").unwrap();
    let checkout = dir.path().join("wt");
    fs::create_dir_all(&checkout).unwrap();
    fs::write(checkout.join(".git"), "gitdir: ../main/.git/worktrees/wt\n").unwrap();
    assert!(
        Repository::open(&checkout, ObjectFormat::Sha1).is_ok(),
        "the relative pointer joins onto the root"
    );
}

#[test]
fn a_gitdir_pointer_to_a_file_or_reparse_point_is_refused() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("plain"), "not a directory").unwrap();
    fs::write(dir.path().join(".git"), "gitdir: plain\n").unwrap();
    assert_eq!(
        Repository::open(dir.path(), ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "a pointer to a regular file"
    );

    let linked = TempDir::new().unwrap();
    let store = linked.path().join("store");
    fs::create_dir_all(store.join("objects")).unwrap();
    directory_link(&store, &linked.path().join("alias")).unwrap();
    fs::write(linked.path().join(".git"), "gitdir: alias\n").unwrap();
    assert_eq!(
        Repository::open(linked.path(), ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "a pointer target is never followed through a reparse point"
    );
}

#[test]
fn a_commondir_that_reaches_no_object_store_is_refused() {
    let dir = TempDir::new().unwrap();
    let private = dir.path().join("private");
    fs::create_dir_all(&private).unwrap();
    fs::write(private.join("commondir"), "../common\n").unwrap();
    fs::create_dir_all(dir.path().join("common")).unwrap();
    fs::write(dir.path().join(".git"), "gitdir: private\n").unwrap();
    assert_eq!(
        Repository::open(dir.path(), ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "a common directory without objects"
    );

    let chained = TempDir::new().unwrap();
    let private = chained.path().join("private");
    fs::create_dir_all(&private).unwrap();
    fs::create_dir_all(private.join("objects")).unwrap();
    fs::write(private.join("commondir"), "gone\n").unwrap();
    fs::write(chained.path().join(".git"), "gitdir: private\n").unwrap();
    assert_eq!(
        Repository::open(chained.path(), ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "a present commondir binds; private objects are never a fallback"
    );

    let refused = TempDir::new().unwrap();
    let private = refused.path().join("private");
    fs::create_dir_all(private.join("objects")).unwrap();
    let store = refused.path().join("store");
    fs::create_dir_all(&store).unwrap();
    directory_link(&store, &private.join("commondir")).unwrap();
    fs::write(refused.path().join(".git"), "gitdir: private\n").unwrap();
    assert_eq!(
        Repository::open(refused.path(), ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "a refused commondir is present, not absent; the fallback is NotFound-only"
    );
}

/// The hand-built two-hop shape, no git binary: a worktree-form checkout
/// whose commondir names the store, read back through the opened handles.
#[test]
fn a_two_hop_worktree_shape_opens_and_reads() {
    let dir = TempDir::new().unwrap();
    let common = dir.path().join("main/.git");
    fs::create_dir_all(common.join("objects")).unwrap();
    let private = common.join("worktrees/wt");
    fs::create_dir_all(&private).unwrap();
    fs::write(private.join("commondir"), "../..\n").unwrap();
    let checkout = dir.path().join("wt");
    fs::create_dir_all(&checkout).unwrap();
    fs::write(
        checkout.join(".git"),
        format!("gitdir: {}\n", private.display()),
    )
    .unwrap();

    let body = b"boundary blob";
    let oid = amiss_fixtures::loose_object(&dir.path().join("main"), "blob", body).unwrap();
    let repo = Repository::open(&checkout, ObjectFormat::Sha1).unwrap();
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let object = repo
        .read_object(&mut resources, &Oid::new(ObjectFormat::Sha1, oid).unwrap())
        .unwrap();
    assert_eq!(object.body, body, "objects come from the common store");
}

#[test]
fn an_absent_repository_is_refused() {
    let dir = TempDir::new().unwrap();
    assert_eq!(
        Repository::open(&dir.path().join("nowhere"), ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "an absent root"
    );
    assert_eq!(
        Repository::open(dir.path(), ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "an absent .git"
    );
}

/// The three real layouts git itself writes, each opened and read: a linked
/// worktree, a separate-git-dir checkout, and a worktree of a bare main
/// whose bare root stays refused directly.
#[test]
fn real_git_worktree_layouts_open_through_the_boundary() {
    let dir = TempDir::new().unwrap();
    let main = dir.path().join("main");
    fs::create_dir_all(&main).unwrap();
    amiss_fixtures::git(&main, &["init", "-q"]).unwrap();
    fs::write(main.join("README.md"), "# R\n").unwrap();
    amiss_fixtures::git(&main, &["add", "."]).unwrap();
    amiss_fixtures::git(&main, &["commit", "-qm", "base"]).unwrap();
    let tree = amiss_fixtures::git(&main, &["rev-parse", "HEAD^{tree}"])
        .unwrap()
        .trim()
        .to_owned();
    let worktree = dir.path().join("wt");
    amiss_fixtures::git(
        &main,
        &["worktree", "add", "-q", worktree.to_str().unwrap(), "HEAD"],
    )
    .unwrap();
    let repo = Repository::open(&worktree, ObjectFormat::Sha1).unwrap();
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    let oid = Oid::new(ObjectFormat::Sha1, tree).unwrap();
    assert!(
        repo.read_object(&mut resources, &oid).is_ok(),
        "a linked worktree reads through its commondir"
    );

    let separate = dir.path().join("separate");
    let store = dir.path().join("store.git");
    amiss_fixtures::git(
        dir.path(),
        &[
            "init",
            "-q",
            "--separate-git-dir",
            store.to_str().unwrap(),
            separate.to_str().unwrap(),
        ],
    )
    .unwrap();
    assert!(
        Repository::open(&separate, ObjectFormat::Sha1).is_ok(),
        "a separate-git-dir checkout holds objects one hop away"
    );

    let bare = dir.path().join("bare.git");
    amiss_fixtures::git(
        dir.path(),
        &[
            "clone",
            "-q",
            "--bare",
            main.to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(
        Repository::open(&bare, ObjectFormat::Sha1).unwrap_err(),
        Error::RepositoryUnavailable,
        "a bare repository stays refused directly"
    );
    let bare_worktree = dir.path().join("bare-wt");
    amiss_fixtures::git(
        &bare,
        &[
            "worktree",
            "add",
            "-q",
            bare_worktree.to_str().unwrap(),
            "HEAD",
        ],
    )
    .unwrap();
    assert!(
        Repository::open(&bare_worktree, ObjectFormat::Sha1).is_ok(),
        "a worktree of a bare main reaches the bare store through commondir"
    );
}
