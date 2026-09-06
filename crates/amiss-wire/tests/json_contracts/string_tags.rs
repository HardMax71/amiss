use amiss_wire::requests::SnapshotRequest;

#[test]
fn snapshot_request_tags_require_their_published_string_shape() {
    let original: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../spec/examples/scanner-snapshot-request.json"
    ))
    .unwrap();
    for field in ["schema", "materialization"] {
        let mut value = original.clone();
        let tag = value[field].as_str().unwrap().to_owned();
        value[field] = serde_json::json!({tag: null});
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(SnapshotRequest::parse(&bytes).is_err(), "{field}: {value}");
    }
}
