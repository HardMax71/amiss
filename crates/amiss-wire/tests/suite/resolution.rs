use amiss_wire::digest::hb;
use amiss_wire::resolution::{
    BlobContent, BlobContentTag, BlobMode, BlobTarget, ExternalReference, InvalidReference,
    Missing, MissingTag, Resolution, ResolutionTag, Target, TargetTag, UnsupportedSemantics,
    UnsupportedTarget, VersionScope,
};
use strum::IntoDiscriminant;

#[test]
fn payload_variant_names_are_generated_in_kebab_case() {
    let missing = Missing::LineFragmentOutOfRange { path: () };
    let target = UnsupportedTarget::Gitlink { path: () };
    let semantics = UnsupportedSemantics::<()>::CodeFragment(Target::Tree { path: () });
    let scope = VersionScope::KnownPath { path: () };
    let resolution = Resolution::TypeMismatch(Target::Tree { path: () });

    assert_eq!(
        missing.discriminant().as_ref(),
        "line-fragment-out-of-range"
    );
    assert_eq!(target.discriminant().as_ref(), "gitlink");
    assert_eq!(semantics.discriminant().as_ref(), "code-fragment");
    assert_eq!(scope.discriminant().as_ref(), "known-path");
    assert_eq!(resolution.discriminant().as_ref(), "type-mismatch");
}

#[test]
fn fieldless_reasons_round_trip_generated_names() {
    assert_eq!(
        "percent-encoding".parse::<InvalidReference>(),
        Ok(InvalidReference::PercentEncoding)
    );
    assert_eq!(
        InvalidReference::DecodedPathControl.as_ref(),
        "decoded-path-control"
    );
    assert_eq!(
        "foreign-repository".parse::<ExternalReference>(),
        Ok(ExternalReference::ForeignRepository)
    );
    assert_eq!(ExternalReference::Url.as_ref(), "url");
    assert!("not-a-reason".parse::<InvalidReference>().is_err());
}

#[test]
fn generated_tags_decode_payload_variant_names_without_payload_defaults() {
    assert_eq!(
        "missing".parse::<ResolutionTag>(),
        Ok(ResolutionTag::Missing)
    );
    assert_eq!(
        "line-fragment-out-of-range".parse::<MissingTag>(),
        Ok(MissingTag::LineFragmentOutOfRange)
    );
    assert_eq!("blob".parse::<TargetTag>(), Ok(TargetTag::Blob));
    assert_eq!(
        "lfs-pointer".parse::<BlobContentTag>(),
        Ok(BlobContentTag::LfsPointer)
    );
    assert_eq!("100644".parse::<BlobMode>(), Ok(BlobMode::Regular));
    assert_eq!(BlobMode::Executable.as_ref(), "100755");
}

fn available() -> BlobContent {
    BlobContent::Available {
        raw_digest: hb("amiss/raw-evidence", b"raw"),
        projection_digest: hb("amiss/scanner-source-projection", b"projection"),
    }
}

fn pointer() -> BlobContent {
    BlobContent::LfsPointer {
        raw_digest: hb("amiss/raw-evidence", b"pointer"),
    }
}

fn blob(content: BlobContent) -> Target<()> {
    Target::Blob(BlobTarget {
        path: (),
        mode: BlobMode::Regular,
        content,
    })
}

/// A pointer carries no projection and says so at every level that wraps it,
/// while a tree is not a pointer at all.
#[test]
fn the_pointer_answer_survives_every_wrapper() {
    assert!(available().projection_digest().is_some());
    assert!(pointer().projection_digest().is_none());
    assert!(!available().is_lfs_pointer());
    assert!(pointer().is_lfs_pointer());

    assert!(blob(available()).projection_digest().is_some());
    assert!(blob(pointer()).projection_digest().is_none());
    assert!(Target::Tree { path: () }.projection_digest().is_none());
    assert!(!blob(available()).is_lfs_pointer());
    assert!(blob(pointer()).is_lfs_pointer());
    assert!(!Target::Tree { path: () }.is_lfs_pointer());

    let fragment = |content| {
        UnsupportedSemantics::Fragment(BlobTarget {
            path: (),
            mode: BlobMode::Regular,
            content,
        })
    };
    assert!(fragment(pointer()).is_lfs_pointer());
    assert!(!fragment(available()).is_lfs_pointer());
    assert!(UnsupportedSemantics::<()>::Query(blob(pointer())).is_lfs_pointer());
    assert!(!UnsupportedSemantics::<()>::Query(blob(available())).is_lfs_pointer());
    assert!(!UnsupportedSemantics::<()>::SiteRoute.is_lfs_pointer());

    assert!(Resolution::Resolved(blob(pointer())).is_lfs_pointer());
    assert!(!Resolution::Resolved(blob(available())).is_lfs_pointer());
    assert!(Resolution::TypeMismatch(blob(pointer())).is_lfs_pointer());
    assert!(
        Resolution::UnsupportedSemantics(fragment(pointer())).is_lfs_pointer(),
        "a wrapper does not lose the answer"
    );
    assert!(!Resolution::<()>::Invalid(InvalidReference::PathTraversal).is_lfs_pointer());
}
