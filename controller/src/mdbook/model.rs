use serde::{Deserialize, Serialize};

use super::config::Config;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenderContext<P, R> {
    pub version: String,
    pub root: String,
    pub config: Config<P, R>,
    pub book: Book,
    pub destination: String,
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
