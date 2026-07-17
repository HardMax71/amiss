use crate::digest::Digest;
use strum::{AsRefStr, EnumDiscriminants, EnumIter, EnumString};

/// The two ordinary Git blob modes. Trees, symlinks, and gitlinks are
/// represented by other target types and cannot be smuggled into a blob.
#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumString, EnumIter)]
pub enum BlobMode {
    #[strum(serialize = "100644")]
    Regular,
    #[strum(serialize = "100755")]
    Executable,
}

/// The content evidence retained for a located blob. An available blob has
/// both digests; an LFS pointer has only the digest of the pointer bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumDiscriminants)]
#[strum_discriminants(name(BlobContentTag))]
#[strum_discriminants(derive(AsRefStr, EnumString, EnumIter))]
#[strum_discriminants(strum(serialize_all = "kebab-case"))]
pub enum BlobContent {
    Available {
        raw_digest: Digest,
        projection_digest: Digest,
    },
    LfsPointer {
        raw_digest: Digest,
    },
}

impl BlobContent {
    #[must_use]
    pub const fn projection_digest(self) -> Option<Digest> {
        match self {
            Self::Available {
                projection_digest, ..
            } => Some(projection_digest),
            Self::LfsPointer { .. } => None,
        }
    }

    #[must_use]
    pub const fn is_lfs_pointer(self) -> bool {
        matches!(self, Self::LfsPointer { .. })
    }
}

/// A located ordinary blob and the evidence read from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobTarget<P> {
    pub path: P,
    pub mode: BlobMode,
    pub content: BlobContent,
}

/// A located target. A tree has no blob content; a blob always carries a
/// valid blob mode and one exact content-evidence shape.
#[derive(Clone, Debug, PartialEq, Eq, EnumDiscriminants)]
#[strum_discriminants(name(TargetTag))]
#[strum_discriminants(derive(AsRefStr, EnumString, EnumIter))]
#[strum_discriminants(strum(serialize_all = "kebab-case"))]
pub enum Target<P> {
    Tree { path: P },
    Blob(BlobTarget<P>),
}

impl<P> Target<P> {
    #[must_use]
    pub const fn projection_digest(&self) -> Option<Digest> {
        match self {
            Self::Tree { .. } => None,
            Self::Blob(blob) => blob.content.projection_digest(),
        }
    }

    #[must_use]
    pub const fn is_lfs_pointer(&self) -> bool {
        match self {
            Self::Tree { .. } => false,
            Self::Blob(blob) => blob.content.is_lfs_pointer(),
        }
    }
}

/// A target that was absent at a known repository path. Each diagnostic owns
/// the path required by its wire row.
#[derive(Clone, Debug, PartialEq, Eq, EnumDiscriminants)]
#[strum_discriminants(name(MissingTag))]
#[strum_discriminants(derive(AsRefStr, EnumString, EnumIter))]
#[strum_discriminants(strum(serialize_all = "kebab-case"))]
pub enum Missing<P> {
    PathNotFound { path: P },
    LineFragmentOutOfRange { path: P },
}

/// A special Git entry that is present but cannot be followed as an ordinary
/// repository target. Each diagnostic owns the affected path.
#[derive(Clone, Debug, PartialEq, Eq, EnumDiscriminants)]
#[strum_discriminants(name(UnsupportedTargetTag))]
#[strum_discriminants(derive(AsRefStr, EnumString, EnumIter))]
#[strum_discriminants(strum(serialize_all = "kebab-case"))]
pub enum UnsupportedTarget<P> {
    Symlink { path: P },
    Gitlink { path: P },
}

/// Reference syntax whose target may be located but whose meaning is outside
/// the scanner's current evaluator.
#[derive(Clone, Debug, PartialEq, Eq, EnumDiscriminants)]
#[strum_discriminants(name(UnsupportedSemanticsTag))]
#[strum_discriminants(derive(AsRefStr, EnumString, EnumIter))]
#[strum_discriminants(strum(serialize_all = "kebab-case"))]
pub enum UnsupportedSemantics<P> {
    Query(Target<P>),
    Fragment(BlobTarget<P>),
    CodeFragment(Target<P>),
    SiteRoute,
    NetworkPath,
}

impl<P> UnsupportedSemantics<P> {
    #[must_use]
    pub const fn is_lfs_pointer(&self) -> bool {
        match self {
            Self::Query(target) | Self::CodeFragment(target) => target.is_lfs_pointer(),
            Self::Fragment(blob) => blob.content.is_lfs_pointer(),
            Self::SiteRoute | Self::NetworkPath => false,
        }
    }
}

/// Version-scoped forge references either identify a contained path outside
/// the candidate scope or fail before a path can be identified.
#[derive(Clone, Debug, PartialEq, Eq, EnumDiscriminants)]
#[strum_discriminants(name(VersionScopeTag))]
#[strum_discriminants(derive(AsRefStr, EnumString, EnumIter))]
#[strum_discriminants(strum(serialize_all = "kebab-case"))]
pub enum VersionScope<P> {
    KnownPath { path: P },
    UnknownPath,
}

/// A syntax defect that prevents a reference from identifying a repository or
/// external target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumString, EnumIter)]
#[strum(serialize_all = "kebab-case")]
pub enum InvalidReference {
    Uri,
    PercentEncoding,
    DecodedPathControl,
    PathTraversal,
    BackslashSeparator,
    EncodedSlash,
    FragmentEncoding,
    Syntax,
}

/// References that are valid but intentionally outside the evaluated
/// repository.
#[derive(Clone, Copy, Debug, PartialEq, Eq, AsRefStr, EnumString, EnumIter)]
#[strum(serialize_all = "kebab-case")]
pub enum ExternalReference {
    Url,
    ForeignRepository,
}

/// The total outcome of resolving one authored reference. The outer variants
/// are the semantic partitions used by evaluation, correlation, and summary
/// reporting; leaf enums retain the exact diagnostic and its required data.
#[derive(Clone, Debug, PartialEq, Eq, EnumDiscriminants)]
#[strum_discriminants(name(ResolutionTag))]
#[strum_discriminants(derive(AsRefStr, EnumString, EnumIter))]
#[strum_discriminants(strum(serialize_all = "kebab-case"))]
pub enum Resolution<P> {
    Resolved(Target<P>),
    Missing(Missing<P>),
    TypeMismatch(Target<P>),
    UnsupportedTarget(UnsupportedTarget<P>),
    UnsupportedSemantics(UnsupportedSemantics<P>),
    UnsupportedVersion(VersionScope<P>),
    Invalid(InvalidReference),
    External(ExternalReference),
}

impl<P> Resolution<P> {
    #[must_use]
    pub const fn is_lfs_pointer(&self) -> bool {
        match self {
            Self::Resolved(target) | Self::TypeMismatch(target) => target.is_lfs_pointer(),
            Self::UnsupportedSemantics(semantics) => semantics.is_lfs_pointer(),
            Self::Missing(_)
            | Self::UnsupportedTarget(_)
            | Self::UnsupportedVersion(_)
            | Self::Invalid(_)
            | Self::External(_) => false,
        }
    }
}
