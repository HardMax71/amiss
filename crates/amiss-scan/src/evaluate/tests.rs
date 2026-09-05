#![cfg(test)]

use std::collections::BTreeSet;

use amiss_wire::digest::hb;
use amiss_wire::json;
use amiss_wire::model::{ObjectFormat, Oid, RepoPath};
use amiss_wire::report::model as report;
use amiss_wire::resolution::{
    BlobContent, BlobMode, BlobTarget, DeclaredUntracked, ExternalReference, InvalidReference,
    Missing, Resolution, ResolutionTag, Target, UnsupportedSemantics, UnsupportedTarget,
    VersionScope,
};
use strum::{IntoDiscriminant, IntoEnumIterator};

#[test]
fn source_groups_preserve_sorted_digests_and_exact_multiplicities() {
    let first =
        amiss_wire::digest::Digest::from_wire(&format!("sha256:{}", "1".repeat(64))).unwrap();
    let second =
        amiss_wire::digest::Digest::from_wire(&format!("sha256:{}", "a".repeat(64))).unwrap();
    for first_count in 0..=3 {
        for second_count in 0..=3 {
            let mut input: Vec<_> = std::iter::repeat_n(first, first_count)
                .chain(std::iter::repeat_n(second, second_count))
                .collect();
            let expected = [(first, first_count), (second, second_count)]
                .into_iter()
                .filter(|(_, count)| *count != 0)
                .map(|(digest, count)| format!(r#"{{"digest":"{digest}","multiplicity":{count}}}"#))
                .collect::<Vec<_>>()
                .join(",");
            let expected = format!("[{expected}]").into_bytes();
            for reverse in [false, true] {
                if reverse {
                    input.reverse();
                }
                for _ in 0..input.len().max(1) {
                    let sources = super::source_multiplicities(input.iter().copied());
                    assert_eq!(serde_json::to_vec(&sources).unwrap(), expected);
                    assert_eq!(
                        json::canonical(&super::claims::sources_value(&sources)),
                        expected
                    );
                    if !input.is_empty() {
                        input.rotate_left(1);
                    }
                }
            }
        }
    }
}

#[test]
fn derived_resolutions_match_produced_bytes_and_the_report_reader() -> Result<(), serde_json::Error>
{
    for raw in [
        b"docs/guide.md".to_vec(),
        "docs/quoted-\"β\n.md".as_bytes().to_vec(),
        b"docs/raw-\xff.md".to_vec(),
        [b"docs/".as_slice(), &vec![0xff; 4091]].concat(),
    ] {
        let path = RepoPath::from_bytes(raw).unwrap();
        let mut cases = resolution_cases(&path);
        for (format, width) in [(ObjectFormat::Sha1, 40), (ObjectFormat::Sha256, 64)] {
            cases.push(Resolution::UnsupportedVersion {
                scope: VersionScope::KnownCommit {
                    commit_oid: Oid::new(format, "a".repeat(width)).unwrap(),
                    path: path.clone(),
                },
            });
        }
        let tags: BTreeSet<_> = cases
            .iter()
            .map(|case| case.discriminant().as_ref().to_owned())
            .collect();
        assert_eq!(
            tags,
            ResolutionTag::iter()
                .map(|tag| tag.as_ref().to_owned())
                .collect()
        );
        assert_eq!(cases.len(), 56);
        for resolution in cases {
            let encoded = serde_json_canonicalizer::to_vec(&resolution)?;
            assert_eq!(
                encoded,
                json::canonical(&super::resolution_row(&resolution)),
                "{resolution:?}"
            );
            let decoded: report::Resolution = serde_json::from_slice(&encoded)?;
            assert_eq!(serde_json_canonicalizer::to_vec(&decoded)?, encoded);
        }
    }
    Ok(())
}

fn resolution_cases(path: &RepoPath) -> Vec<Resolution<RepoPath>> {
    let mut targets = vec![Target::Tree { path: path.clone() }];
    let mut cases = Vec::new();
    for mode in [BlobMode::Regular, BlobMode::Executable] {
        for content in [
            BlobContent::Available {
                raw_digest: hb("amiss/raw-evidence", b"raw"),
                projection_digest: hb("amiss/scanner-source-projection", b"projection"),
            },
            BlobContent::LfsPointer {
                raw_digest: hb("amiss/raw-evidence", b"pointer"),
            },
        ] {
            let blob = BlobTarget {
                path: path.clone(),
                mode,
                content,
            };
            cases.push(Resolution::UnsupportedSemantics(
                UnsupportedSemantics::Fragment(blob.clone()),
            ));
            targets.push(Target::Blob(blob));
        }
    }
    for target in targets {
        cases.extend([
            Resolution::Resolved {
                target: target.clone(),
            },
            Resolution::TypeMismatch {
                target: target.clone(),
            },
            Resolution::UnsupportedSemantics(UnsupportedSemantics::Query(target.clone())),
            Resolution::UnsupportedSemantics(UnsupportedSemantics::CodeFragment(target)),
        ]);
    }
    cases.extend([
        Resolution::Missing(Missing::LineFragmentOutOfRange { path: path.clone() }),
        Resolution::Missing(Missing::LabelNotDeclared),
        Resolution::DeclaredUntracked(DeclaredUntracked {
            path: path.clone(),
            declared_by: RepoPath::new(".gitignore".to_owned()).unwrap_or_else(|| panic!()),
        }),
        Resolution::UnsupportedTarget(UnsupportedTarget::Symlink { path: path.clone() }),
        Resolution::UnsupportedTarget(UnsupportedTarget::Gitlink { path: path.clone() }),
        Resolution::UnsupportedVersion {
            scope: VersionScope::KnownPath { path: path.clone() },
        },
        Resolution::UnsupportedVersion {
            scope: VersionScope::UnknownPath,
        },
    ]);
    for near in [None, Some(path.clone())] {
        for same_object_at in [None, Some(path.clone())] {
            cases.push(Resolution::Missing(Missing::PathNotFound {
                path: path.clone(),
                near: near.clone(),
                same_object_at,
            }));
        }
    }
    for near in [None, Some("quoted-\"β\n-anchor".to_owned())] {
        cases.push(Resolution::Missing(Missing::HeadingAnchorNotFound {
            path: path.clone(),
            near,
        }));
    }
    cases.extend(
        [
            UnsupportedSemantics::SiteRoute,
            UnsupportedSemantics::NetworkPath,
            UnsupportedSemantics::AttributeDependent,
            UnsupportedSemantics::DuplicateLabel,
            UnsupportedSemantics::ExternalInventory,
        ]
        .map(Resolution::UnsupportedSemantics),
    );
    cases.extend(InvalidReference::iter().map(|reason| Resolution::Invalid { reason }));
    cases.extend(ExternalReference::iter().map(|reason| Resolution::External { reason }));
    cases
}
