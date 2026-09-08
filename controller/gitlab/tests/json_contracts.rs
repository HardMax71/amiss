use std::io::Cursor;

use amiss_controller::{ProviderError, decode_bounded_json};
use amiss_controller_gitlab::tree::{TreeEntry, TreeObject};
use amiss_wire::controls::GitMode;

#[test]
fn tree_responses_retain_every_field_from_the_live_contract() {
    let input = include_bytes!("fixtures/tree.json");
    let (rows, length): (Vec<TreeObject>, _) =
        decode_bounded_json(Cursor::new(input), None, input.len()).unwrap();
    assert_eq!(length, input.len());
    assert_eq!(
        rows,
        vec![TreeObject::Tree(TreeEntry {
            id: "2132d150328bd9334cc4e62a16a5d998a7e399b9".parse().unwrap(),
            name: "flat".to_owned(),
            path: "files/flat".to_owned(),
            mode: GitMode::Tree,
        })]
    );
    let encoded = serde_json::to_vec(&rows).unwrap();
    let (replayed, _): (Vec<TreeObject>, _) =
        decode_bounded_json(Cursor::new(&encoded), None, encoded.len()).unwrap();
    assert_eq!(rows, replayed);
    for (kind, mode) in [
        ("blob", GitMode::RegularFile),
        ("blob", GitMode::ExecutableFile),
        ("blob", GitMode::Symlink),
        ("commit", GitMode::Gitlink),
    ] {
        let input = std::str::from_utf8(input)
            .unwrap()
            .replace("\"tree\"", &format!("\"{kind}\""))
            .replace("040000", mode.as_ref());
        let (rows, _): (Vec<TreeObject>, _) =
            decode_bounded_json(Cursor::new(input.as_bytes()), None, input.len()).unwrap();
        assert_eq!(rows.len(), 1);
    }
}

#[test]
fn tree_responses_refuse_unknown_or_malformed_rows() {
    let input = std::str::from_utf8(include_bytes!("fixtures/tree.json")).unwrap();
    for (original, replacement) in [
        ("[{", "[{\"future\":true,"),
        (r#""type":"tree""#, r#""type":"unknown""#),
        (r#""type":"tree""#, r#""type":null"#),
        (r#""name":"flat""#, r#""name":false"#),
        (r#""path":"files/flat""#, r#""path":[]"#),
        ("040000", "040001"),
        (
            "2132d150328bd9334cc4e62a16a5d998a7e399b9",
            "not-an-object-id",
        ),
        (r#""name":"flat","#, ""),
        (r#""path":"files/flat","#, ""),
        (r#","mode":"040000""#, ""),
        (r#""type":"tree","#, ""),
        (r#""id":"2132d150328bd9334cc4e62a16a5d998a7e399b9","#, ""),
        (r#""name":"flat""#, r#""name":"flat","name":"flat""#),
        (r#""name":"flat""#, r#""name":"flat","n\u0061me":"flat""#),
    ] {
        let invalid = input.replace(original, replacement);
        assert_ne!(invalid, input);
        assert!(
            matches!(
                decode_bounded_json::<Vec<TreeObject>>(
                    Cursor::new(invalid.as_bytes()),
                    None,
                    invalid.len()
                ),
                Err(ProviderError::InvalidResponse)
            ),
            "{invalid}"
        );
    }
    for invalid in [
        "[null]",
        "[true]",
        "[1]",
        "[{}]",
        r#"[["2132d150328bd9334cc4e62a16a5d998a7e399b9","flat","tree","files/flat","040000"]]"#,
    ] {
        assert!(
            matches!(
                decode_bounded_json::<Vec<TreeObject>>(
                    Cursor::new(invalid.as_bytes()),
                    None,
                    invalid.len()
                ),
                Err(ProviderError::InvalidResponse)
            ),
            "{invalid}"
        );
    }
}
