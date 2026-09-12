#![cfg(test)]

use std::cell::Cell;
use std::io;

use amiss_wire::json;
use amiss_wire::model::RepoPath;

use super::write_array;

#[test]
fn streaming_arrays_keep_paths_and_open_values_canonical() {
    let paths = [
        RepoPath::from_bytes(b"docs/quoted\".md".to_vec()).unwrap(),
        RepoPath::from_bytes(vec![0xff]).unwrap(),
    ];
    let mut written = Vec::new();
    write_array(paths.iter(), &mut written).unwrap();
    assert_eq!(written, br#"["docs/quoted\".md",{"bytes_hex":"ff"}]"#);

    let values = [serde_json::json!({"\u{e000}": 1, "\u{10000}": {"line": "one\ntwo"}})];
    written.clear();
    write_array(values.iter().map(json::canonical_view), &mut written).unwrap();
    assert_eq!(
        written,
        "[{\"\u{10000}\":{\"line\":\"one\\ntwo\"},\"\u{e000}\":1}]".as_bytes()
    );

    written.clear();
    write_array(std::iter::empty::<bool>(), &mut written).unwrap();
    assert_eq!(written, b"[]");
}

#[test]
fn output_failure_stops_consuming_the_iterator() {
    let observed = Cell::new(0);
    let items = (0..10).inspect(|_item| observed.set(observed.get() + 1));
    let mut storage = [0_u8; 1];
    let mut output = io::Cursor::new(storage.as_mut_slice());
    let failure = write_array(items, &mut output).unwrap_err();
    assert_eq!(failure.kind(), io::ErrorKind::WriteZero);
    assert_eq!(observed.get(), 1);
    assert_eq!(storage, *b"[");
}
