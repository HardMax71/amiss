#![cfg(test)]

use amiss_git::{GitLimits, GitResources, Repository};
use amiss_wire::digest::{hb, hj};
use amiss_wire::model::{Adapter, ObjectFormat, RepoPath};
use amiss_wire::report::{EngineProvenance, adapter_contract};

use super::{ObservationContext, resolved_observation};
use crate::observe::{OBSERVATION_ID_DOMAIN, ObservationIdentity, observation_input};
use crate::resolve::{Resolver, TargetCache};
use crate::resources::{ScanLimits, ScanResources};

#[test]
fn resolved_observations_bind_the_fields_retained_for_reporting() {
    let directory = tempfile::tempdir().unwrap();
    amiss_fixtures::init_repository(directory.path()).unwrap();
    let repository = Repository::open(directory.path(), ObjectFormat::Sha1).unwrap();
    let discovery = crate::discovery::empty_discovery();
    let labels = std::collections::BTreeMap::new();
    let engine = EngineProvenance {
        version: "0.0.0-test".to_owned(),
        digest: hb("amiss/test-engine", b"typed observations"),
    };
    let context = ObservationContext {
        engine: &engine,
        forge: None,
        semantic: crate::semantic::View {
            labels: &labels,
            routes: None,
        },
    };
    let paths = [
        RepoPath::new("docs/quoted-\"β.md".to_owned()).unwrap(),
        RepoPath::from_bytes(b"docs/raw-\xff.md".to_vec()).unwrap(),
    ];
    for (adapter, source) in [
        (
            Adapter::Markdown,
            "[local](target.md) [remote](https://example.com/doc?q#part)",
        ),
        (
            Adapter::Mdx,
            "[local](target.md) [remote](https://example.com/doc?q#part)",
        ),
        (
            Adapter::AsciiDoc,
            "link:target.md[local] link:https://example.com/doc?q#part[remote]",
        ),
        (
            Adapter::Rst,
            "`local <target.md>`_ `remote <https://example.com/doc?q#part>`_",
        ),
    ] {
        let mut scan = ScanResources::new(ScanLimits::CONTRACT);
        let scanned = crate::scan::scan_bytes(&mut scan, adapter, source.as_bytes()).unwrap();
        assert_eq!(scanned.occurrences.len(), 2, "{adapter:?}");
        let mut git = GitResources::new(GitLimits::CONTRACT);
        let mut cache = TargetCache::default();
        let mut resolver = Resolver::new(&repository, &mut git, &mut scan, &mut cache, &discovery);
        let contract = adapter_contract(&engine, adapter).1;
        for path in &paths {
            for occurrence in &scanned.occurrences {
                let observation = resolved_observation(
                    &mut resolver,
                    context,
                    adapter,
                    contract,
                    path,
                    occurrence,
                )
                .unwrap();
                let retained = observation_input(&ObservationIdentity {
                    adapter: observation.adapter,
                    contract_digest: observation.adapter_contract_digest,
                    document: &observation.document,
                    construct: observation.construct,
                    node_path: &observation.node_path,
                    projection_digest: observation.projection_digest,
                    intent: &observation.intent,
                    raw_destination_digest: observation.raw_destination_digest,
                });
                assert_eq!(observation.id, hj(OBSERVATION_ID_DOMAIN, &retained));
                assert!(matches!(
                    resolved_observation(
                        &mut resolver,
                        context,
                        Adapter::PlainAdvisory,
                        contract,
                        path,
                        occurrence,
                    ),
                    Err(crate::Error::Internal)
                ));
            }
        }
    }
}
