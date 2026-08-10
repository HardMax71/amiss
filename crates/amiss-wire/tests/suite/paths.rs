use amiss_wire::json::Value;
use amiss_wire::model::{RepoPath, RepoPathText};

/// Every byte string over this alphabet up to length four: enough to cross
/// every rule boundary (separators, dots, NUL, backslash, a non-UTF-8 byte,
/// a multibyte lead) while staying exhaustively enumerable.
fn small_universe() -> Vec<Vec<u8>> {
    let alphabet: [u8; 7] = [b'a', b'/', b'.', 0xff, 0x00, b'\\', 0xc3];
    let mut out: Vec<Vec<u8>> = vec![Vec::new()];
    let mut layer: Vec<Vec<u8>> = vec![Vec::new()];
    for _ in 0..4 {
        let mut next = Vec::new();
        for stem in &layer {
            for byte in alphabet {
                let mut grown = stem.clone();
                grown.push(byte);
                next.push(grown);
            }
        }
        out.extend(next.iter().cloned());
        layer = next;
    }
    out
}

/// The byte grammar restated independently of the implementation.
fn oracle(raw: &[u8]) -> bool {
    if raw.is_empty() || raw.len() > 4096 || raw.contains(&0) || raw.contains(&b'\\') {
        return false;
    }
    !raw.split(|byte| *byte == b'/')
        .any(|segment| segment.is_empty() || segment == b"." || segment == b"..")
}

#[test]
fn acceptance_matches_the_grammar_and_both_constructors_agree() {
    for raw in small_universe() {
        let accepted = RepoPath::from_bytes(raw.clone()).is_some();
        assert_eq!(accepted, oracle(&raw), "{raw:?}");
        if let Ok(text) = String::from_utf8(raw.clone()) {
            assert_eq!(
                RepoPath::new(text.clone()).is_some(),
                accepted,
                "the String constructor is the byte constructor: {raw:?}"
            );
            assert_eq!(
                RepoPathText::new(text).is_some(),
                accepted,
                "the text-only type accepts exactly the UTF-8 slice of the grammar: {raw:?}"
            );
        }
    }
}

#[test]
fn ordering_is_raw_byte_ordering_across_both_forms() {
    let accepted: Vec<RepoPath> = small_universe()
        .into_iter()
        .filter_map(RepoPath::from_bytes)
        .collect();
    for a in &accepted {
        for b in &accepted {
            assert_eq!(
                a.cmp(b),
                a.as_bytes().cmp(b.as_bytes()),
                "{:?} vs {:?}",
                a.as_bytes(),
                b.as_bytes()
            );
            assert_eq!(
                a == b,
                a.as_bytes() == b.as_bytes(),
                "equality and ordering agree"
            );
        }
    }
}

#[test]
fn construction_classifies_and_the_forms_never_overlap() {
    for raw in small_universe() {
        let Some(path) = RepoPath::from_bytes(raw.clone()) else {
            continue;
        };
        assert_eq!(path.as_bytes(), raw.as_slice());
        match (std::str::from_utf8(&raw).is_ok(), path.as_str()) {
            (true, Some(text)) => assert_eq!(text.as_bytes(), raw.as_slice()),
            (false, None) => {}
            (utf8, held) => panic!("classification split: utf8={utf8}, text={held:?} for {raw:?}"),
        }
    }
}

#[test]
fn the_wire_form_is_the_string_or_the_hex_object() {
    let text = RepoPath::new("docs/guide.md".to_owned()).unwrap();
    assert_eq!(text.to_value(), Value::String("docs/guide.md".to_owned()));

    let bytes = RepoPath::from_bytes(b"docs/b\xff.md".to_vec()).unwrap();
    assert_eq!(
        bytes.to_value(),
        Value::Object(vec![(
            "bytes_hex".to_owned(),
            Value::String("646f63732f62ff2e6d64".to_owned()),
        )])
    );
}

#[test]
fn the_length_ceiling_binds_at_exactly_the_contract_figure() {
    assert!(RepoPath::from_bytes(vec![b'a'; 4096]).is_some());
    assert!(RepoPath::from_bytes(vec![b'a'; 4097]).is_none());
    let mut long_bytes = vec![0xff_u8; 4095];
    long_bytes.insert(0, b'a');
    assert!(RepoPath::from_bytes(long_bytes.clone()).is_some());
    long_bytes.push(0xff);
    assert!(RepoPath::from_bytes(long_bytes).is_none());
}
