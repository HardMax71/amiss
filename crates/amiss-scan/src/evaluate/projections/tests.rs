#![cfg(test)]

use amiss_wire::json;
use amiss_wire::report::model::{
    CountProjectionDifferenceKind, ProjectionDifference, RowsProjectionDifference,
    RowsProjectionDifferenceKind,
};

#[test]
fn projection_difference_fields_keep_their_canonical_bytes() {
    let cases = [
        (
            ProjectionDifference::Count {
                kind: CountProjectionDifferenceKind::Count,
                expected_count: 7,
                observed_count: None,
            },
            r#"{"expected_count":7,"kind":"count","observed_count":null}"#,
        ),
        (
            ProjectionDifference::Count {
                kind: CountProjectionDifferenceKind::Count,
                expected_count: 9_007_199_254_740_991,
                observed_count: Some(0),
            },
            r#"{"expected_count":9007199254740991,"kind":"count","observed_count":0}"#,
        ),
        (
            ProjectionDifference::Rows(Box::new(RowsProjectionDifference {
                kind: RowsProjectionDifferenceKind::Rows,
                ordering_only: false,
                expected_records: 7,
                observed_records: 8,
                missing_records: 3,
                extra_records: 4,
                missing_preview: vec!["a".to_owned(), "β".to_owned()],
                extra_preview: vec!["quoted-\"\n".to_owned()],
                missing_omitted: 1,
                extra_omitted: 3,
            })),
            r#"{"expected_records":7,"extra_omitted":3,"extra_preview":["quoted-\"\n"],"extra_records":4,"kind":"rows","missing_omitted":1,"missing_preview":["a","β"],"missing_records":3,"observed_records":8,"ordering_only":false}"#,
        ),
        (
            ProjectionDifference::Rows(Box::new(RowsProjectionDifference {
                kind: RowsProjectionDifferenceKind::Rows,
                ordering_only: true,
                expected_records: 2,
                observed_records: 2,
                missing_records: 0,
                extra_records: 0,
                missing_preview: Vec::new(),
                extra_preview: Vec::new(),
                missing_omitted: 0,
                extra_omitted: 0,
            })),
            r#"{"expected_records":2,"extra_omitted":0,"extra_preview":[],"extra_records":0,"kind":"rows","missing_omitted":0,"missing_preview":[],"missing_records":0,"observed_records":2,"ordering_only":true}"#,
        ),
    ];
    for (difference, expected) in cases {
        assert_eq!(
            serde_json::to_vec(&difference).unwrap(),
            expected.as_bytes()
        );
        assert_eq!(
            json::canonical(&super::difference_value(&difference)),
            expected.as_bytes()
        );
        let decoded: ProjectionDifference = serde_json::from_str(expected).unwrap();
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), expected.as_bytes());
        let mut additive: serde_json::Value = serde_json::from_str(expected).unwrap();
        for field in additive.as_object().unwrap().keys() {
            let mut missing = additive.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<ProjectionDifference>(missing).is_err(),
                "{field}"
            );
        }
        additive["future-field"] = true.into();
        let decoded: ProjectionDifference = serde_json::from_value(additive).unwrap();
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), expected.as_bytes());
    }
}
