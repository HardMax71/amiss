#![cfg(test)]

use amiss_wire::report::MACHINE_JSON_BYTES;

use crate::file_ledger::FileLedgerError;

use super::report_length;

#[test]
fn report_references_keep_the_exact_bytes_and_digest_contract() {
    use super::ReportRef;

    let reference = ReportRef::new(b"ledger report").unwrap();
    let encoded = serde_json::to_string(&reference).unwrap();
    assert_eq!(
        encoded,
        r#"{"digest":"sha256:425eeb72fcb2c159fe7a0025da15ec0b6218d5c9130ffc81b8f602ffa4d1b177","length":13}"#
    );
    assert_eq!(
        serde_json::from_str::<ReportRef>(&encoded).unwrap(),
        reference
    );
    for mutation in [
        encoded.replace("sha256:", "sha256!"),
        encoded.replace("425e", "425E"),
        encoded.replacen('{', r#"{"unknown":false,"#, 1),
        encoded.replace(r#""length""#, r#""other""#),
    ] {
        assert_ne!(mutation, encoded);
        assert!(serde_json::from_str::<ReportRef>(&mutation).is_err());
    }
}

#[test]
fn report_lengths_include_the_ceiling_and_exclude_the_next_byte() {
    let ceiling = usize::try_from(MACHINE_JSON_BYTES).expect("the ceiling fits this host");
    let oversized = usize::try_from(MACHINE_JSON_BYTES.saturating_add(1))
        .expect("the first oversized length fits this host");

    assert!(matches!(
        report_length(ceiling),
        Ok(length) if length == MACHINE_JSON_BYTES
    ));
    assert!(matches!(
        report_length(oversized),
        Err(FileLedgerError::ReportTooLarge)
    ));
}
