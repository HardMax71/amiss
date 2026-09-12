use amiss_controller_github::check::CheckRunApp;

const APP: &str = include_str!("../fixtures/github-app.json");

#[test]
fn captured_check_app_keeps_only_its_consumed_identity() {
    let app: CheckRunApp = serde_json::from_str(APP).unwrap();
    assert_eq!(app, CheckRunApp { id: 15368 });
    for metadata in [
        r#"{"id":15368,"owner":false,"permissions":[],"events":null,"installations_count":-1}"#,
        r#"{"id":15368,"owner":{"future":[]},"node_id":{},"extra":42}"#,
        "[15368]",
    ] {
        assert_eq!(serde_json::from_str::<CheckRunApp>(metadata).unwrap(), app);
    }
    assert!(serde_json::from_str::<CheckRunApp>(&format!("{APP} {{}}")).is_err());
}

#[test]
fn check_app_identity_is_required_unique_and_lossless() {
    amiss_fixtures::assert_json_rejections::<CheckRunApp>(
        APP,
        &[
            (r#""id":15368"#, r#""missing_id":15368"#),
            (r#""id":15368"#, r#""id":15368,"\u0069d":15368"#),
            (r#""id":15368"#, r#""id":9007199254740992"#),
            (r#""id":15368"#, r#""id":-1"#),
            (r#""id":15368"#, r#""id":1.5"#),
            (r#""id":15368"#, r#""id":true"#),
            (r#""id":15368"#, r#""id":"15368""#),
            (r#""id":15368"#, r#""id":null"#),
        ],
    );
    for id in [0, js_int::MAX_SAFE_UINT] {
        let app = CheckRunApp { id };
        assert_eq!(
            serde_json::from_slice::<CheckRunApp>(&serde_json::to_vec(&app).unwrap()).unwrap(),
            app
        );
    }
    assert!(
        serde_json::to_vec(&CheckRunApp {
            id: js_int::MAX_SAFE_UINT + 1
        })
        .is_err()
    );
}
