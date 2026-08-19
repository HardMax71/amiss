use strum::{AsRefStr, EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

use crate::de::{self, Error, ErrorKind, fail};
use crate::json::Value;

macro_rules! resource_names {
    ($($variant:ident => $phase:literal),+ $(,)?) => {
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
            EnumIter,
            IntoStaticStr,
        )]
        #[strum(serialize_all = "kebab-case")]
        pub enum ResourceName {
            $($variant),+
        }

        impl ResourceName {
            #[must_use]
            pub fn all() -> impl ExactSizeIterator<Item = Self> {
                Self::iter()
            }

            #[must_use]
            pub const fn phase(self) -> &'static str {
                match self {
                    $(Self::$variant => $phase),+
                }
            }

            #[must_use]
            pub fn as_str(self) -> &'static str {
                self.into()
            }

            pub(super) fn decode(path: &str, value: Value) -> Result<Self, Error> {
                let raw = de::string(path, value)?;
                let Ok(resource) = raw.parse() else {
                    return fail(path, ErrorKind::InvalidValue);
                };
                Ok(resource)
            }
        }
    };
}

resource_names! {
    GitObjectBytes => "git",
    GitCompressedObjectBytes => "git",
    AggregateGitCompressedObjectBytesPerEvaluation => "git",
    GitPackDirectoryEntries => "git",
    GitPackFiles => "git",
    GitPackIndexBytes => "git",
    AggregateGitPackIndexBytes => "git",
    GitDeltaDepth => "git",
    GitIndexBytes => "git",
    GitTreeEntriesPerSnapshot => "git",
    DocumentsPerSnapshot => "discovery",
    ControlInputBytes => "configuration",
    SelectedControlBlobBytes => "discovery",
    AggregateSelectedControlBytesPerSnapshot => "discovery",
    RepositoryPolicyEntries => "configuration",
    DebtItems => "configuration",
    WaiverItems => "configuration",
    RawPathBytes => "git",
    DocumentBlobBytes => "discovery",
    ReferencedTargetBlobBytes => "resolution",
    AggregateReferencedTargetBytesPerSnapshot => "resolution",
    IgnoreDeclarationBlobBytes => "resolution",
    AggregateIgnoreDeclarationBytesPerSnapshot => "resolution",
    AggregateLineFragmentEvaluationBytesPerSnapshot => "resolution",
    AggregateHeadingAnchorEvaluationBytesPerSnapshot => "resolution",
    AggregateDocumentBytesPerSnapshot => "discovery",
    RawLinkDestinationBytes => "parse",
    ParserNesting => "parse",
    ParserNodesPerDocument => "parse",
    ParserNodesPerSnapshot => "parse",
    AggregateEmbeddedCodeEvaluationBytesPerSnapshot => "parse",
    ReferencesPerDocument => "parse",
    ReferencesPerSnapshot => "parse",
    DeclaredLabelsPerSnapshot => "parse",
    OrganizationPolicyEntries => "configuration",
    CompleteFindings => "policy",
    TypedAnalysisErrorsRetained => "internal",
    MachineJsonBytes => "output",
    PrivateTemporaryStorageBytes => "internal",
    EvaluatorManagedMemoryBytes => "internal",
}
