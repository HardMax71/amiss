use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{Display, EnumString};

use crate::assessment::Nullable;
use crate::model::{ArtifactId, RepoPathText};

pub const SPHINX_INVENTORY_PRODUCER: &str = "sphinx-inventory-set";
pub const SPHINX_INVENTORY_VERSION: &str = "1";
pub const SPHINX_LABEL: &str = "sphinx-label";
pub const SITE_BUILD_PRODUCER: &str = "site-build";
pub const SITE_BUILD_VERSION: &str = "0.5.1";
pub const SITE_ROUTE: &str = "site-route";
pub const SITE_GENERATED_ROUTE: &str = "site-generated-route";
pub const SITE_REDIRECT: &str = "site-redirect";
pub const SITE_NAVIGATION: &str = "site-navigation";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum SiteBuildObservation {
    #[serde(rename = "site-route")]
    Route {
        route: String,
        source: RepoPathText,
        anchors: Vec<String>,
    },
    #[serde(rename = "site-generated-route")]
    GeneratedRoute {
        route: String,
        source: Nullable<RepoPathText>,
        anchors: Vec<String>,
    },
    #[serde(rename = "site-redirect")]
    Redirect {
        route: String,
        source: RepoPathText,
        destination: String,
    },
    #[serde(rename = "site-navigation")]
    Navigation {
        root: Nullable<RepoPathText>,
        manifest: RepoPathText,
        entrypoints: Vec<String>,
        reachable: Vec<RepoPathText>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SphinxLabelObservation {
    pub kind: SphinxLabelKind,
    pub inventory: ArtifactId,
    pub name: String,
    pub destination: String,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Display, EnumString, SerializeDisplay, DeserializeFromStr,
)]
pub enum SphinxLabelKind {
    #[strum(serialize = "sphinx-label")]
    Current,
}
