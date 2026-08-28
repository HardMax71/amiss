use amiss_scan::claim;
use amiss_scan::claim::{GovernedForm, ValueClaim, classify};
use amiss_wire::extraction::GovernedDefinition;
use amiss_wire::model::RepoPath;

fn definition(label: &str, url: &str, title: Option<&str>, angled: bool) -> GovernedDefinition {
    GovernedDefinition {
        span: (0, 1),
        url: url.to_owned(),
        title: title.map(str::to_owned),
        label: label.to_owned(),
        angled,
        previous_code: None,
    }
}

fn canonical() -> GovernedDefinition {
    definition(
        "amiss:pkg-version",
        "amiss:value?path=Cargo.toml&line=L3",
        Some("version = 0.16.0"),
        true,
    )
}

/// The canonical spelling parses to exactly its own words, and the widest
/// lawful values stay inside the grammar.
#[test]
fn the_canonical_value_claim_parses_to_its_words() {
    assert_eq!(
        classify(&canonical()),
        GovernedForm::Value(ValueClaim {
            name: "pkg-version".to_owned(),
            path: RepoPath::new("Cargo.toml".to_owned()).expect("a path"),
            line: 3,
            expected: "version = 0.16.0".to_owned(),
        })
    );

    let widest = definition(
        &format!("amiss:{}", "n".repeat(120)),
        "amiss:value?path=a.md&line=L9007199254740991",
        Some(""),
        true,
    );
    let GovernedForm::Value(claim) = classify(&widest) else {
        panic!("the widest lawful claim parses");
    };
    assert_eq!(claim.line, 9_007_199_254_740_991);
    assert_eq!(claim.expected, "", "an empty expectation is lawful");
    assert_eq!(claim.name.len(), 120);
}

type Deviation = fn(&mut GovernedDefinition);

/// Every clause of the closed grammar refuses alone; a definition that trips
/// two proves neither.
#[test]
fn each_grammar_clause_refuses_alone() {
    let deviations: [(&str, Deviation); 15] = [
        ("a bare destination", |definition| definition.angled = false),
        ("a missing title", |definition| definition.title = None),
        ("an empty name", |definition| {
            definition.label = "amiss:".to_owned();
        }),
        ("a name led by punctuation", |definition| {
            definition.label = "amiss:-x".to_owned();
        }),
        ("a slash in the name", |definition| {
            definition.label = "amiss:a/b".to_owned();
        }),
        ("a name one byte past the ceiling", |definition| {
            definition.label = format!("amiss:{}", "n".repeat(121));
        }),
        ("an unknown kind", |definition| {
            definition.url = "amiss:value2?path=Cargo.toml&line=L3".to_owned();
        }),
        ("reordered parameters", |definition| {
            definition.url = "amiss:value?line=L3&path=Cargo.toml".to_owned();
        }),
        ("a third parameter", |definition| {
            definition.url = "amiss:value?path=Cargo.toml&line=L3&x=1".to_owned();
        }),
        ("an empty path", |definition| {
            definition.url = "amiss:value?path=&line=L3".to_owned();
        }),
        ("a query byte inside the path", |definition| {
            definition.url = "amiss:value?path=a?b.md&line=L3".to_owned();
        }),
        ("a traversal path", |definition| {
            definition.url = "amiss:value?path=../a.md&line=L3".to_owned();
        }),
        ("line zero", |definition| {
            definition.url = "amiss:value?path=Cargo.toml&line=L0".to_owned();
        }),
        ("a padded line", |definition| {
            definition.url = "amiss:value?path=Cargo.toml&line=L03".to_owned();
        }),
        ("a line past the safe range", |definition| {
            definition.url = "amiss:value?path=Cargo.toml&line=L9007199254740992".to_owned();
        }),
    ];
    for (reason, deviate) in deviations {
        let mut deviant = canonical();
        deviate(&mut deviant);
        assert_eq!(classify(&deviant), GovernedForm::Unknown, "{reason}");
    }
}

/// The rewrite's two gates each refuse alone: the spellability guard turns
/// away control bytes the title parser itself would keep, and the round-trip
/// proof turns away a path whose ampersand would re-split the claim grammar.
#[test]
fn a_rewrite_is_proved_or_refused_on_each_gate() {
    let path = |text: &str| RepoPath::new(text.to_owned()).unwrap();
    assert_eq!(
        claim::rewrite(
            "v",
            &path("README.md"),
            1,
            b"# R",
            claim::ClaimCarrier::Definition,
        ),
        Some("[amiss:v]: <amiss:value?path=README.md&line=L1> \"# R\"".to_owned()),
    );
    assert_eq!(
        claim::rewrite(
            "v",
            &path("README.md"),
            1,
            b"say \"hi\"",
            claim::ClaimCarrier::Definition,
        ),
        None,
        "a double quote cannot sit in a quoted title"
    );
    assert_eq!(
        claim::rewrite(
            "v",
            &path("README.md"),
            1,
            b"a\tb",
            claim::ClaimCarrier::Definition,
        ),
        None,
        "the guard refuses control bytes on its own clause"
    );
    assert_eq!(
        claim::rewrite(
            "v",
            &path("a&b.md"),
            1,
            b"words",
            claim::ClaimCarrier::Definition,
        ),
        None,
        "an ampersand path would re-split the grammar, which only the round trip sees"
    );
    assert_eq!(
        claim::rewrite(
            "v",
            &path("README.md"),
            1,
            b"\xff",
            claim::ClaimCarrier::Definition,
        ),
        None
    );
    assert_eq!(
        claim::rewrite(
            "v",
            &path("README.md"),
            1,
            b"a\\b",
            claim::ClaimCarrier::Definition,
        ),
        None,
        "a backslash is refused by the guard's own clause, conservatively"
    );
    assert_eq!(
        claim::rewrite(
            "v",
            &path("README.md"),
            1,
            b"&amp;",
            claim::ClaimCarrier::Definition,
        ),
        None,
        "an entity decodes to different expected words, which only the field proof sees"
    );
}
