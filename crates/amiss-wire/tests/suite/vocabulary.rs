use amiss_wire::controls::{
    BLOB_LINES_SOURCE, BlobLineSelection, NAMED_REGION_SOURCE, NamedRegionSelection,
    ProjectionKind, ProjectionSource, RECORD_SET_SOURCE, RECORD_VALUE_SOURCE, RecordSetSelection,
    RecordValueSelection, TREE_PATHS_SOURCE, TreePathSelection,
};
use amiss_wire::model::{ArtifactId, RepoPathText};
use serde_json::Value;
use strum::IntoEnumIterator;

fn spellings_agree<T: IntoEnumIterator + AsRef<str> + serde::Serialize>() {
    for variant in T::iter() {
        assert_eq!(
            serde_json::to_value(&variant).ok(),
            Some(Value::String(variant.as_ref().to_owned()))
        );
    }
}

#[test]
fn serde_and_strum_spell_every_vocabulary_alike() {
    spellings_agree::<ProjectionKind>();
}

#[test]
fn projection_source_tags_spell_their_constants() {
    let path = |raw: &str| RepoPathText::new(raw.to_owned());
    let set = ArtifactId::new("rust/public-api".to_owned());
    let sources = [
        (
            BLOB_LINES_SOURCE,
            path("src/lib.rs").map(|path| {
                ProjectionSource::BlobLines(BlobLineSelection {
                    path,
                    first_line: 1,
                    last_line: 2,
                })
            }),
        ),
        (
            NAMED_REGION_SOURCE,
            path("src/lib.rs").map(|path| {
                ProjectionSource::NamedRegion(NamedRegionSelection {
                    path,
                    start_marker: "// start".to_owned(),
                    end_marker: "// end".to_owned(),
                })
            }),
        ),
        (
            TREE_PATHS_SOURCE,
            path("crates").map(|root| {
                ProjectionSource::TreePaths(TreePathSelection {
                    root,
                    suffix: None,
                    maximum_depth: 3,
                })
            }),
        ),
        (
            RECORD_VALUE_SOURCE,
            set.clone().map(|set| {
                ProjectionSource::RecordValue(RecordValueSelection {
                    set,
                    key: "amiss::check".to_owned(),
                })
            }),
        ),
        (
            RECORD_SET_SOURCE,
            set.map(|set| ProjectionSource::RecordSet(RecordSetSelection { set })),
        ),
    ];
    for (tag, source) in sources {
        let value = source.and_then(|source| serde_json::to_value(&source).ok());
        assert_eq!(
            value.as_ref().and_then(|value| value.get("kind")),
            Some(&Value::String(tag.to_owned())),
            "{tag}"
        );
        assert_eq!(
            value.as_ref().and_then(|value| value.get("suffix")),
            None,
            "an absent suffix stays absent"
        );
    }
}
