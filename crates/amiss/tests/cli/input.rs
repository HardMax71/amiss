use std::fs::{self, File};

use amiss_wire::report::MACHINE_JSON_BYTES;

use crate::support::amiss;

#[test]
fn report_consumers_keep_bounded_reads_and_strict_validation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("report with spaces.json");
    for (command, format) in [("render", "sarif"), ("external-plan", "json")] {
        let args = [
            command,
            "--report",
            path.to_str().unwrap(),
            "--format",
            format,
        ];
        let (code, stdout, stderr) = amiss(&args);
        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        assert!(stderr.contains(&format!("{} is unreadable", path.display())));

        File::create(&path)
            .unwrap()
            .set_len(MACHINE_JSON_BYTES + 1)
            .unwrap();
        let (code, stdout, stderr) = amiss(&args);
        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        assert!(stderr.contains("larger than a scanner report can be"));

        for malformed in [
            b"not json".as_slice(),
            br#"{"nested":{"duplicate":1,"duplicate":2}}"#,
            br#"{"nested":{"duplicate":1,"\u0064uplicate":2}}"#,
            b"{} trailing content",
            b"9007199254740992",
            b"\xff",
        ] {
            fs::write(&path, malformed).unwrap();
            let (code, stdout, stderr) = amiss(&args);
            assert_eq!(code, 2, "{command}: {malformed:?}");
            assert!(stdout.is_empty());
            assert!(stderr.contains("report"), "{stderr}");
        }
        fs::remove_file(&path).unwrap();
    }
}
