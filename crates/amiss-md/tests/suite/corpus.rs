use std::fs;
use std::path::{Path, PathBuf};

use amiss_md::corpus;
use amiss_wire::digest::hb;
use amiss_wire::json::canonical;

use crate::fixtures::harvest;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The corpus identity. Regenerating with `AMISS_CORPUS_BLESS=1` rewrites the
/// manifest; this constant must then be updated by hand, so no golden can move
/// without the move appearing in review.
const CORPUS_DIGEST: &str =
    "sha256:e814a08945eac97891e9efe187bd2c38ad799b9bab5142331622a6d145fc3e0c";

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
    let path = root().join("corpus/parser-profile-corpus.json");

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

/// One crafted suite driving every corner of the fixture reader: a literal
/// holding a close paren, template sources with and without substitutions,
/// unicode escapes in both spellings, a substitution inside the config, and
/// the nearest-assertion rule that separates rejection from acceptance.
#[test]
fn the_fixture_reader_walks_every_literal_shape() {
    let suite = r"
t.test('crafted', function () {
  assert.equal(micromark('a)b'), '<p>a)b</p>')
  assert.equal(micromark(`x${sub}y`), '<p>never</p>')
  assert.equal(micromark(`a$b`), `<i>kept</i>`)
  assert.equal(micromark('p${q}r'), other)
  assert.equal(micromark('A\u0041\u{42}Z'), '<p>u</p>')
  assert.equal(micromark('cfg', {ext: `(${(v)})`}), '<p>cfg</p>')
  assert.throws(function () { micromark('one') }, /first/)
  assert.equal(micromark('two'), '<p>two</p>')
  assert.deepEqual(micromark('tree'), tree)
  assert.throws(function () { micromark('three') }, /last/)
  assert.equal(micromark('s1', {a: '${', b: '}'}), '<p>s1</p>')
  assert.equal(micromark('s2', {t: `a{b`}), '<p>s2</p>')
  assert.equal(micromark('s3', {u: `x${(`)`)}y`}), '<p>s3</p>')
  assert.equal(micromark('tail'), '<p>tail</p>')
})
";
    let fixtures = corpus::micromark_fixtures("crafted", suite);
    let seen: Vec<(String, corpus::Expect)> = fixtures
        .cases
        .iter()
        .map(|case| (case.source.clone(), case.expect.clone()))
        .collect();
    assert_eq!(
        seen,
        vec![
            (
                "a)b".to_owned(),
                corpus::Expect::Html("<p>a)b</p>".to_owned())
            ),
            (
                "a$b".to_owned(),
                corpus::Expect::Html("<i>kept</i>".to_owned())
            ),
            ("p${q}r".to_owned(), corpus::Expect::Accepted),
            (
                "AABZ".to_owned(),
                corpus::Expect::Html("<p>u</p>".to_owned())
            ),
            (
                "cfg".to_owned(),
                corpus::Expect::Html("<p>cfg</p>".to_owned())
            ),
            (
                "one".to_owned(),
                corpus::Expect::Rejected("first".to_owned())
            ),
            (
                "two".to_owned(),
                corpus::Expect::Html("<p>two</p>".to_owned())
            ),
            ("tree".to_owned(), corpus::Expect::Accepted),
            (
                "three".to_owned(),
                corpus::Expect::Rejected("last".to_owned())
            ),
            (
                "s1".to_owned(),
                corpus::Expect::Html("<p>s1</p>".to_owned())
            ),
            (
                "s2".to_owned(),
                corpus::Expect::Html("<p>s2</p>".to_owned())
            ),
            (
                "s3".to_owned(),
                corpus::Expect::Html("<p>s3</p>".to_owned())
            ),
            (
                "tail".to_owned(),
                corpus::Expect::Html("<p>tail</p>".to_owned())
            ),
        ],
    );
    assert_eq!(fixtures.skipped, 1, "the substitution source is refused");
    let config = fixtures
        .cases
        .iter()
        .find(|case| case.source == "cfg")
        .map(|case| case.config.clone())
        .unwrap_or_default();
    assert!(
        config.contains("`(${(v)})`"),
        "the config template survives whole: {config:?}"
    );
}
