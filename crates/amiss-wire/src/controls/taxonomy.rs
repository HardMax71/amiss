use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{AsRefStr, Display, EnumIter, EnumString, IntoStaticStr};

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AsRefStr,
    EnumString,
    IntoStaticStr,
    Display,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum IncludeKind {
    Document,
    Tree,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AsRefStr,
    EnumString,
    IntoStaticStr,
    Display,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum Disposition {
    Warn,
    Fail,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AsRefStr,
    EnumString,
    IntoStaticStr,
    serde_with::SerializeDisplay,
    serde_with::DeserializeFromStr,
    strum::Display,
)]
#[strum(serialize_all = "kebab-case")]
pub enum Profile {
    Observe,
    EnforceIntroduced,
    Enforce,
}

impl Profile {
    #[must_use]
    pub const fn enforces(self) -> bool {
        matches!(self, Self::EnforceIntroduced | Self::Enforce)
    }

    #[must_use]
    pub const fn introduced_only(self) -> bool {
        matches!(self, Self::EnforceIntroduced)
    }

    #[must_use]
    pub const fn policy_defaults(self) -> Self {
        match self {
            Self::Observe => Self::Observe,
            Self::EnforceIntroduced | Self::Enforce => Self::Enforce,
        }
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AsRefStr,
    EnumIter,
    EnumString,
    IntoStaticStr,
    Display,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum PromotableFindingKind {
    ExplicitTargetMissing,
    ExplicitTargetTypeMismatch,
    InvalidReference,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AsRefStr,
    EnumIter,
    EnumString,
    IntoStaticStr,
    Serialize,
    Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum EligibleFindingKind {
    ExplicitTargetMissing,
    ExplicitTargetTypeMismatch,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AsRefStr,
    EnumIter,
    EnumString,
    IntoStaticStr,
    Display,
    SerializeDisplay,
    DeserializeFromStr,
)]
pub enum SourceConstruct {
    #[strum(serialize = "markdown-inline-link")]
    InlineLink,
    #[strum(serialize = "markdown-full-reference-link")]
    FullReferenceLink,
    #[strum(serialize = "markdown-collapsed-reference-link")]
    CollapsedReferenceLink,
    #[strum(serialize = "markdown-shortcut-reference-link")]
    ShortcutReferenceLink,
    #[strum(serialize = "markdown-autolink")]
    Autolink,
    #[strum(serialize = "markdown-inline-image")]
    InlineImage,
    #[strum(serialize = "markdown-full-reference-image")]
    FullReferenceImage,
    #[strum(serialize = "markdown-collapsed-reference-image")]
    CollapsedReferenceImage,
    #[strum(serialize = "markdown-shortcut-reference-image")]
    ShortcutReferenceImage,
    #[strum(serialize = "asciidoc-xref-macro")]
    AsciidocCrossReference,
    #[strum(serialize = "asciidoc-internal-xref")]
    AsciidocInternalCrossReference,
    #[strum(serialize = "asciidoc-link-macro")]
    AsciidocLinkMacro,
    #[strum(serialize = "asciidoc-block-image")]
    AsciidocBlockImage,
    #[strum(serialize = "asciidoc-inline-image")]
    AsciidocInlineImage,
    #[strum(serialize = "asciidoc-include")]
    AsciidocInclude,
    #[strum(serialize = "rst-inline-hyperlink")]
    RstInlineHyperlink,
    #[strum(serialize = "rst-named-target")]
    RstNamedTarget,
    #[strum(serialize = "rst-image-directive")]
    RstImageDirective,
    #[strum(serialize = "rst-include-directive")]
    RstIncludeDirective,
    #[strum(serialize = "rst-file-option")]
    RstFileOption,
    #[strum(serialize = "rst-doc-role")]
    RstDocRole,
    #[strum(serialize = "rst-ref-role")]
    RstRefRole,
    #[strum(serialize = "markdown-link-reference-definition")]
    LinkReferenceDefinition,
    #[strum(serialize = "html-anchor")]
    HtmlAnchor,
    #[strum(serialize = "html-image")]
    HtmlImage,
}

impl SourceConstruct {
    /// Whether the consuming syntax node is an image form, which fixes the
    /// authored target kind.
    #[must_use]
    pub const fn is_image(self) -> bool {
        matches!(
            self,
            Self::InlineImage
                | Self::FullReferenceImage
                | Self::CollapsedReferenceImage
                | Self::ShortcutReferenceImage
                | Self::AsciidocBlockImage
                | Self::AsciidocInlineImage
                | Self::RstImageDirective
                | Self::HtmlImage
        )
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AsRefStr,
    EnumIter,
    EnumString,
    IntoStaticStr,
    Serialize,
    Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum TargetKind {
    Blob,
    Tree,
    Either,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum EntryKind {
    Blob,
    Tree,
    Symlink,
    Gitlink,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    AsRefStr,
    EnumIter,
    IntoStaticStr,
    Serialize,
    Deserialize,
)]
pub enum GitMode {
    #[serde(rename = "100644")]
    #[strum(serialize = "100644")]
    RegularFile,
    #[serde(rename = "100755")]
    #[strum(serialize = "100755")]
    ExecutableFile,
    #[serde(rename = "040000")]
    #[strum(serialize = "040000")]
    Tree,
    #[serde(rename = "120000")]
    #[strum(serialize = "120000")]
    Symlink,
    #[serde(rename = "160000")]
    #[strum(serialize = "160000")]
    Gitlink,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumIter, IntoStaticStr, Serialize, Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
pub enum ContentAvailability {
    Available,
    NotRead,
    NotApplicable,
    LfsPointerOnly,
}
