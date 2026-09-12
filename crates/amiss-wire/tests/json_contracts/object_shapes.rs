#![cfg(test)]

use amiss_wire::{external, locale, publication, relation};
use serde_json::Value;
use sha2::Digest as _;

struct ObjectCase<'a> {
    name: &'static str,
    example: &'a [u8],
    accepts: fn(&[u8]) -> bool,
    typed: fn(&[u8]) -> bool,
    domain: Option<&'static str>,
    objects: &'static [(&'static str, &'static str)],
}

const CASES: &[ObjectCase<'static>] = &[
    ObjectCase {
        name: "publication-plan",
        example: include_bytes!("../../../../spec/examples/publication-plan.json"),
        accepts: |bytes| publication::parse_plan(bytes).is_ok(),
        typed: |bytes| {
            serde_json::from_slice::<publication::PublicationPlanEnvelope>(bytes).is_ok()
        },
        domain: Some(publication::PLAN_PAYLOAD_SCHEMA),
        objects: &[
            ("", "schema,payload,payload_digest"),
            (
                "/payload",
                "schema,report_payload_digest,docs,target,site,product,producer,relation",
            ),
            (
                "/payload/docs",
                "repository,object_format,commit_oid,tree_oid,candidate_identity_digest",
            ),
            (
                "/payload/target",
                "provider,instance,environment,channel,canonical_url",
            ),
            ("/payload/site", "artifact,input_digest"),
            ("/payload/site/artifact", "uri,digest"),
            ("/payload/product", "uri,digest"),
            ("/payload/producer", "identity,version,context_digest"),
            ("/payload/relation", "identity,context_digest"),
        ],
    },
    ObjectCase {
        name: "publication-evidence",
        example: include_bytes!("../../../../spec/examples/publication-evidence.json"),
        accepts: |bytes| publication::parse_evidence(bytes).is_ok(),
        typed: |bytes| {
            serde_json::from_slice::<publication::PublicationEvidenceEnvelope>(bytes).is_ok()
        },
        domain: Some(publication::EVIDENCE_PAYLOAD_SCHEMA),
        objects: &[
            ("", "schema,payload,payload_digest"),
            (
                "/payload",
                "schema,plan_payload_digest,producer,deployment,docs,target,site,product",
            ),
            (
                "/payload/deployment",
                "outcome,record,workflow,provider_run_attempt",
            ),
            ("/payload/deployment/record", "uri,digest"),
        ],
    },
    ObjectCase {
        name: "publication-assessment",
        example: include_bytes!("../../../../spec/examples/publication-assessment.json"),
        accepts: |bytes| publication::parse_assessment(bytes).is_ok(),
        typed: |bytes| {
            serde_json::from_slice::<publication::PublicationAssessmentEnvelope>(bytes).is_ok()
        },
        domain: Some(publication::ASSESSMENT_PAYLOAD_SCHEMA),
        objects: &[
            ("", "schema,payload,payload_digest"),
            ("/payload", "schema,engine,subject,verdict,reasons"),
            ("/payload/engine", "engine_version,engine_digest"),
            (
                "/payload/subject",
                "report_payload_digest,plan_payload_digest,evidence_payload_digest",
            ),
        ],
    },
    ObjectCase {
        name: "locale-coverage-plan",
        example: include_bytes!("../../../../spec/examples/locale-coverage-plan.json"),
        accepts: |bytes| locale::parse_plan(bytes).is_ok(),
        typed: |bytes| serde_json::from_slice::<locale::LocaleCoveragePlanEnvelope>(bytes).is_ok(),
        domain: Some(locale::PLAN_PAYLOAD_SCHEMA),
        objects: &[
            ("", "schema,payload,payload_digest"),
            (
                "/payload",
                "schema,report_payload_digest,docs,scope,product,producer,policy",
            ),
            (
                "/payload/scope",
                "site,source_locale,target_locale,channel,version",
            ),
            ("/payload/product", "uri,digest"),
            (
                "/payload/policy",
                "identity,context_digest,required,fallbacks,require_target_lineage",
            ),
            ("/payload/policy/required", "mode,keys"),
            ("/payload/policy/fallbacks/0", "class,pages"),
            ("/payload/policy/fallbacks/0/pages", "mode,keys"),
        ],
    },
    ObjectCase {
        name: "locale-coverage-evidence",
        example: include_bytes!("../../../../spec/examples/locale-coverage-evidence.json"),
        accepts: |bytes| locale::parse_evidence(bytes).is_ok(),
        typed: |bytes| {
            serde_json::from_slice::<locale::LocaleCoverageEvidenceEnvelope>(bytes).is_ok()
        },
        domain: Some(locale::EVIDENCE_PAYLOAD_SCHEMA),
        objects: &[
            ("", "schema,payload,payload_digest"),
            (
                "/payload",
                "schema,plan_payload_digest,docs,scope,producer,source,target",
            ),
            ("/payload/source", "input_digest,product,complete,pages"),
            ("/payload/source/pages/0", "key,resource_digest"),
            ("/payload/target", "input_digest,product,complete,pages"),
            ("/payload/target/pages/0", "key,resource_digest,origin"),
            (
                "/payload/target/pages/0/origin",
                "kind,based_on_source_digest",
            ),
            (
                "/payload/target/pages/1/origin",
                "kind,class,source_resource_digest",
            ),
        ],
    },
    ObjectCase {
        name: "locale-coverage-assessment",
        example: include_bytes!("../../../../spec/examples/locale-coverage-assessment.json"),
        accepts: |bytes| locale::parse_assessment(bytes).is_ok(),
        typed: |bytes| {
            serde_json::from_slice::<locale::LocaleCoverageAssessmentEnvelope>(bytes).is_ok()
        },
        domain: Some(locale::ASSESSMENT_PAYLOAD_SCHEMA),
        objects: &[
            ("", "schema,payload,payload_digest"),
            (
                "/payload",
                "schema,engine,subject,verdict,reasons,coverage,product",
            ),
            (
                "/payload/coverage",
                "complete,source_missing,target_missing,target_orphaned,fallbacks,lineage",
            ),
            ("/payload/coverage/fallbacks/0", "key,class,status"),
            ("/payload/coverage/lineage/0", "key,status"),
            ("/payload/product", "source,target"),
        ],
    },
    ObjectCase {
        name: "relation-plan",
        example: include_bytes!("../../../../spec/examples/relation-plan.json"),
        accepts: |bytes| relation::parse_plan(bytes).is_ok(),
        typed: |bytes| serde_json::from_slice::<relation::RelationPlanEnvelope>(bytes).is_ok(),
        domain: Some(relation::PLAN_PAYLOAD_SCHEMA),
        objects: &[
            ("", "schema,payload,payload_digest"),
            (
                "/payload",
                "schema,report_payload_digest,relation,coordination,trigger_role,projection,subjects",
            ),
            ("/payload/relation", "identity,context_digest"),
            (
                "/payload/subjects/0",
                "role,repository,target,object_format,source,base,candidate",
            ),
            ("/payload/subjects/0/base", "commit_oid,tree_oid"),
        ],
    },
    ObjectCase {
        name: "relation-evidence",
        example: include_bytes!("../../../../spec/examples/relation-evidence.json"),
        accepts: |bytes| relation::parse_evidence(bytes).is_ok(),
        typed: |bytes| serde_json::from_slice::<relation::RelationEvidenceEnvelope>(bytes).is_ok(),
        domain: Some(relation::EVIDENCE_PAYLOAD_SCHEMA),
        objects: &[
            ("", "schema,payload,payload_digest"),
            ("/payload", "schema,plan_payload_digest,subjects"),
            ("/payload/subjects/0", "role,base,candidate"),
            ("/payload/subjects/0/base", "value_digest,value_bytes"),
        ],
    },
    ObjectCase {
        name: "relation-assessment",
        example: include_bytes!("../../../../spec/examples/relation-assessment.json"),
        accepts: |bytes| relation::parse_assessment(bytes).is_ok(),
        typed: |bytes| {
            serde_json::from_slice::<relation::RelationAssessmentEnvelope>(bytes).is_ok()
        },
        domain: Some(relation::ASSESSMENT_PAYLOAD_SCHEMA),
        objects: &[
            ("", "schema,payload,payload_digest"),
            ("/payload", "schema,engine,subject,verdict,reason"),
            ("/payload/engine", "engine_version,engine_digest"),
            (
                "/payload/subject",
                "report_payload_digest,plan_payload_digest,evidence_payload_digest",
            ),
        ],
    },
    ObjectCase {
        name: "scanner-external-plan",
        example: include_bytes!("../../../../spec/examples/scanner-external-plan.json"),
        accepts: |bytes| external::parse_plan(bytes).is_ok(),
        typed: |bytes| serde_json::from_slice::<external::ExternalPlanEnvelope>(bytes).is_ok(),
        domain: Some(external::PLAN_PAYLOAD_SCHEMA),
        objects: &[
            ("", "schema,payload,payload_digest"),
            (
                "/payload",
                "schema,engine,report,introduced,removed,retained_count",
            ),
            ("/payload/engine", "engine_version,engine_digest"),
            ("/payload/report", "payload_digest,base,candidate,mode"),
            ("/payload/introduced/0", "destination,scheme,documents"),
        ],
    },
    ObjectCase {
        name: "scanner-external-evidence",
        example: include_bytes!("../../../../spec/examples/scanner-external-evidence.json"),
        accepts: |bytes| external::parse_evidence(bytes).is_ok(),
        typed: |bytes| serde_json::from_slice::<external::ExternalEvidence>(bytes).is_ok(),
        domain: None,
        objects: &[
            ("", "schema,plan_payload_digest,producer,rows"),
            ("/producer", "name,version"),
        ],
    },
    ObjectCase {
        name: "scanner-external-assessment",
        example: include_bytes!("../../../../spec/examples/scanner-external-assessment.json"),
        accepts: |bytes| external::parse_assessment(bytes).is_ok(),
        typed: |bytes| {
            serde_json::from_slice::<external::ExternalAssessmentEnvelope>(bytes).is_ok()
        },
        domain: Some(external::ASSESSMENT_PAYLOAD_SCHEMA),
        objects: &[
            ("", "schema,payload,payload_digest"),
            ("/payload", "schema,engine,subject,producer,verdicts"),
            ("/payload/engine", "engine_version,engine_digest"),
            (
                "/payload/subject",
                "report_payload_digest,plan_payload_digest,evidence_digest",
            ),
            ("/payload/producer", "name,version"),
            (
                "/payload/verdicts/0",
                "destination,documents,verdict,reason,retarget",
            ),
        ],
    },
];

#[test]
fn published_documents_require_objects_at_every_known_level() {
    for case in CASES {
        assert_objects(case);
    }
}

#[test]
fn an_optional_external_repository_keeps_its_object_shape() {
    let mut plan = external::parse_plan(include_bytes!(
        "../../../../spec/examples/scanner-external-plan.json"
    ))
    .expect("published external plan");
    plan.payload
        .introduced
        .first_mut()
        .expect("introduced destination")
        .repository = Some(external::ExternalRepository {
        host: "github.com".to_owned(),
        dialect: amiss_wire::model::ForgeDialect::Github,
        owner: "example".to_owned(),
        name: "manual".to_owned(),
        form: Some("blob".to_owned()),
        tail: Some("main/README.md".to_owned()),
    });
    plan.payload_digest = amiss_wire::model::Digest::from(
        sha2::Sha256::new_with_prefix(external::PLAN_PAYLOAD_SCHEMA)
            .chain_update([0_u8])
            .chain_update(serde_json_canonicalizer::to_vec(&plan.payload).expect("extended plan"))
            .finalize()
            .0,
    );
    let bytes = serde_json_canonicalizer::to_vec(&plan).expect("extended plan");
    assert_objects(&ObjectCase {
        name: "external repository",
        example: &bytes,
        accepts: |bytes| external::parse_plan(bytes).is_ok(),
        typed: |bytes| serde_json::from_slice::<external::ExternalPlanEnvelope>(bytes).is_ok(),
        domain: Some(external::PLAN_PAYLOAD_SCHEMA),
        objects: &[(
            "/payload/introduced/0/repository",
            "host,dialect,owner,name,form,tail",
        )],
    });
}

fn assert_objects(case: &ObjectCase<'_>) {
    let ObjectCase {
        name,
        example,
        accepts,
        typed,
        domain,
        objects,
    } = case;
    assert!(accepts(example), "valid {name}");
    assert!(typed(example));
    let original: Value = serde_json::from_slice(example).expect("published object fixture");
    let mut admitted = Vec::new();
    for &(path, fields) in *objects {
        let mut changed = original.clone();
        let object = changed.pointer_mut(path).expect("published object fixture");
        let members = object.as_object_mut().expect("published object fixture");
        let sequence = fields
            .split(',')
            .map(|field| members.remove(field).expect("published object fixture"))
            .collect();
        assert!(members.is_empty(), "unlisted fields at {path}: {members:?}");
        *object = Value::Array(sequence);
        let encoded = serde_json_canonicalizer::to_vec(&changed).expect("published object fixture");
        if typed(&encoded) {
            admitted.push(format!("typed {path}"));
        }
        if accepts(&encoded) {
            admitted.push(format!("original digest {path}"));
        }
        if let Some(domain) = domain
            && !path.is_empty()
        {
            let digest = amiss_wire::model::Digest::from(
                sha2::Sha256::new_with_prefix(domain)
                    .chain_update([0_u8])
                    .chain_update(
                        serde_json_canonicalizer::to_vec(
                            changed.get("payload").expect("envelope payload"),
                        )
                        .expect("published object fixture"),
                    )
                    .finalize()
                    .0,
            );
            *changed.get_mut("payload_digest").expect("envelope digest") =
                serde_json::to_value(digest).expect("published object fixture");
            if accepts(
                &serde_json_canonicalizer::to_vec(&changed).expect("published object fixture"),
            ) {
                admitted.push(format!("rehashed {path}"));
            }
        }
    }
    assert!(
        admitted.is_empty(),
        "{name} objects accepted as arrays: {admitted:?}"
    );
}
