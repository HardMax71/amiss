#![cfg(test)]

use super::{StoredProviderRun, StoredRun};
use amiss_wire::model::{ForgeDialect, ObjectFormat, Oid};
use strum::IntoEnumIterator;

const PROVIDER_RUN: &str = concat!(
    r#"{"run_id":"run/11","attempt":1,"object_format":"sha1","#,
    r#""candidate_commit":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}"#,
);

const RUN: &str = concat!(
    r#"{"change":{"provider":{"namespace":"gitea","instance":"forge.example.test"},"#,
    r#""repository":{"host":"forge.example.test","owner":"owner","name":"amiss"},"#,
    r#""change":"42"},"refs":{"forge":"gitea","candidate":"refs/heads/topic","#,
    r#""target":"refs/heads/main","default_branch":"refs/heads/main"},"object_format":"sha1","#,
    r#""commits":{"base":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","#,
    r#""candidate":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},"#,
    r#""trees":{"base":"cccccccccccccccccccccccccccccccccccccccc","#,
    r#""candidate":"dddddddddddddddddddddddddddddddddddddddd"}}"#,
);

#[test]
fn stored_ledger_identity_preimages_are_stable() {
    use crate::file_ledger::format::{StoredPublication, delivery_key, staged_digest};
    use crate::{ControllerEvaluationId, DeliveryId, DeliveryIdentity, IntegrationId};

    let evaluation = ControllerEvaluationId::try_from("eval/ledger".to_owned()).unwrap();
    let raw = format!(
        r#"{{"provider_run":{PROVIDER_RUN},"evaluation_id":"eval/ledger","check":{{"plan_digest":"sha256:{plan}","required_status_name":"amiss/enforce","execution_constraint_digest":"sha256:{constraint}"}},"run":{RUN},"gate_commit":"{gate}","conclusion":{{"conclusion":"pass"}},"report":{{"report":"absent"}}}}"#,
        plan = "a".repeat(64),
        constraint = "b".repeat(64),
        gate = "b".repeat(40),
    );
    let stored: StoredPublication = serde_json::from_str(&raw).unwrap();
    for replacement in ["\"\"", "\"eval invalid\"", "null", "[]"] {
        let mutation = raw.replace("\"eval/ledger\"", replacement);
        assert_ne!(mutation, raw);
        assert!(serde_json::from_str::<StoredPublication>(&mutation).is_err());
    }
    let publication = stored.materialize(None).unwrap();
    let identity = DeliveryIdentity {
        provider: publication.run.change.provider,
        integration: IntegrationId::try_from("integration/7".to_owned()).unwrap(),
        delivery: DeliveryId::try_from("delivery/42".to_owned()).unwrap(),
    };
    assert_eq!(
        delivery_key(&identity).unwrap(),
        "a1e0fabbd2934452219e1a66ba8ae933791fe55bbfd29c6379aa5ae2a5e89ae9"
    );
    assert_eq!(
        staged_digest(&evaluation, 1, &stored).unwrap().to_string(),
        "sha256:1321a7f0f747db2639f365b3aa9a97a14a564748a289d8b602be427da96a339e"
    );
}

#[test]
fn stored_run_fields_preserve_the_existing_byte_layout() {
    let provider_run: StoredProviderRun = serde_json::from_str(PROVIDER_RUN).unwrap();
    let run: StoredRun = serde_json::from_str(RUN).unwrap();
    provider_run.materialize().unwrap();
    assert_eq!(serde_json::to_string(&provider_run).unwrap(), PROVIDER_RUN);
    assert_eq!(serde_json::to_string(&run).unwrap(), RUN);
}

#[test]
fn stored_provider_attempts_are_checked_before_materialization() {
    for raw in [
        "0",
        "9007199254740992",
        "18446744073709551615",
        "-1",
        "1.0",
        "\"1\"",
        "null",
    ] {
        let mutation = PROVIDER_RUN.replace("\"attempt\":1", &format!("\"attempt\":{raw}"));
        assert_ne!(mutation, PROVIDER_RUN);
        assert!(
            serde_json::from_str::<StoredProviderRun>(&mutation).is_err(),
            "{raw}"
        );
    }
    let maximum = PROVIDER_RUN.replace("\"attempt\":1", "\"attempt\":9007199254740991");
    let stored: StoredProviderRun = serde_json::from_str(&maximum).unwrap();
    let restored = stored.materialize().unwrap();
    assert_eq!(*restored.attempt, 9_007_199_254_740_991);
    assert_eq!(serde_json::to_string(&stored).unwrap(), maximum);
}

#[test]
fn stored_opaque_identities_are_checked_before_materialization() {
    for raw in [
        "\"\"",
        "\"has space\"",
        "\"bad%id\"",
        "\"a\\n\"",
        "null",
        "42",
    ] {
        let mutation = PROVIDER_RUN.replace("\"run/11\"", raw);
        assert!(
            serde_json::from_str::<StoredProviderRun>(&mutation).is_err(),
            "{mutation}"
        );
        for original in ["\"forge.example.test\"", "\"42\""] {
            let mutation = RUN.replacen(original, raw, 1);
            assert_ne!(mutation, RUN);
            assert!(
                serde_json::from_str::<StoredRun>(&mutation).is_err(),
                "{mutation}"
            );
        }
    }
    for (length, accepted) in [(256, true), (257, false)] {
        let spelling = "A".repeat(length);
        let mutation = PROVIDER_RUN.replace("run/11", &spelling);
        let parsed = serde_json::from_str::<StoredProviderRun>(&mutation);
        assert_eq!(parsed.is_ok(), accepted);
        if accepted {
            let stored = parsed.unwrap();
            assert_eq!(stored.materialize().unwrap().run_id.as_str(), spelling);
            assert_eq!(serde_json::to_string(&stored).unwrap(), mutation);
        }
    }
}

#[test]
fn stored_namespaces_are_checked_before_materialization() {
    for raw in ["", ".gitea", "gitea/forge", "Gitea", "gitea\n", "gitea💡"] {
        let encoded = serde_json::to_string(raw).unwrap();
        let mutation = RUN.replace(
            "\"namespace\":\"gitea\"",
            &format!("\"namespace\":{encoded}"),
        );
        assert_ne!(mutation, RUN);
        assert!(
            serde_json::from_str::<StoredRun>(&mutation).is_err(),
            "{raw:?}"
        );
    }
    for (length, accepted) in [(64, true), (65, false)] {
        let spelling = "a".repeat(length);
        let mutation = RUN.replace(
            "\"namespace\":\"gitea\"",
            &format!("\"namespace\":\"{spelling}\""),
        );
        let stored = serde_json::from_str::<StoredRun>(&mutation);
        assert_eq!(stored.is_ok(), accepted);
        if accepted {
            let stored = stored.unwrap();
            let change = stored.change.materialize().unwrap();
            assert_eq!(change.provider.namespace.as_str(), spelling);
            assert_eq!(serde_json::to_string(&stored).unwrap(), mutation);
        }
    }
}

#[test]
fn stored_runs_preserve_sha256_ids_for_every_forge() {
    let mut provider_run: StoredProviderRun = serde_json::from_str(PROVIDER_RUN).unwrap();
    let mut run: StoredRun = serde_json::from_str(RUN).unwrap();
    provider_run.object_format = ObjectFormat::Sha256;
    provider_run.candidate_commit = Oid::new(ObjectFormat::Sha256, "b".repeat(64)).unwrap();
    run.object_format = ObjectFormat::Sha256;
    for (oid, digit) in [
        (&mut run.commits.base, 'a'),
        (&mut run.commits.candidate, 'b'),
        (&mut run.trees.base, 'c'),
        (&mut run.trees.candidate, 'd'),
    ] {
        *oid = Oid::new(ObjectFormat::Sha256, digit.to_string().repeat(64)).unwrap();
    }
    for forge in ForgeDialect::iter() {
        run.refs.forge = forge;
        let bytes = serde_json::to_vec(&run).unwrap();
        let stored: StoredRun = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(stored, run);
    }
    let bytes = serde_json::to_vec(&provider_run).unwrap();
    let stored: StoredProviderRun = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(stored, provider_run);
    assert_eq!(
        stored.materialize().unwrap(),
        provider_run.materialize().unwrap()
    );
}

#[test]
fn stored_run_fields_reject_invalid_and_mixed_format_ids() {
    for original in ['a', 'b', 'c', 'd'] {
        for replacement in ["A".repeat(40), "a".repeat(39)] {
            let mutation = RUN.replace(&original.to_string().repeat(40), &replacement);
            assert_ne!(mutation, RUN);
            assert!(
                serde_json::from_str::<StoredRun>(&mutation).is_err(),
                "{original}: {replacement}"
            );
        }
    }
    for replacement in ["e".repeat(64), "B".repeat(40), "b".repeat(39)] {
        let mutation = PROVIDER_RUN.replace(&"b".repeat(40), &replacement);
        assert!(
            serde_json::from_str::<StoredProviderRun>(&mutation)
                .ok()
                .and_then(|run| run.materialize().ok())
                .is_none(),
            "{replacement}"
        );
    }
}

#[test]
fn stored_run_fields_reject_bad_refs_and_format_declarations() {
    for (before, after) in [
        ("refs/heads/topic", "refs/heads/topic..bad"),
        ("refs/heads/main", "refs/heads/.hidden"),
        (r#""sha1""#, r#""unknown""#),
        (r#""gitea""#, r#""unknown""#),
    ] {
        let mutation = RUN.replace(before, after);
        assert_ne!(mutation, RUN);
        assert!(
            serde_json::from_str::<StoredRun>(&mutation).is_err(),
            "{before}: {after}"
        );
    }
}
