use amiss_wire::ExitClass;
use amiss_wire::digest::{hb, hj};
use amiss_wire::json::{ErrorKind, MAX_SAFE_INTEGER, Value, canonical, parse};

#[expect(clippy::unwrap_used, reason = "test helper on inputs that must fail")]
fn kind(input: &[u8]) -> ErrorKind {
    parse(input).unwrap_err().kind
}

#[test]
fn exit_codes_are_contract() {
    assert_eq!(ExitClass::Success.code(), 0);
    assert_eq!(ExitClass::BlockingFindings.code(), 1);
    assert_eq!(ExitClass::Failure.code(), 2);
}

#[test]
fn accepts_the_restricted_grammar() {
    assert_eq!(parse(b"null").unwrap(), Value::Null);
    assert_eq!(parse(b" true ").unwrap(), Value::Bool(true));
    assert_eq!(parse(b"-1").unwrap(), Value::Integer(-1));
    assert_eq!(
        parse(b"9007199254740991").unwrap(),
        Value::Integer(MAX_SAFE_INTEGER)
    );
    assert_eq!(
        parse(b"-9007199254740991").unwrap(),
        Value::Integer(-9_007_199_254_740_991)
    );
    assert_eq!(
        parse(br#""A\/\n""#).unwrap(),
        Value::String("A/\n".to_owned())
    );
    assert_eq!(
        parse("\"\u{1f600}\"".as_bytes()).unwrap(),
        Value::String("\u{1f600}".to_owned())
    );
    assert_eq!(
        parse(b"[0, {\"a\": []}]").unwrap(),
        Value::Array(vec![
            Value::Integer(0),
            Value::Object(vec![("a".to_owned(), Value::Array(Vec::new()))]),
        ])
    );
}

#[test]
fn rejects_everything_the_contract_names() {
    let cases: &[(&[u8], ErrorKind)] = &[
        (b"", ErrorKind::UnexpectedEnd),
        (b"\xEF\xBB\xBF{}", ErrorKind::ByteOrderMark),
        (b"\xff", ErrorKind::InvalidUtf8),
        (br#"{"a":1,"a":2}"#, ErrorKind::DuplicateKey),
        (br#"{"a":1,"a":2}"#, ErrorKind::DuplicateKey),
        (b"-0", ErrorKind::NegativeZero),
        (b"1.5", ErrorKind::FractionOrExponent),
        (b"1e3", ErrorKind::FractionOrExponent),
        (b"0E0", ErrorKind::FractionOrExponent),
        (b"9007199254740992", ErrorKind::IntegerOutOfRange),
        (b"-9007199254740992", ErrorKind::IntegerOutOfRange),
        (b"99999999999999999999", ErrorKind::IntegerOutOfRange),
        (b"01", ErrorKind::UnexpectedByte),
        (br#""\ud800""#, ErrorKind::LoneSurrogate),
        (br#""\udc00""#, ErrorKind::LoneSurrogate),
        (br#""\ud83dx""#, ErrorKind::LoneSurrogate),
        (br#""\x""#, ErrorKind::InvalidEscape),
        (br#""\u00g0""#, ErrorKind::InvalidEscape),
        (b"\"\x01\"", ErrorKind::ControlCharacter),
        (b"1 2", ErrorKind::TrailingContent),
        (b"{} {}", ErrorKind::TrailingContent),
        (b"{", ErrorKind::UnexpectedEnd),
        (br#"{"a":1"#, ErrorKind::UnexpectedEnd),
        (b"[1,]", ErrorKind::UnexpectedByte),
        (b"{\"a\":1,}", ErrorKind::UnexpectedByte),
        (b"nul", ErrorKind::UnexpectedEnd),
        (b"nulL", ErrorKind::UnexpectedByte),
        (b"'a'", ErrorKind::UnexpectedByte),
    ];
    for (input, expected) in cases {
        assert_eq!(
            kind(input),
            *expected,
            "input {:?}",
            String::from_utf8_lossy(input)
        );
    }
}

#[test]
fn rejects_past_the_depth_limit() {
    let mut deep = vec![b'['; 600];
    deep.extend(vec![b']'; 600]);
    assert_eq!(kind(&deep), ErrorKind::DepthLimit);
}

#[test]
fn error_offsets_point_at_the_defect() {
    assert_eq!(parse(b"1 2").unwrap_err().offset, 2);
    assert_eq!(parse(br#"{"a":1,"a":2}"#).unwrap_err().offset, 7);
}

#[test]
fn canonical_matches_the_gv003_bytes() {
    let value = parse("{ \"z\" : \"\u{e9}\", \"a\" : 1 }".as_bytes()).unwrap();
    assert_eq!(canonical(&value), "{\"a\":1,\"z\":\"\u{e9}\"}".as_bytes());
}

#[test]
fn canonical_sorts_keys_by_utf16_code_units() {
    let astral = "\u{10000}";
    let bmp = "\u{fffd}";
    let input = format!("{{\"{bmp}\":2,\"{astral}\":1}}");
    let value = parse(input.as_bytes()).unwrap();
    let expected = format!("{{\"{astral}\":1,\"{bmp}\":2}}");
    assert_eq!(canonical(&value), expected.into_bytes());
}

#[test]
fn canonical_escapes_match_jcs() {
    let value = parse(br#"["\u0007\b\/<\">"]"#).unwrap();
    assert_eq!(canonical(&value), b"[\"\\u0007\\b/<\\\">\"]");
}

#[test]
fn reproduces_the_normative_seed_vectors() {
    let gv001 = parse(br#"{"claim_id":"docs.expr-precedence"}"#).unwrap();
    assert_eq!(
        hj("assure/claim-key/v1", &gv001).to_string(),
        "sha256:f6a22f480cab9ed6e0fc82bcbe67eba85d88f10103f5107008809dec44fb71b0"
    );

    let gv002 = parse(br#"{"members":[]}"#).unwrap();
    assert_eq!(
        hj("assure/path-set-projection/v1", &gv002).to_string(),
        "sha256:6765a67e22b2efbaaf89509cd34a70682613f002cd82d0ff4e08332e26b76954"
    );

    let gv003 = parse("{\"z\":\"\u{e9}\",\"a\":1}".as_bytes()).unwrap();
    assert_eq!(
        hj("assure/test-json/v1", &gv003).to_string(),
        "sha256:1a2aab8858a444002cd16e1fa53cc33fd12e5e6ac4568f85e06bef971a28425d"
    );

    assert_eq!(
        hb("assure/text-projection/v1", b"a\nb\n").to_string(),
        "sha256:bab154d44fb1340ee8c20af6a1e36b9a903a5e44c584f8ce524237f0289b88c6"
    );

    assert_eq!(
        hb("assure/raw-bytes/v1", b"").to_string(),
        "sha256:28031daa5fbb3a297dc947195957fe4a05c1bd2e58c56163013ee62be9368fac"
    );
}

#[test]
fn domain_separation_changes_the_digest() {
    assert_ne!(hb("amiss/a/v1", b"x"), hb("amiss/b/v1", b"x"));
    assert_ne!(hb("amiss/a/v1", b"x"), hb("amiss/a/v1", b"y"));
}

/// The identity grammar after the host opened: a host is any nonempty
/// slash-free claim up to the cap, an owner is one or more slash-joined
/// segments, and the github constructor keeps the strict single-segment
/// form the GitHub-bound v1 documents can spell.
#[test]
fn the_open_identity_grammar_admits_claims_and_keeps_structure() {
    use amiss_wire::model::{ForgeDialect, RepositoryIdentity};
    let new = |host: &str, owner: &str, name: &str| {
        RepositoryIdentity::new(host.to_owned(), owner.to_owned(), name.to_owned())
    };
    assert!(new("github.com", "acme", "widget").is_some());
    assert!(new("GitHub.com:8080", "acme", "widget").is_some());
    assert!(new("192.168.0.1", "acme", "widget").is_some());
    assert!(new(&"a".repeat(255), "acme", "widget").is_some());
    assert!(new("", "acme", "widget").is_none());
    assert!(new("git/hub.com", "acme", "widget").is_none());
    assert!(new(&"a".repeat(256), "acme", "widget").is_none());

    assert!(new("gitlab.com", "group/subgroup", "widget").is_some());
    assert!(new("gitlab.com", "group//sub", "widget").is_none());
    assert!(new("gitlab.com", "/group", "widget").is_none());
    assert!(new("gitlab.com", "group/", "widget").is_none());
    assert!(new("gitlab.com", "Group", "widget").is_none());
    assert!(new("gitlab.com", "group/-", "widget").is_none());
    let deep = ["a"; 128].join("/");
    assert_eq!(deep.len(), 255);
    assert!(new("gitlab.com", &deep, "widget").is_some());
    assert!(new("gitlab.com", &format!("{deep}/a"), "widget").is_none());

    let github = RepositoryIdentity::github("acme".to_owned(), "widget".to_owned());
    assert_eq!(
        github.as_ref().map(|identity| identity.host.as_str()),
        Some("github.com")
    );
    assert!(RepositoryIdentity::github("group/sub".to_owned(), "widget".to_owned()).is_none());

    assert_eq!(
        ForgeDialect::default_for_host("github.com"),
        Some(ForgeDialect::Github)
    );
    assert_eq!(ForgeDialect::default_for_host("ghes.corp.example"), None);
    assert_eq!(ForgeDialect::Github.as_str(), "github");
}
