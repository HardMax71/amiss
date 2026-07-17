use amiss_wire::resolution::{
    BlobContentTag, BlobMode, ExternalReference, InvalidReference, Missing, MissingTag, Resolution,
    ResolutionTag, Target, TargetTag, UnsupportedSemantics, UnsupportedTarget, VersionScope,
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
