use std::collections::BTreeMap;

use serde::{Deserialize, de::IgnoredAny};

#[derive(Deserialize)]
pub(super) struct RenderContext {
    pub(super) version: String,
    pub(super) config: serde_json::Value,
    pub(super) book: Book,
}

#[derive(Deserialize)]
pub(super) struct Config {
    pub(super) book: BookConfig,
    #[serde(default)]
    pub(super) output: serde_json::Value,
}

#[derive(Deserialize)]
pub(super) struct BookConfig {
    #[serde(default, deserialize_with = "json_serde::deserialize_some")]
    pub(super) src: Option<String>,
}

#[derive(Deserialize)]
#[expect(
    clippy::zero_sized_map_values,
    reason = "only the JSON object shape is needed"
)]
pub(super) struct HtmlOutput {
    pub(super) html: BTreeMap<String, IgnoredAny>,
}

#[derive(Deserialize)]
pub(super) struct Book {
    pub(super) items: Vec<BookItem>,
}

#[derive(Deserialize)]
pub(super) enum BookItem {
    Chapter(Chapter),
    Separator,
    PartTitle(String),
}

#[derive(Deserialize)]
pub(super) struct Chapter {
    #[serde(deserialize_with = "Option::deserialize")]
    pub(super) path: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(super) source_path: Option<String>,
    pub(super) sub_items: Vec<BookItem>,
}
