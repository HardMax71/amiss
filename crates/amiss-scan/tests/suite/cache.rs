#![expect(
    clippy::unwrap_used,
    reason = "integration harness over asserted fixture shapes"
)]
use sha2::Digest as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use amiss_git::{GitLimits, GitResources, Repository};
use amiss_scan::cache::ScanCacheRow;
use amiss_scan::{ScanCache, ScanLimits, ScanResources, SnapshotDiscovery, discover};
use amiss_wire::envelope::Payload as _;
use amiss_wire::model::{Digest, ObjectFormat, Oid};
use tempfile::TempDir;

fn engine(label: &str) -> Digest {
    Digest::from(
        sha2::Sha256::new_with_prefix("amiss/scanner-engine")
            .chain_update([0_u8])
            .chain_update(label.as_bytes())
            .finalize()
            .0,
    )
}

fn repository(documents: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    amiss_fixtures::representative_repository(dir.path(), documents).unwrap();
    dir
}

fn head_tree(dir: &Path) -> Oid {
    let hex = amiss_fixtures::git(dir, &["rev-parse", "HEAD^{tree}"])
        .unwrap()
        .trim()
        .to_owned();
    Oid::new(ObjectFormat::Sha1, hex).unwrap()
}

fn discover_with(dir: &Path, cache: Option<Arc<ScanCache>>) -> SnapshotDiscovery {
    let repo = Repository::open(dir, ObjectFormat::Sha1).unwrap();
    let mut git = GitResources::new(GitLimits::CONTRACT);
    let mut scan = ScanResources::new(ScanLimits::CONTRACT).with_scan_cache(cache);
    discover(
        &repo,
        &mut git,
        &mut scan,
        &amiss_scan::Includes::default(),
        &head_tree(dir),
    )
    .unwrap()
}

fn rows(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn a_second_run_reads_the_rows_the_first_run_wrote() {
    let repo = repository(12);
    let store = TempDir::new().unwrap();
    let cache = ScanCache::open(store.path().to_path_buf(), engine("one"));
    let bare = discover_with(repo.path(), None);
    let first = discover_with(repo.path(), Some(Arc::clone(&cache)));
    assert_eq!(bare, first);
    let written = rows(store.path());
    assert_eq!(written.len(), 13);
    let stamps: Vec<_> = written
        .iter()
        .map(|row| fs::metadata(row).unwrap().modified().unwrap())
        .collect();
    let second = discover_with(repo.path(), Some(cache));
    assert_eq!(first, second);
    let untouched: Vec<_> = written
        .iter()
        .map(|row| fs::metadata(row).unwrap().modified().unwrap())
        .collect();
    assert_eq!(stamps, untouched);
}

#[test]
fn a_damaged_or_foreign_row_costs_a_parse_and_never_an_answer() {
    let repo = repository(6);
    let store = TempDir::new().unwrap();
    let bare = discover_with(repo.path(), None);
    let cache = ScanCache::open(store.path().to_path_buf(), engine("one"));
    discover_with(repo.path(), Some(Arc::clone(&cache)));
    let written = rows(store.path());
    let [truncated, garbled, ..] = written.as_slice() else {
        panic!("rows written: {}", written.len());
    };
    let bytes = fs::read(truncated).unwrap();
    fs::write(truncated, &bytes[..bytes.len() / 2]).unwrap();
    let bytes = fs::read(garbled).unwrap();
    let mut flipped = bytes.clone();
    let inside = flipped.iter().position(|byte| *byte == b'"').unwrap() + 40;
    flipped[inside] ^= 0x01;
    fs::write(garbled, &flipped).unwrap();
    assert!(ScanCacheRow::parse(&fs::read(truncated).unwrap()).is_err());
    assert!(ScanCacheRow::parse(&fs::read(garbled).unwrap()).is_err());
    assert_eq!(bare, discover_with(repo.path(), Some(Arc::clone(&cache))));
    assert!(ScanCacheRow::parse(&fs::read(truncated).unwrap()).is_ok());
    assert!(ScanCacheRow::parse(&fs::read(garbled).unwrap()).is_ok());
    let other = ScanCache::open(store.path().to_path_buf(), engine("two"));
    assert_eq!(bare, discover_with(repo.path(), Some(other)));
    assert_eq!(rows(store.path()).len(), written.len() * 2);
}

#[test]
fn a_row_is_a_sealed_document_naming_its_engine() {
    let repo = repository(2);
    let store = TempDir::new().unwrap();
    let cache = ScanCache::open(store.path().to_path_buf(), engine("one"));
    discover_with(repo.path(), Some(cache));
    let row = rows(store.path()).into_iter().next().unwrap();
    let bytes = fs::read(&row).unwrap();
    let parsed = ScanCacheRow::parse(&bytes).unwrap();
    assert_eq!(parsed.payload.engine, engine("one"));
    let shown = row.to_str().unwrap();
    assert!(
        shown.contains(&engine("one").to_string().replace(':', "-")),
        "{shown}"
    );
    let mut damaged = bytes.clone();
    let at = damaged.len() / 2;
    damaged[at] ^= 0x01;
    assert!(ScanCacheRow::parse(&damaged).is_err());
}
