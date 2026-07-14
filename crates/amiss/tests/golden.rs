use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

#[expect(clippy::expect_used, reason = "test fixture helper")]
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("absent-global-config"))
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("git output utf-8")
}

fn golden_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/commit-pair.json")
}

/// Every author date, committer date, file byte, and identity below is pinned,
/// so the object IDs, and through them the entire report, are a function of
/// nothing but this fixture and the engine.
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test fixture helper"
)]
fn fixed_evaluation() -> (i32, Vec<u8>) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("README.md"), "See [the guide](docs/guide.md).\n").unwrap();
    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[home](../README.md)\n",
    )
    .unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(
        root.join("docs/guide.md"),
        "# Guide\n\n[home](../README.md) and [gone](missing.md)\n",
    )
    .unwrap();
    fs::write(root.join("docs/unlinked.md"), "# Alone\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "candidate"]);
    let candidate = git(root, &["rev-parse", "HEAD"]).trim().to_owned();

    let repo = root.to_str().unwrap().to_owned();
    let output = Command::new(env!("CARGO_BIN_EXE_amiss"))
        .args([
            "check",
            "--repo",
            &repo,
            "--object-format",
            "sha1",
            "--base",
            &base,
            "--candidate",
            &candidate,
            "--repository",
            "github.com/acme/docs",
            "--ref",
            "refs/heads/main",
            "--default-branch-ref",
            "refs/heads/main",
            "--profile",
            "enforce",
            "--format",
            "json",
        ])
        .output()
        .expect("run amiss");
    (output.status.code().unwrap_or(-1), output.stdout)
}

/// The report names the exact binary that produced it: `engine_digest` is a
/// hash of the running executable, and `payload_digest` moves with it. Those
/// are the two spans a golden must not pin, because they differ between a
/// debug and a release build of the same source, and between platforms, by
/// design. Each occurs exactly once on the wire, asserted here, and the
/// golden stores both zeroed, so the placeholder announces itself.
#[expect(clippy::indexing_slicing, reason = "asserted fixture shapes")]
fn normalized(mut wire: Vec<u8>) -> Vec<u8> {
    for marker in [
        b"\"engine_digest\":\"sha256:".as_slice(),
        b"\"payload_digest\":\"sha256:".as_slice(),
    ] {
        let hits: Vec<usize> = wire
            .windows(marker.len())
            .enumerate()
            .filter(|(_, window)| *window == marker)
            .map(|(at, _)| at)
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "the wire carries {} exactly once",
            String::from_utf8_lossy(marker)
        );
        let start = hits[0].saturating_add(marker.len());
        wire[start..start.saturating_add(64)].fill(b'0');
    }
    wire
}

/// The determinism suite proves each platform agrees with itself: the same
/// input twice, the same bytes. This golden is the half no single run can
/// prove, that every platform agrees with every other. The fixture pins all
/// its inputs, the emitted wire is committed with the two self-referential
/// digests zeroed, and the CI matrix runs this same comparison on Linux,
/// macOS, and Windows, so a report that encodes anything host-shaped, a
/// path, a line ending, a locale, an ordering, fails the platform it
/// diverges on. The evaluation deliberately fails under enforce, because a
/// golden that only ever pins passing runs would let the failing shape
/// drift.
///
/// When the report format changes on purpose, regenerate with
/// `AMISS_BLESS_GOLDEN=1 cargo nextest run -p amiss golden` and commit the
/// diff, which is then the reviewable statement of exactly what changed on
/// the wire.
#[test]
fn the_wire_bytes_of_a_fixed_evaluation_match_the_committed_golden() {
    let (code, wire) = fixed_evaluation();
    assert_eq!(code, 1, "the fixture fails under enforce, by design");
    let wire = normalized(wire);

    if std::env::var_os("AMISS_BLESS_GOLDEN").is_some() {
        fs::write(golden_path(), &wire).unwrap();
        return;
    }

    let golden = fs::read(golden_path())
        .expect("the committed golden exists; regenerate with AMISS_BLESS_GOLDEN=1");
    if wire != golden {
        let divergence = wire
            .iter()
            .zip(golden.iter())
            .position(|(actual, expected)| actual != expected)
            .unwrap_or_else(|| wire.len().min(golden.len()));
        let dump = std::env::temp_dir().join("amiss-golden-actual.json");
        fs::write(&dump, &wire).unwrap();
        panic!(
            "the emitted wire diverges from the committed golden: \
             {} bytes emitted vs {} committed, first divergence at byte {divergence}; \
             the normalized emitted bytes are at {} for comparison, and if the \
             change is deliberate, regenerate with AMISS_BLESS_GOLDEN=1 and \
             commit the diff",
            wire.len(),
            golden.len(),
            dump.display()
        );
    }
}
