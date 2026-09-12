#![expect(
    clippy::unwrap_used,
    reason = "tests build known-valid publication identities and inspect exact refusals"
)]

use amiss_wire::de::ErrorKind;
use amiss_wire::digest::{Digest, hb};
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
    let envelope = plan(expected.clone()).unwrap();
    let mut bytes = Vec::new();
    amiss_wire::write_json(
        &envelope,
        &mut bytes,
        amiss_wire::publication::PUBLICATION_DOCUMENT_BYTES,
    )
    .unwrap();
    let parsed = parse_plan(&bytes).unwrap();

    assert_eq!(parsed, envelope);
    assert_eq!(parsed.payload, expected);
    assert_eq!(
        parsed.payload_digest,
        hb(
            PLAN_PAYLOAD_SCHEMA,
            &serde_json_canonicalizer::to_vec(&expected).unwrap()
        )
    );
    assert_eq!(serde_json_canonicalizer::to_vec(&parsed).unwrap(), bytes);
}

#[test]
fn publication_plan_refuses_ambiguous_resources_and_git_objects() {
    let mut mismatched_git = publication_plan();
    mismatched_git.docs.tree = oid('b', ObjectFormat::Sha256);
    let error = plan(mismatched_git).unwrap_err();
    assert_eq!(error.path, "$.payload.docs.tree_oid");
    assert!(matches!(error.kind, ErrorKind::InvalidValue));

    let mut fragment = publication_plan();
    fragment.target.canonical_url = "https://docs.example.com/#candidate".to_owned();
    let error = plan(fragment).unwrap_err();
    assert_eq!(error.path, "$.payload.target.canonical_url");
    assert!(matches!(error.kind, ErrorKind::InvalidValue));

    for invalid in [
        "https://user@docs.example.com/",
        "https://docs.example.com:/",
        "https://docs.example.com:port/",
    ] {
        let mut invalid_authority = publication_plan();
        invalid_authority.target.canonical_url = invalid.to_owned();
        let error = plan(invalid_authority).unwrap_err();
        assert_eq!(error.path, "$.payload.target.canonical_url");
        assert!(matches!(error.kind, ErrorKind::InvalidValue));
    }

    let mut relative_resource = publication_plan();
    relative_resource.product.uri = "registry.example.com/widget:latest".to_owned();
    let error = plan(relative_resource).unwrap_err();
    assert_eq!(error.path, "$.payload.product.uri");
    assert!(matches!(error.kind, ErrorKind::InvalidValue));
}

#[test]
fn publication_plan_refuses_repository_values_that_bypass_construction() {
    let mut document = plan(publication_plan()).unwrap();
    document.payload.docs.repository =
        serde_json::from_str(r#"{"host":"invalid/host","owner":"acme","name":"widget"}"#).unwrap();
    document.payload_digest = hb(
        PLAN_PAYLOAD_SCHEMA,
        &serde_json_canonicalizer::to_vec(&document.payload).unwrap(),
    );

    let error = parse_plan(&serde_json_canonicalizer::to_vec(&document).unwrap()).unwrap_err();
    assert_eq!(error.path, "$.payload.docs.repository");
    assert!(matches!(error.kind, ErrorKind::InvalidValue));
}

#[test]
fn publication_plan_reports_derived_shape_errors_at_their_fields() {
    let document = plan(publication_plan()).unwrap();
    let text = serde_json::to_string(&document).unwrap();
    for (field, original, replacement, expected_path) in [
        (
            "report_payload_digest",
            serde_json::to_string(&document.payload.report_payload_digest).unwrap(),
            serde_json::to_string(&format!("sha256:{}", "z".repeat(64))).unwrap(),
            "$.payload.report_payload_digest",
        ),
        (
            "report_payload_digest",
            serde_json::to_string(&document.payload.report_payload_digest).unwrap(),
            "false".to_owned(),
            "$.payload.report_payload_digest",
        ),
        (
            "commit_oid",
            serde_json::to_string(&document.payload.docs.commit).unwrap(),
            serde_json::to_string(&"z".repeat(40)).unwrap(),
            "$.payload.docs.commit_oid",
        ),
        (
            "provider",
            serde_json::to_string(&document.payload.target.provider).unwrap(),
            r#""invalid identity""#.to_owned(),
            "$.payload.target.provider",
        ),
        (
            "schema",
            serde_json::to_string(&document.payload.schema).unwrap(),
            r#""unknown""#.to_owned(),
            "$.payload.schema",
        ),
    ] {
        let changed = text.replacen(
            &format!("\"{field}\":{original}"),
            &format!("\"{field}\":{replacement}"),
            1,
        );
        assert_ne!(changed, text);
        let error = parse_plan(changed.as_bytes()).unwrap_err();
        assert_eq!(error.path, expected_path);
        assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));
    }

    let missing = text.replacen(
        &format!(
            "\"schema\":{},",
            serde_json::to_string(&document.payload.schema).unwrap()
        ),
        "",
        1,
    );
    assert_ne!(missing, text);
    let error = parse_plan(missing.as_bytes()).unwrap_err();
    assert_eq!(error.path, "$.payload");
    assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));
}

#[test]
fn publication_plan_refuses_tampering_and_open_shapes() {
    let mut document = plan(publication_plan()).unwrap();
    let text = serde_json::to_string(&document).unwrap();
    document.payload_digest = digest('f');
    let error = parse_plan(&serde_json_canonicalizer::to_vec(&document).unwrap()).unwrap_err();
    assert_eq!(error.path, "$.payload_digest");
    assert!(matches!(error.kind, ErrorKind::DigestMismatch));

    let open = text.replacen(
        "\"report_payload_digest\":",
        "\"unknown\":true,\"report_payload_digest\":",
        1,
    );
    assert_ne!(open, text);
    let error = parse_plan(open.as_bytes()).unwrap_err();
    assert_eq!(error.path, "$.payload.unknown");
    assert!(matches!(error.kind, ErrorKind::Deserialize(source) if source.is_data()));
}
