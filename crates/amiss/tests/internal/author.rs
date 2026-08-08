#![cfg(test)]

use crate::invocation::AuthorInvocation;

use super::round_trips;

fn author(path: &str, line: u64, name: &str) -> AuthorInvocation {
    AuthorInvocation {
        repo: std::path::PathBuf::from("."),
        path: path.to_owned(),
        line,
        name: name.to_owned(),
    }
}

/// Each binding clause of the round trip stands alone: the printed bytes
/// must name exactly the claim the flags asked for, so a definition naming
/// any other name, line, path, or text is refused.
#[test]
fn every_round_trip_clause_refuses_alone() {
    let definition = r#"[amiss:right]: <amiss:value?path=a.txt&line=L2> "text""#;
    assert!(round_trips(
        definition,
        &author("a.txt", 2, "right"),
        "text"
    ));
    assert!(!round_trips(
        definition,
        &author("a.txt", 2, "wrong"),
        "text"
    ));
    assert!(!round_trips(
        definition,
        &author("a.txt", 3, "right"),
        "text"
    ));
    assert!(!round_trips(
        definition,
        &author("b.txt", 2, "right"),
        "text"
    ));
    assert!(!round_trips(
        definition,
        &author("a.txt", 2, "right"),
        "other"
    ));
}
