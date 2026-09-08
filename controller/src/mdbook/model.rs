use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::IgnoredAny};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RenderContext {
    pub(super) version: String,
    pub(super) root: String,
    pub(super) config: serde_json::Value,
    pub(super) book: Book,
    pub(super) destination: String,
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

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Book {
    pub items: Vec<BookItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum BookItem {
    Chapter(Chapter),
    Separator,
    PartTitle(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Chapter {
    pub name: String,
    pub content: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub number: Option<Vec<u32>>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub path: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub source_path: Option<String>,
    pub sub_items: Vec<BookItem>,
    pub parent_names: Vec<String>,
}
