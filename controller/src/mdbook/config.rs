use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_with::{As, MapPreventDuplicates, Same};

#[serde_with::skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config<P, R> {
    pub book: BookConfig,
    pub build: Option<BuildConfig>,
    pub rust: Option<RustConfig>,
    pub output: HtmlOutput<R>,
    pub preprocessor: Option<P>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct BookConfig {
    #[serde(deserialize_with = "Option::deserialize")]
    pub title: Option<String>,
    pub authors: Vec<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub language: Option<String>,
    #[serde(deserialize_with = "Option::deserialize")]
    pub text_direction: Option<TextDirection>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum TextDirection {
    #[serde(rename = "ltr")]
    LeftToRight,
    #[serde(rename = "rtl")]
    RightToLeft,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct BuildConfig {
    pub build_dir: String,
    pub create_missing: bool,
    pub use_default_preprocessors: bool,
    pub extra_watch_dirs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RustConfig {
    #[serde(deserialize_with = "Option::deserialize")]
    pub edition: Option<RustEdition>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum RustEdition {
    #[serde(rename = "2015")]
    E2015,
    #[serde(rename = "2018")]
    E2018,
    #[serde(rename = "2021")]
    E2021,
    #[serde(rename = "2024")]
    E2024,
}

#[serde_with::skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct HtmlOutput<R> {
    pub html: Option<HtmlConfig>,
    #[serde(flatten)]
    pub additional: R,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct NoExtensions {}

#[serde_with::skip_serializing_none]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct HtmlConfig {
    pub theme: Option<String>,
    pub default_theme: Option<String>,
    pub preferred_dark_theme: Option<String>,
    pub smart_punctuation: Option<bool>,
    pub definition_lists: Option<bool>,
    pub admonitions: Option<bool>,
    pub mathjax_support: Option<bool>,
    pub additional_css: Option<Vec<String>>,
    pub additional_js: Option<Vec<String>>,
    pub fold: Option<Fold>,
    #[serde(flatten)]
    pub playground: Option<PlaygroundConfig>,
    pub code: Option<Code>,
    pub print: Option<Print>,
    pub no_section_label: Option<bool>,
    pub search: Option<Search>,
    pub git_repository_url: Option<String>,
    pub git_repository_icon: Option<String>,
    pub input_404: Option<String>,
    pub site_url: Option<String>,
    pub cname: Option<String>,
    pub edit_url_template: Option<String>,
    pub live_reload_endpoint: Option<String>,
    #[serde(default, with = "As::<Option<MapPreventDuplicates<Same, Same>>>")]
    pub redirect: Option<BTreeMap<String, String>>,
    pub hash_files: Option<bool>,
    pub sidebar_header_nav: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaygroundConfig {
    Playground(Playground),
    Playpen(Playground),
}

#[serde_with::skip_serializing_none]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Playground {
    pub editable: Option<bool>,
    pub copyable: Option<bool>,
    pub copy_js: Option<bool>,
    pub line_numbers: Option<bool>,
    pub runnable: Option<bool>,
}

#[serde_with::skip_serializing_none]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Fold {
    pub enable: Option<bool>,
    pub level: Option<u8>,
}

#[serde_with::skip_serializing_none]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Code {
    #[serde(default, with = "As::<Option<MapPreventDuplicates<Same, Same>>>")]
    pub hidelines: Option<BTreeMap<String, String>>,
}

#[serde_with::skip_serializing_none]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Print {
    pub enable: Option<bool>,
    pub page_break: Option<bool>,
}

#[serde_with::skip_serializing_none]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Search {
    pub enable: Option<bool>,
    pub limit_results: Option<u32>,
    pub teaser_word_count: Option<u32>,
    pub use_boolean_and: Option<bool>,
    pub boost_title: Option<u8>,
    pub boost_hierarchy: Option<u8>,
    pub boost_paragraph: Option<u8>,
    pub expand: Option<bool>,
    pub heading_split_level: Option<u8>,
    pub copy_js: Option<bool>,
    #[serde(default, with = "As::<Option<MapPreventDuplicates<Same, Same>>>")]
    pub chapter: Option<BTreeMap<String, SearchChapterSettings>>,
}

#[serde_with::skip_serializing_none]
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SearchChapterSettings {
    pub enable: Option<bool>,
}
