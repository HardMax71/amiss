use amiss_wire::{
    de::{Error, ErrorKind},
    manifest::{ReleaseManifest, canonical_release_manifest, parse_release_manifest},
};

const MANIFEST: &[u8] = include_bytes!("../../../../spec/examples/scanner-release-manifest.json");

#[test]
fn manifest_root_is_an_object_and_preserves_canonical_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let manifest = parse_release_manifest(MANIFEST)?;
    super::input::assert_object_required(
        (&manifest, parse_release_manifest),
        &manifest,
        (
            &manifest.artifacts,
            &manifest.build_source,
            &manifest.dependency_lock,
            manifest.dependency_lock_digest,
            &manifest.engine_version,
            manifest.schema,
        ),
    )?;
    let (bytes, digest) = canonical_release_manifest(&manifest)?;
    let replay = parse_release_manifest(&bytes)?;
    assert_eq!(replay, manifest);
    assert_eq!(canonical_release_manifest(&replay)?.1, digest);
    Ok(())
}

#[test]
fn every_manifest_record_requires_object_input() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = parse_release_manifest(MANIFEST)?;
    let text = serde_json::to_string(&manifest)?;
    let source = &manifest.build_source;
    let repository = &source.repository;
    let lock = &manifest.dependency_lock;
    let lock_file = &lock.files[0];
    let artifact = &manifest.artifacts[0];
    let runtime = &artifact.runtime_files[0];
    let cases = [
        (
            serde_json::to_string(source)?,
            serde_json::to_string(&(&source.commit_oid, source.object_format, repository))?,
        ),
        (
            serde_json::to_string(repository)?,
            serde_json::to_string(&(repository.host(), repository.name(), repository.owner()))?,
        ),
        (
            serde_json::to_string(lock)?,
            serde_json::to_string(&(&lock.files, lock.schema))?,
        ),
        (
            serde_json::to_string(lock_file)?,
            serde_json::to_string(&(&lock_file.path, lock_file.raw_digest))?,
        ),
        (
            serde_json::to_string(artifact)?,
            serde_json::to_string(&(
                &artifact.artifact_name,
                artifact.binary_sha256,
                artifact.engine_digest,
                artifact.environment_contract,
                artifact.platform,
                artifact.runtime_contract,
                &artifact.runtime_files,
                &artifact.tree_path,
            ))?,
        ),
        (
            serde_json::to_string(runtime)?,
            serde_json::to_string(&(
                runtime.file_sha256,
                runtime.git_mode,
                &runtime.path,
                runtime.role,
            ))?,
        ),
    ];
    let mut rejections = Vec::new();
    for (object, positional) in cases {
        assert_eq!(text.matches(&object).count(), 1);
        let changed = text.replacen(&object, &positional, 1);
        assert_ne!(changed, text);
        assert_eq!(serde_json::from_str::<ReleaseManifest>(&changed)?, manifest);
        rejections.push(parse_release_manifest(changed.as_bytes()).err());
    }
    assert_eq!(
        rejections,
        vec![
            Some(Error {
                path: "$".to_owned(),
                kind: ErrorKind::InvalidValue,
            });
            6
        ]
    );
    Ok(())
}
