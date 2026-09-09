use amiss_controller::RelationLimits;

#[test]
fn relation_limit_fields_preserve_exact_json_integers() {
    let make = |number| RelationLimits {
        acquisition_objects: number,
        acquisition_bytes: number,
        projection_records: number,
        projection_bytes: number,
    };
    for number in [0, 1, js_int::MAX_SAFE_UINT] {
        let limits = make(number);
        let encoded = serde_json::to_string(&limits).unwrap();
        assert_eq!(
            serde_json::from_str::<RelationLimits>(&encoded).unwrap(),
            limits
        );
    }
    let encoded = serde_json::to_string(&make(js_int::MAX_SAFE_UINT)).unwrap();
    let maximum = js_int::MAX_SAFE_UINT.to_string();
    for (offset, token) in encoded.match_indices(&maximum) {
        for invalid in [
            "9007199254740992",
            "18446744073709551615",
            "-1",
            "-0",
            "1.0",
            "1e0",
            "null",
            "true",
            r#""1""#,
        ] {
            let mut changed = encoded.clone();
            changed.replace_range(offset..offset + token.len(), invalid);
            assert!(
                serde_json::from_str::<RelationLimits>(&changed).is_err(),
                "{changed}"
            );
        }
    }
    for number in [js_int::MAX_SAFE_UINT + 1, u64::MAX] {
        assert!(serde_json::to_string(&make(number)).is_err());
    }
}
