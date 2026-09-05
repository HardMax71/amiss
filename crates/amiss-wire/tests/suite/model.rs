use std::collections::{BTreeMap, BTreeSet};

use amiss_wire::model::{Adapter, ForgeDialect, OwnerId, RepoPath, RepositoryIdentity, UtcInstant};
use strum::IntoEnumIterator;

#[test]
fn repo_paths_order_and_borrow_as_their_bytes() {
    let path = |raw: &str| RepoPath::new(raw.to_owned()).unwrap();
    let early = path("docs/a.md");
    let late = path("docs/b.md");
    assert!(early < late, "ordering is byte order");

    let mut map: BTreeMap<RepoPath, u32> = BTreeMap::new();
    map.insert(early, 1);
    map.insert(late, 2);
    assert_eq!(
        map.get(b"docs/a.md".as_slice()),
        Some(&1),
        "a byte query finds the owning path"
    );
}

#[test]
fn an_owner_id_keeps_its_grammar_and_its_text() {
    let owner = OwnerId::new("team:docs-guild".to_owned()).unwrap();
    assert_eq!(owner.as_str(), "team:docs-guild");
    assert!(
        OwnerId::new("team:docs!".to_owned()).is_none(),
        "an illegal tail byte is refused"
    );
    assert!(
        OwnerId::new("team:Docs".to_owned()).is_none(),
        "an uppercase tail byte is refused"
    );
}

#[test]
fn epoch_seconds_render_the_documented_instants() {
    let render = |seconds: i64| {
        UtcInstant::from_epoch_seconds(seconds)
            .unwrap()
            .as_str()
            .to_owned()
    };
    assert_eq!(render(0), "1970-01-01T00:00:00Z");
    assert_eq!(render(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(render(4_102_444_799), "2099-12-31T23:59:59Z");
    assert_eq!(render(951_868_800), "2000-03-01T00:00:00Z");
    assert_eq!(render(4_107_456_000), "2100-02-28T00:00:00Z");
    assert_eq!(render(4_107_542_400), "2100-03-01T00:00:00Z");
    assert_eq!(render(946_598_400), "1999-12-31T00:00:00Z");
    assert_eq!(render(2_147_472_000), "2038-01-19T00:00:00Z");

    let mut previous = render(-2_000_000_000);
    for step in 1_i64..=800 {
        let sample = render(-2_000_000_000 + step * 86_400 * 90);
        assert!(sample > previous, "instants advance at step {step}");
        previous = sample;
    }
}

#[test]
fn a_repository_name_is_bounded_and_never_a_dot_path() {
    let identity = |name: &str| {
        RepositoryIdentity::new(
            "forge.example".to_owned(),
            "acme".to_owned(),
            name.to_owned(),
        )
    };
    assert!(identity("widget").is_some());
    assert!(
        identity(&"n".repeat(100)).is_some(),
        "the length ceiling itself"
    );
    assert!(identity(&"n".repeat(101)).is_none());
    assert!(identity("").is_none());
    assert!(identity(".").is_none());
    assert!(identity("..").is_none());
}

#[test]
fn the_known_host_table_is_exact() {
    assert_eq!(
        ForgeDialect::default_for_host("github.com"),
        Some(ForgeDialect::Github)
    );
    assert_eq!(
        ForgeDialect::default_for_host("gitlab.com"),
        Some(ForgeDialect::Gitlab)
    );
    assert_eq!(
        ForgeDialect::default_for_host("codeberg.org"),
        Some(ForgeDialect::Gitea)
    );
    assert_eq!(
        ForgeDialect::default_for_host("bitbucket.org"),
        Some(ForgeDialect::BitbucketCloud)
    );
    assert_eq!(ForgeDialect::default_for_host("forge.example"), None);
}

/// Every adapter projection is populated; profiles are distinct per adapter,
/// while the shared contracts must still name more than one value.
#[test]
fn the_adapter_tables_are_populated_and_distinct() {
    let adapters: Vec<Adapter> = Adapter::iter().collect();
    let ids: BTreeSet<&str> = adapters.iter().map(AsRef::as_ref).collect();
    assert_eq!(ids.len(), adapters.len());
    let parser_names: BTreeSet<&str> = adapters
        .iter()
        .map(|adapter| adapter.metadata().parser_name)
        .collect();
    assert_eq!(parser_names.len(), adapters.len());
    assert!(parser_names.iter().all(|text| !text.is_empty()));
    let profiles: BTreeSet<&str> = adapters
        .iter()
        .map(|adapter| adapter.metadata().grammar_profile)
        .collect();
    assert_eq!(profiles.len(), adapters.len());
    assert!(profiles.iter().all(|text| !text.is_empty()));
    let projections = [
        adapters
            .iter()
            .map(|adapter| adapter.metadata().frontmatter_contract)
            .collect::<BTreeSet<_>>(),
        adapters
            .iter()
            .map(|adapter| adapter.metadata().source_projection)
            .collect(),
        adapters
            .iter()
            .map(|adapter| {
                adapter
                    .metadata()
                    .structural_address
                    .map_or("none", Into::into)
            })
            .collect(),
    ];
    for values in projections {
        assert!(values.len() > 1, "more than one contract value");
        assert!(values.iter().all(|text| !text.is_empty()));
    }
}

/// The digest's debug form is its display form: one spelling on every
/// channel, `sha256:` plus sixty-four hex bytes.
#[test]
fn a_digest_debugs_as_it_displays() {
    let digest = amiss_wire::digest::hb("test/domain", b"payload");
    let shown = format!("{digest}");
    assert_eq!(format!("{digest:?}"), shown);
    assert!(shown.starts_with("sha256:") && shown.len() == 71, "{shown}");
    assert_eq!(amiss_wire::digest::Digest::from_wire(&shown), Some(digest));
}
