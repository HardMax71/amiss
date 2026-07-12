use std::fs;
use std::path::{Path, PathBuf};

use amiss_md::corpus::{self, Case};
use amiss_wire::digest::hb;
use amiss_wire::json::canonical;
use sha2::{Digest as _, Sha256};

/// The corpus identity. Regenerating with `AMISS_CORPUS_BLESS=1` rewrites the
/// manifest; this constant must then be updated by hand, so no golden can move
/// without the move appearing in review.
const CORPUS_DIGEST: &str =
    "sha256:133cc1f8ebad8b55c9e4aa1bedad40465b017f7c67f41ecd7a30fb7cb1f6d128";

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn input(name: &str, pin: &str) -> Vec<u8> {
    let path = root().join("corpus/inputs").join(name);
    let bytes = fs::read(path).unwrap();
    let mut hex = String::from("sha256:");
    for byte in Sha256::digest(&bytes) {
        hex.push(char::from_digit(u32::from(byte >> 4), 16).unwrap());
        hex.push(char::from_digit(u32::from(byte & 0x0f), 16).unwrap());
    }
    assert_eq!(hex, pin, "{name} drifted from its pinned digest");
    bytes
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[expect(clippy::unwrap_used, reason = "test fixture helper")]
fn cases() -> Vec<Case> {
    let commonmark = corpus::commonmark(&input(
        "commonmark-0.31.2.spec.json",
        corpus::COMMONMARK_PIN,
    ))
    .unwrap();
    let gfm_text = String::from_utf8(input("gfm-0.29.spec.txt", corpus::GFM_PIN)).unwrap();
    let mut all = commonmark;
    all.extend(corpus::gfm(&gfm_text));
    all
}

/// The manifest is the gate: every case's raw source with its exact node count
/// and depth under every published profile. It is regenerated here and must
/// reproduce the checked-in bytes and digest exactly.
#[test]
fn manifest_reproduces() {
    let cases = cases();
    assert_eq!(cases.len(), 1324, "652 CommonMark plus 672 GFM examples");

    let mut wire = canonical(&corpus::manifest(&cases));
    wire.push(b'\n');
    let digest = hb(corpus::SCHEMA, &wire).to_string();
    let path = root().join("corpus/parser-profile-corpus-v1.json");

    if std::env::var_os("AMISS_CORPUS_BLESS").is_some() {
        fs::write(&path, &wire).unwrap();
        println!("blessed corpus digest: {digest}");
        return;
    }

    let checked_in = fs::read(&path).unwrap();
    assert!(
        checked_in == wire,
        "the checked-in corpus is {} bytes and regeneration produced {}",
        checked_in.len(),
        wire.len()
    );
    assert_eq!(digest, CORPUS_DIGEST, "the corpus digest moved");
}

/// Frontmatter contributes no parser node, so charging a document is
/// independent of a recognized header even when that header is full of braces,
/// JSX-looking text, imports, and link syntax.
#[test]
fn hostile_frontmatter_changes_no_charge() {
    let body = "# Title\n\nSee [docs](./a.md).\n";
    let hostile = format!(
        "---\ntitle: {{ a: <b/> }}\nimport: \"[x](y)\"\nlist:\n  - \"{{expr}}\"\n---\n{body}"
    );
    let bare = amiss_md::charge(amiss_wire::model::Adapter::Markdown, body.as_bytes());
    let with_header = amiss_md::charge(amiss_wire::model::Adapter::Markdown, hostile.as_bytes());
    assert_eq!(bare, with_header);
}
