use std::fs;
use std::path::Path;

use amiss_scan::{ScanLimits, ScanResources, scan_document};
use amiss_wire::model::Adapter;

#[derive(serde::Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}

#[derive(serde::Deserialize)]
struct Case {
    #[serde(rename = "case_id")]
    id: String,
    source: String,
    work: std::collections::BTreeMap<String, Work>,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Work {
    Fault {
        fault: String,
    },
    Complete {
        nodes: u64,
        nesting: u64,
        occurrences: Option<Vec<Occurrence>>,
    },
}

#[derive(serde::Deserialize)]
struct Occurrence {
    source_construct: String,
    raw_destination: String,
    semantic_destination: String,
    span: (u64, u64),
    block_kind: String,
}

/// Replays every corpus case through the scan layer under the contract
/// ceilings and requires it to reproduce the checked-in goldens: same faults,
/// same work, and for every occurrence the same construct, destinations,
/// span, and owner. The corpus, not this crate, is the oracle.
#[test]
fn the_scan_layer_reproduces_the_corpus_goldens() {
    let manifest = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/parser-profile-corpus.json"),
    )
    .unwrap();
    let corpus: Corpus = serde_json::from_slice(&manifest).unwrap();

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_silenced| {}));
    let mut compared = 0_usize;
    for Case {
        id: case_id,
        source,
        work,
    } in corpus.cases
    {
        for (profile, adapter) in [
            ("commonmark-gfm", Adapter::Markdown),
            ("mdx-source", Adapter::Mdx),
        ] {
            let Some(golden) = work.get(profile) else {
                continue;
            };
            let mut resources = ScanResources::new(ScanLimits::CONTRACT);
            let got = scan_document(&mut resources, adapter, source.as_bytes());
            compared = compared.saturating_add(1);

            if let Work::Fault { fault } = golden {
                let Err(defect) = got else {
                    panic!("{case_id} {profile}: expected a fault")
                };
                assert_eq!(fault, defect.code().as_ref(), "{case_id} {profile}");
                continue;
            }

            let Work::Complete {
                nodes,
                nesting,
                occurrences,
            } = golden
            else {
                panic!("{case_id} {profile}: expected parser work")
            };
            let expected = occurrences
                .as_ref()
                .expect("a parsing profile has occurrence goldens");
            let scanned = got.unwrap_or_else(|defect| {
                panic!("{case_id} {profile}: unexpected failure {defect:?}")
            });
            assert_eq!(scanned.work.nodes, *nodes, "{case_id} {profile} nodes");
            assert_eq!(
                scanned.work.nesting, *nesting,
                "{case_id} {profile} nesting"
            );
            assert_eq!(
                scanned.occurrences.len(),
                expected.len(),
                "{case_id} {profile} occurrence count"
            );
            for (ours, row) in scanned.occurrences.iter().zip(expected) {
                let entry = &ours.occurrence;
                assert_eq!(
                    entry.construct.as_ref(),
                    row.source_construct,
                    "{case_id} {profile}"
                );
                assert_eq!(
                    entry.raw_destination, row.raw_destination,
                    "{case_id} {profile}"
                );
                assert_eq!(
                    entry.semantic_destination, row.semantic_destination,
                    "{case_id} {profile}"
                );
                let (start, end) = row.span;
                let ours_span = (
                    u64::try_from(entry.span.0).unwrap(),
                    u64::try_from(entry.span.1).unwrap(),
                );
                assert_eq!(ours_span, (start, end), "{case_id} {profile}");
                assert_eq!(
                    entry.block_kind.as_ref(),
                    row.block_kind,
                    "{case_id} {profile}"
                );
            }
        }
    }
    std::panic::set_hook(previous);
    assert!(compared > 3000, "compared {compared} case-profile pairs");
}
