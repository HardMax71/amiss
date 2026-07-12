use std::fs;
use std::path::Path;

use amiss_scan::{ScanLimits, ScanResources, scan_document};
use amiss_wire::json::{Value, parse};
use amiss_wire::model::Adapter;

fn field<'a>(members: &'a [(String, Value)], name: &str) -> Option<&'a Value> {
    members
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
}

#[expect(clippy::panic, reason = "test fixture helper")]
fn text(value: Option<&Value>) -> String {
    let Some(Value::String(text)) = value else {
        panic!("expected a string, found {value:?}")
    };
    text.clone()
}

#[expect(clippy::panic, clippy::unwrap_used, reason = "test fixture helper")]
fn integer(value: Option<&Value>) -> u64 {
    let Some(Value::Integer(number)) = value else {
        panic!("expected an integer, found {value:?}")
    };
    u64::try_from(*number).unwrap()
}

#[expect(clippy::panic, reason = "test fixture helper")]
fn span(value: Option<&Value>) -> (u64, u64) {
    let Some(Value::Array(pair)) = value else {
        panic!("expected a span, found {value:?}")
    };
    (integer(pair.first()), integer(pair.get(1)))
}

/// Replays every corpus case through the scan layer under the contract
/// ceilings and requires it to reproduce the checked-in goldens: same faults,
/// same work, and for every occurrence the same construct, destinations,
/// span, and owner. The corpus, not this crate, is the oracle.
#[test]
fn the_scan_layer_reproduces_the_corpus_goldens() {
    let manifest = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/parser-profile-corpus-v1.json"),
    )
    .unwrap();
    let Value::Object(root) = parse(&manifest).unwrap() else {
        panic!("the manifest is an object")
    };
    let Some(Value::Array(cases)) = field(&root, "cases") else {
        panic!("the manifest holds cases")
    };

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_silenced| {}));
    let mut compared = 0_usize;
    for case in cases {
        let Value::Object(members) = case else {
            panic!("a case is an object")
        };
        let case_id = text(field(members, "case_id"));
        let source = text(field(members, "source"));
        let Some(Value::Object(work)) = field(members, "work") else {
            panic!("{case_id} lacks work")
        };
        for (profile, adapter) in [
            ("commonmark-gfm-v1", Adapter::Markdown),
            ("mdx-source-v1", Adapter::Mdx),
        ] {
            let Some(Value::Object(golden)) = field(work, profile) else {
                continue;
            };
            let mut resources = ScanResources::new(ScanLimits::CONTRACT);
            let got = scan_document(&mut resources, adapter, source.as_bytes());
            compared = compared.saturating_add(1);

            if let Some(fault) = field(golden, "fault") {
                let Err(defect) = got else {
                    panic!("{case_id} {profile}: expected a fault")
                };
                assert_eq!(
                    text(Some(fault)),
                    defect.code().as_str(),
                    "{case_id} {profile}"
                );
                continue;
            }

            let scanned = got.unwrap_or_else(|defect| {
                panic!("{case_id} {profile}: unexpected failure {defect:?}")
            });
            assert_eq!(
                scanned.work.nodes,
                integer(field(golden, "nodes")),
                "{case_id} {profile} nodes"
            );
            assert_eq!(
                scanned.work.nesting,
                integer(field(golden, "nesting")),
                "{case_id} {profile} nesting"
            );
            let Some(Value::Array(expected)) = field(golden, "occurrences") else {
                panic!("{case_id} {profile} lacks occurrences")
            };
            assert_eq!(
                scanned.occurrences.len(),
                expected.len(),
                "{case_id} {profile} occurrence count"
            );
            for (ours, golden_row) in scanned.occurrences.iter().zip(expected) {
                let Value::Object(row) = golden_row else {
                    panic!("an occurrence is an object")
                };
                let entry = &ours.occurrence;
                assert_eq!(
                    entry.construct.as_str(),
                    text(field(row, "source_construct")),
                    "{case_id} {profile}"
                );
                assert_eq!(
                    entry.raw_destination,
                    text(field(row, "raw_destination")),
                    "{case_id} {profile}"
                );
                assert_eq!(
                    entry.semantic_destination,
                    text(field(row, "semantic_destination")),
                    "{case_id} {profile}"
                );
                let (start, end) = span(field(row, "span"));
                let ours_span = (
                    u64::try_from(entry.span.0).unwrap(),
                    u64::try_from(entry.span.1).unwrap(),
                );
                assert_eq!(ours_span, (start, end), "{case_id} {profile}");
                assert_eq!(
                    entry.block_kind.as_str(),
                    text(field(row, "block_kind")),
                    "{case_id} {profile}"
                );
            }
        }
    }
    std::panic::set_hook(previous);
    assert!(compared > 3000, "compared {compared} case-profile pairs");
}
