use amiss_controller::ProviderRunAttempt;

#[test]
fn provider_attempts_validate_construction_and_serde_without_changing_json() {
    for raw in [
        1,
        2,
        4_294_967_296,
        9_007_199_254_740_990,
        9_007_199_254_740_991,
    ] {
        let attempt = ProviderRunAttempt::try_from(raw).unwrap();
        assert_eq!(*attempt, raw);
        let encoded = serde_json::to_string(&attempt).unwrap();
        assert_eq!(encoded, raw.to_string());
        assert_eq!(
            serde_json::from_str::<ProviderRunAttempt>(&encoded).unwrap(),
            attempt
        );
    }
    for raw in [0, 9_007_199_254_740_992, u64::MAX] {
        assert!(ProviderRunAttempt::try_from(raw).is_err(), "{raw}");
        assert!(
            serde_json::from_str::<ProviderRunAttempt>(&raw.to_string()).is_err(),
            "{raw}"
        );
    }
    for raw in [
        "-1",
        "1.0",
        "1e0",
        "\"1\"",
        "true",
        "null",
        "[]",
        "[1]",
        "{}",
        "{\"value\":1}",
    ] {
        assert!(
            serde_json::from_str::<ProviderRunAttempt>(raw).is_err(),
            "{raw}"
        );
    }
}
