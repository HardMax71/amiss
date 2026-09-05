use std::collections::BTreeSet;

use amiss_scan::evaluate::structural_facts;

use super::*;

#[test]
fn adoption_facts_match_evaluation_for_each_shape_and_multiplicity() -> Result<(), amiss_scan::Error>
{
    assert!(structural_facts(&[])?.is_empty());
    for raw in [
        b"docs/guide.md".to_vec(),
        "docs/quoted-\"β\n.md".as_bytes().to_vec(),
        b"docs/raw-\xff.md".to_vec(),
        [b"docs/".as_slice(), &vec![0xff; 4091]].concat(),
    ] {
        let path = RepoPath::from_bytes(raw).unwrap();
        let resolutions = structural_resolutions(&path);
        assert_eq!(resolutions.len(), 13);
        for resolution in resolutions {
            let mut fixture = spec("d.md", "t.md", resolution);
            fixture.intent.repository_path = Some(path.clone());
            let mut digests = BTreeSet::new();
            for multiplicity in [1_usize, 2, 3] {
                let observations: Vec<_> = (0..multiplicity)
                    .map(|index| {
                        fixture.node_path = vec![index];
                        observation(&fixture)
                    })
                    .collect();
                let reproduced = structural_facts(&observations)?;
                let findings: Vec<_> = evaluate(
                    &[],
                    &comparisons(Vec::new(), observations),
                    Profile::Observe,
                )?
                .into_iter()
                .filter(|finding| {
                    matches!(
                        finding.key_input.finding_kind,
                        FindingKind::ExplicitTargetMissing
                            | FindingKind::ExplicitTargetTypeMismatch,
                    )
                })
                .collect();
                assert_eq!(findings.len(), 1);
                assert_eq!(reproduced.len(), 1);
                let finding = &findings[0];
                let fact = finding.candidate_fact.as_ref().expect("reference fact");
                assert_eq!(
                    reproduced.get(&finding.finding_key),
                    Some(&(u64::try_from(multiplicity).unwrap(), fact.digest()))
                );
                digests.insert(fact.digest());
            }
            assert_eq!(digests.len(), 3, "multiplicity changes the bound fact");
        }
    }
    Ok(())
}

fn structural_resolutions(path: &RepoPath) -> Vec<Resolution> {
    let mut cases = vec![
        Resolution::Missing(Missing::LineFragmentOutOfRange { path: path.clone() }),
        Resolution::Missing(Missing::LabelNotDeclared),
        Resolution::TypeMismatch {
            target: Target::Tree { path: path.clone() },
        },
    ];
    for near in [None, Some(path.clone())] {
        for same_object_at in [None, Some(path.clone())] {
            cases.push(Resolution::Missing(Missing::PathNotFound {
                path: path.clone(),
                near: near.clone(),
                same_object_at,
            }));
        }
    }
    for near in [None, Some("near-\"β\n-anchor".to_owned())] {
        cases.push(Resolution::Missing(Missing::HeadingAnchorNotFound {
            path: path.clone(),
            near,
        }));
    }
    for mode in [BlobMode::Regular, BlobMode::Executable] {
        for content in [
            BlobContent::Available {
                raw_digest: hb("amiss/raw-evidence", b"raw"),
                projection_digest: hb("amiss/scanner-target-projection", b"projection"),
            },
            BlobContent::LfsPointer {
                raw_digest: hb("amiss/raw-evidence", b"pointer"),
            },
        ] {
            cases.push(Resolution::TypeMismatch {
                target: Target::Blob(BlobTarget {
                    path: path.clone(),
                    mode,
                    content,
                }),
            });
        }
    }
    cases
}
