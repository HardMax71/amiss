#![expect(
    clippy::unwrap_used,
    reason = "tests build known-valid publication identities and inspect exact refusals"
)]

use std::{fs, path::Path};

use amiss_wire::de::ErrorKind;
use amiss_wire::digest::{Digest, hb, hj};
use amiss_wire::json;
use amiss_wire::model::{ArtifactId, ObjectFormat, Oid, RepositoryIdentity};
use amiss_wire::publication::{
    CompletedSite, DocsCandidate, PLAN_PAYLOAD_SCHEMA, PlanPayloadSchema, PublicationPlan,
    PublicationProducer, PublicationRelation, PublicationResource, PublicationTarget, parse_plan,
    plan,
};

mod assessment;
mod evidence;

fn digest(digit: char) -> Digest {
    Digest::from_wire(&format!("sha256:{}", digit.to_string().repeat(64))).unwrap()
}

fn oid(digit: char, format: ObjectFormat) -> Oid {
    let length = match format {
        ObjectFormat::Sha1 => 40,
        ObjectFormat::Sha256 => 64,
    };
    Oid::new(format, digit.to_string().repeat(length)).unwrap()
}

fn identity(value: &str) -> ArtifactId {
    ArtifactId::new(value.to_owned()).unwrap()
}

fn publication_plan() -> PublicationPlan {
    PublicationPlan {
        schema: PlanPayloadSchema::Current,
        report_payload_digest: digest('1'),
        docs: DocsCandidate {
            repository: RepositoryIdentity::github("acme".to_owned(), "widget".to_owned()).unwrap(),
            object_format: ObjectFormat::Sha1,
            commit: oid('a', ObjectFormat::Sha1),
            tree: oid('b', ObjectFormat::Sha1),
            candidate_identity_digest: digest('2'),
        },
        target: PublicationTarget {
            provider: identity("github-pages"),
            instance: identity("github.com"),
            environment: identity("github-pages"),
            channel: identity("stable"),
            canonical_url: "https://docs.example.com/widget/".to_owned(),
        },
        site: CompletedSite {
            artifact: PublicationResource {
                uri: "https://api.github.com/repos/acme/widget/actions/artifacts/123".to_owned(),
                digest: digest('3'),
            },
            input_digest: digest('4'),
        },
        product: PublicationResource {
            uri: "pkg:oci/registry.example.com/widget@1.2.3".to_owned(),
            digest: digest('5'),
        },
        producer: PublicationProducer {
            identity: identity("github-pages-deployment"),
            version: "1".to_owned(),
            context_digest: digest('6'),
        },
        relation: PublicationRelation {
            identity: identity("stable-docs-release"),
            context_digest: digest('7'),
        },
    }
}

#[test]
fn publication_plan_round_trips_with_its_payload_digest() {
    let expected = publication_plan();
    let value = plan(&expected).unwrap();
    let bytes = json::canonical(&value);
    let parsed = parse_plan(&bytes).unwrap();

    assert_eq!(parsed.payload, expected);
    assert_eq!(
        parsed.payload_digest,
        hj(PLAN_PAYLOAD_SCHEMA, value.member("payload").unwrap())
    );
    assert_eq!(json::canonical(&json::parse(&bytes).unwrap()), bytes);

    let example_bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/examples/publication-plan.json"),
    )
    .unwrap();
    let example = parse_plan(&example_bytes).unwrap();
    let written = plan(&example.payload).unwrap();
    assert_eq!(
        json::canonical(&written),
        json::canonical(&json::parse(&example_bytes).unwrap())
    );
}

#[test]
fn publication_plan_refuses_ambiguous_resources_and_git_objects() {
    let mut mismatched_git = publication_plan();
    mismatched_git.docs.tree = oid('b', ObjectFormat::Sha256);
    let error = plan(&mismatched_git).unwrap_err();
    assert_eq!(error.path, "$.payload.docs.tree_oid");
    assert_eq!(error.kind, ErrorKind::InvalidValue);

    let mut fragment = publication_plan();
    fragment.target.canonical_url = "https://docs.example.com/#candidate".to_owned();
    let error = plan(&fragment).unwrap_err();
    assert_eq!(error.path, "$.payload.target.canonical_url");
    assert_eq!(error.kind, ErrorKind::InvalidValue);

    for invalid in [
        "https://user@docs.example.com/",
        "https://docs.example.com:/",
        "https://docs.example.com:port/",
    ] {
        let mut invalid_authority = publication_plan();
        invalid_authority.target.canonical_url = invalid.to_owned();
        let error = plan(&invalid_authority).unwrap_err();
        assert_eq!(error.path, "$.payload.target.canonical_url");
        assert_eq!(error.kind, ErrorKind::InvalidValue);
    }

    let mut relative_resource = publication_plan();
    relative_resource.product.uri = "registry.example.com/widget:latest".to_owned();
    let error = plan(&relative_resource).unwrap_err();
    assert_eq!(error.path, "$.payload.product.uri");
    assert_eq!(error.kind, ErrorKind::InvalidValue);
}

#[test]
fn publication_plan_refuses_repository_values_that_bypass_construction() {
    let value = plan(&publication_plan()).unwrap();
    let mut document: serde_json::Value = serde_json::from_slice(&json::canonical(&value)).unwrap();
    document["payload"]["docs"]["repository"]["host"] = serde_json::json!("invalid/host");
    let payload = serde_json_canonicalizer::to_vec(&document["payload"]).unwrap();
    document["payload_digest"] = serde_json::json!(hb(PLAN_PAYLOAD_SCHEMA, &payload).to_string());

    let error = parse_plan(&serde_json_canonicalizer::to_vec(&document).unwrap()).unwrap_err();
    assert_eq!(error.path, "$.payload.docs.repository");
    assert_eq!(error.kind, ErrorKind::InvalidValue);
}

#[test]
fn publication_plan_reports_derived_shape_errors_at_their_fields() {
    let value = plan(&publication_plan()).unwrap();
    let bytes = json::canonical(&value);
    for (pointer, replacement, expected_path, expected_kind) in [
        (
            "/payload/report_payload_digest",
            serde_json::Value::String(format!("sha256:{}", "z".repeat(64))),
            "$.payload.report_payload_digest",
            ErrorKind::InvalidValue,
        ),
        (
            "/payload/report_payload_digest",
            serde_json::Value::Bool(false),
            "$.payload.report_payload_digest",
            ErrorKind::WrongType,
        ),
        (
            "/payload/docs/commit_oid",
            serde_json::Value::String("z".repeat(40)),
            "$.payload.docs.commit_oid",
            ErrorKind::InvalidValue,
        ),
        (
            "/payload/target/provider",
            serde_json::Value::String("invalid identity".to_owned()),
            "$.payload.target.provider",
            ErrorKind::InvalidValue,
        ),
        (
            "/payload/schema",
            serde_json::Value::String("unknown".to_owned()),
            "$.payload.schema",
            ErrorKind::InvalidValue,
        ),
    ] {
        let mut document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        *document.pointer_mut(pointer).unwrap() = replacement;
        let payload = serde_json_canonicalizer::to_vec(&document["payload"]).unwrap();
        document["payload_digest"] =
            serde_json::json!(hb(PLAN_PAYLOAD_SCHEMA, &payload).to_string());

        let error = parse_plan(&serde_json_canonicalizer::to_vec(&document).unwrap()).unwrap_err();
        assert_eq!(error.path, expected_path);
        assert_eq!(error.kind, expected_kind);
    }

    let mut document: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    document["payload"]
        .as_object_mut()
        .unwrap()
        .remove("schema");
    let payload = serde_json_canonicalizer::to_vec(&document["payload"]).unwrap();
    document["payload_digest"] = serde_json::json!(hb(PLAN_PAYLOAD_SCHEMA, &payload).to_string());
    let error = parse_plan(&serde_json_canonicalizer::to_vec(&document).unwrap()).unwrap_err();
    assert_eq!(error.path, "$.payload.schema");
    assert_eq!(error.kind, ErrorKind::MissingField);
}

#[test]
fn publication_plan_refuses_tampering_and_open_shapes() {
    let value = plan(&publication_plan()).unwrap();
    let bytes = json::canonical(&value);
    let parsed = parse_plan(&bytes).unwrap();
    let tampered = String::from_utf8(bytes)
        .unwrap()
        .replace(&parsed.payload_digest.to_string(), &digest('f').to_string());
    let error = parse_plan(tampered.as_bytes()).unwrap_err();
    assert_eq!(error.path, "$.payload_digest");
    assert_eq!(error.kind, ErrorKind::DigestMismatch);

    let value = plan(&publication_plan()).unwrap();
    let recorded = value.text("payload_digest").unwrap();
    let open = String::from_utf8(json::canonical(&value))
        .unwrap()
        .replacen(
            "\"report_payload_digest\":",
            "\"unknown\":true,\"report_payload_digest\":",
            1,
        );
    let open_value = json::parse(open.as_bytes()).unwrap();
    let rebound = open.replace(
        recorded,
        &hj(PLAN_PAYLOAD_SCHEMA, open_value.member("payload").unwrap()).to_string(),
    );
    let error = parse_plan(rebound.as_bytes()).unwrap_err();
    assert_eq!(error.path, "$.payload.unknown");
    assert_eq!(error.kind, ErrorKind::UnknownField);
}
