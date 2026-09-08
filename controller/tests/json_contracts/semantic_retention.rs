use amiss_controller::BoundSemanticEvidence;
use amiss_wire::{
    digest::{hb, sha256},
    requests::SuppliedSemanticEvidence,
    semantic::{self, SemanticEvidenceTemplate, SemanticProducer},
};

pub(super) fn defects(
    original: &BoundSemanticEvidence,
) -> Result<Vec<(&'static str, BoundSemanticEvidence)>, Box<dyn std::error::Error>> {
    let supplied = original
        .supplied
        .first()
        .ok_or("missing fixture envelope")?;
    let mut cases = [
        ("missing_model", Vec::new()),
        ("extra_model", vec![supplied.clone(); 2]),
        (
            "wrong_context",
            vec![SuppliedSemanticEvidence {
                expected_context_digest: hb("test", b"other context"),
                ..supplied.clone()
            }],
        ),
    ]
    .into_iter()
    .map(|(name, supplied)| {
        (
            name,
            BoundSemanticEvidence {
                supplied,
                ..original.clone()
            },
        )
    })
    .collect::<Vec<_>>();
    cases.push((
        "missing_artifact",
        BoundSemanticEvidence {
            artifact: None,
            ..original.clone()
        },
    ));
    let mut noncanonical = original.clone();
    let row = noncanonical
        .artifact
        .as_mut()
        .and_then(|artifact| artifact.inputs.first_mut())
        .ok_or("missing fixture artifact")?;
    let mut bytes = row.envelope_bytes.to_vec();
    bytes.push(b'\n');
    row.envelope_digest = sha256(&bytes);
    row.envelope_bytes = bytes.into();
    cases.push(("noncanonical_envelope", noncanonical));

    let row = original
        .artifact
        .as_ref()
        .and_then(|artifact| artifact.inputs.first())
        .ok_or("missing fixture artifact")?;
    let template = semantic::parse_template(&row.template_bytes)?;
    let observations = semantic::parse_template(include_bytes!(
        "../../../spec/examples/scanner-semantic-template.json"
    ))?
    .observations;
    assert!(!observations.is_empty());
    for (name, changed) in [
        (
            "wrong_producer",
            SemanticEvidenceTemplate {
                producer: SemanticProducer {
                    version: "2".to_owned(),
                    ..template.producer.clone()
                },
                ..template.clone()
            },
        ),
        (
            "wrong_completeness",
            SemanticEvidenceTemplate {
                complete: !template.complete,
                ..template.clone()
            },
        ),
        (
            "wrong_observations",
            SemanticEvidenceTemplate {
                observations,
                ..template
            },
        ),
    ] {
        let mut bound = original.clone();
        let row = bound
            .artifact
            .as_mut()
            .and_then(|artifact| artifact.inputs.first_mut())
            .ok_or("missing fixture artifact")?;
        let bytes = semantic::template(changed)?;
        row.template_digest = sha256(&bytes);
        row.template_bytes = bytes.into();
        cases.push((name, bound));
    }
    Ok(cases)
}
