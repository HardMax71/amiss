mod fixtures;

use std::fs;
use std::path::{Path, PathBuf};

use amiss_md::corpus;
use amiss_wire::digest::hb;
use amiss_wire::json::canonical;

use fixtures::harvest;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The corpus identity. Regenerating with `AMISS_CORPUS_BLESS=1` rewrites the
/// manifest; this constant must then be updated by hand, so no golden can move
/// without the move appearing in review.
const CORPUS_DIGEST: &str =
    "sha256:e992411de9070490c3c17586e4edbae3a791b09de5f94d914b379a8dada04544";

/// The manifest is the gate: every case's raw source with its exact node count
/// and depth under every published profile. It is regenerated here and must
/// reproduce the checked-in bytes and digest exactly.
#[test]
fn manifest_reproduces() {
    let (cases, skipped) = harvest();
    assert_eq!(
        cases.len(),
        1639,
        "652 CommonMark, 672 GFM, 257 MDX, 29 GFM-bundle, 29 github footnote"
    );
    assert_eq!(
        skipped.iter().map(|(_, count)| *count).sum::<usize>(),
        12,
        "dropped fixtures pass a variable or concatenate their source"
    );

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_silenced| {}));
    let built = corpus::manifest(&cases, &skipped);
    std::panic::set_hook(previous);

    let mut wire = canonical(&built);
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
