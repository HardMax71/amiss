use amiss_md::extract::BlockKind;
use amiss_scan::correlate::{Observation, Side};
use amiss_scan::resolve::{Intent, Resolution};
use amiss_scan::scan::SpanDisplay;
use amiss_wire::controls::{SourceConstruct, TargetKind};
use amiss_wire::digest::hb;
use amiss_wire::model::{Adapter, RepoPath};
use amiss_wire::report::IntentKind;
use amiss_wire::resolution::Missing;

pub(super) fn side(label: &str, offset: usize, count: usize, shared_target: Option<&str>) -> Side {
    let observations = (offset..offset.saturating_add(count))
        .map(|index| {
            shared_target.map_or_else(
                || observation(label, index),
                |target| observation_with_target(label, index, target),
            )
        })
        .collect();
    Side {
        observations,
        ..Side::default()
    }
}

fn observation(side: &str, index: usize) -> Observation {
    let token = format!("{side}/{index}");
    observation_with_target(side, index, &format!("targets/{token}.rs"))
}

fn observation_with_target(side: &str, index: usize, target: &str) -> Observation {
    let token = format!("{side}/{index}");
    let target = repo_path(target.to_owned());
    Observation {
        id: hb("amiss/bench-correlation-id", token.as_bytes()),
        adapter_contract_digest: hb("amiss/bench-adapter-contract", b"markdown"),
        document: repo_path("docs/references.md".to_owned()),
        span: (0, 0),
        display: SpanDisplay {
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
        },
        block_kind: BlockKind::Paragraph,
        node_path: Vec::new(),
        adapter: Adapter::Markdown,
        construct: SourceConstruct::InlineLink,
        external_destination: None,
        intent: Intent {
            kind: IntentKind::RepositoryPath,
            repository_path: Some(target.clone()),
            target_kind: Some(TargetKind::Either),
            external_scheme: None,
            query: None,
            fragment: None,
        },
        raw_destination: String::new(),
        raw_destination_digest: hb("amiss/scanner-raw-destination", target.as_bytes()),
        projection_digest: hb("amiss/scanner-source-projection", b"reference"),
        resolution: Resolution::Missing(Missing::PathNotFound {
            path: target,
            near: None,
            same_object_at: None,
        }),
        fragment_span: None,
        path_span: None,
    }
}

#[expect(clippy::expect_used, reason = "benchmark paths are fixed and valid")]
fn repo_path(raw: String) -> RepoPath {
    RepoPath::new(raw).expect("valid benchmark repository path")
}
