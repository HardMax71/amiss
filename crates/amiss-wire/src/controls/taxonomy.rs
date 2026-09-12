use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumIter, EnumString, IntoStaticStr};

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
    Serialize,
    Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
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
    Serialize,
    Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
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
    Serialize,
    Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
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
    Serialize,
    Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
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
    Serialize,
    Deserialize,
)]
pub enum SourceConstruct {
    #[strum(serialize = "markdown-inline-link")]
    #[serde(rename = "markdown-inline-link")]
    InlineLink,
    #[strum(serialize = "markdown-full-reference-link")]
    #[serde(rename = "markdown-full-reference-link")]
    FullReferenceLink,
    #[strum(serialize = "markdown-collapsed-reference-link")]
    #[serde(rename = "markdown-collapsed-reference-link")]
    CollapsedReferenceLink,
    #[strum(serialize = "markdown-shortcut-reference-link")]
    #[serde(rename = "markdown-shortcut-reference-link")]
    ShortcutReferenceLink,
    #[strum(serialize = "markdown-autolink")]
    #[serde(rename = "markdown-autolink")]
    Autolink,
    #[strum(serialize = "markdown-inline-image")]
    #[serde(rename = "markdown-inline-image")]
    InlineImage,
    #[strum(serialize = "markdown-full-reference-image")]
    #[serde(rename = "markdown-full-reference-image")]
    FullReferenceImage,
    #[strum(serialize = "markdown-collapsed-reference-image")]
    #[serde(rename = "markdown-collapsed-reference-image")]
    CollapsedReferenceImage,
    #[strum(serialize = "markdown-shortcut-reference-image")]
    #[serde(rename = "markdown-shortcut-reference-image")]
    ShortcutReferenceImage,
    #[strum(serialize = "asciidoc-xref-macro")]
    #[serde(rename = "asciidoc-xref-macro")]
    AsciidocCrossReference,
    #[strum(serialize = "asciidoc-internal-xref")]
    #[serde(rename = "asciidoc-internal-xref")]
    AsciidocInternalCrossReference,
    #[strum(serialize = "asciidoc-link-macro")]
    #[serde(rename = "asciidoc-link-macro")]
    AsciidocLinkMacro,
    #[strum(serialize = "asciidoc-block-image")]
    #[serde(rename = "asciidoc-block-image")]
    AsciidocBlockImage,
    #[strum(serialize = "asciidoc-inline-image")]
    #[serde(rename = "asciidoc-inline-image")]
    AsciidocInlineImage,
    #[strum(serialize = "asciidoc-include")]
    #[serde(rename = "asciidoc-include")]
    AsciidocInclude,
    #[strum(serialize = "rst-inline-hyperlink")]
    #[serde(rename = "rst-inline-hyperlink")]
    RstInlineHyperlink,
    #[strum(serialize = "rst-named-target")]
    #[serde(rename = "rst-named-target")]
    RstNamedTarget,
    #[strum(serialize = "rst-image-directive")]
    #[serde(rename = "rst-image-directive")]
    RstImageDirective,
    #[strum(serialize = "rst-include-directive")]
    #[serde(rename = "rst-include-directive")]
    RstIncludeDirective,
    #[strum(serialize = "rst-file-option")]
    #[serde(rename = "rst-file-option")]
    RstFileOption,
    #[strum(serialize = "rst-doc-role")]
    #[serde(rename = "rst-doc-role")]
    RstDocRole,
    #[strum(serialize = "rst-ref-role")]
    #[serde(rename = "rst-ref-role")]
    RstRefRole,
    #[strum(serialize = "markdown-link-reference-definition")]
    #[serde(rename = "markdown-link-reference-definition")]
    LinkReferenceDefinition,
    #[strum(serialize = "html-anchor")]
    #[serde(rename = "html-anchor")]
    HtmlAnchor,
    #[strum(serialize = "html-image")]
    #[serde(rename = "html-image")]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, IntoStaticStr, Serialize, Deserialize)]
#[strum(serialize_all = "kebab-case")]
#[serde(rename_all = "kebab-case")]
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
    #[strum(serialize = "100644")]
    #[serde(rename = "100644")]
    RegularFile,
    #[strum(serialize = "100755")]
    #[serde(rename = "100755")]
    ExecutableFile,
    #[strum(serialize = "040000")]
    #[serde(rename = "040000")]
    Tree,
    #[strum(serialize = "120000")]
    #[serde(rename = "120000")]
    Symlink,
    #[strum(serialize = "160000")]
    #[serde(rename = "160000")]
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
