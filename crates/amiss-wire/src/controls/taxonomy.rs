use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{AsRefStr, Display, EnumIter, EnumString, IntoStaticStr};

use crate::extraction::SourceConstruct;

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
pub enum PolicyDisposition {
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
    SerializeDisplay,
    DeserializeFromStr,
    Display,
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
    Display,
    AsRefStr,
    EnumIter,
    EnumString,
    IntoStaticStr,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum EligibleFindingKind {
    ExplicitTargetMissing,
    ExplicitTargetTypeMismatch,
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
    Display,
    AsRefStr,
    EnumIter,
    EnumString,
    IntoStaticStr,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
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
    Display,
    EnumString,
    AsRefStr,
    EnumIter,
    IntoStaticStr,
    SerializeDisplay,
    DeserializeFromStr,
)]
pub enum GitMode {
    #[strum(serialize = "100644")]
    RegularFile,
    #[strum(serialize = "100755")]
    ExecutableFile,
    #[strum(serialize = "040000")]
    Tree,
    #[strum(serialize = "120000")]
    Symlink,
    #[strum(serialize = "160000")]
    Gitlink,
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Display,
    EnumString,
    AsRefStr,
    EnumIter,
    IntoStaticStr,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum(serialize_all = "kebab-case")]
pub enum ContentAvailability {
    Available,
    NotRead,
    NotApplicable,
    LfsPointerOnly,
}
