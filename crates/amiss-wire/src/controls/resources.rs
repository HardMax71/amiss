use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{AsRefStr, Display, EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

use crate::report::model::AnalysisPhase;

macro_rules! resource_names {
    ($($variant:ident => $phase:ident),+ $(,)?) => {
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
            Display,
            SerializeDisplay,
            DeserializeFromStr,
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
            pub const fn phase(self) -> AnalysisPhase {
                match self {
                    $(Self::$variant => AnalysisPhase::$phase),+
                }
            }

            #[must_use]
            pub fn as_str(self) -> &'static str {
                self.into()
            }
        }
    };
}

resource_names! {
    GitObjectBytes => Git,
    GitCompressedObjectBytes => Git,
    AggregateGitCompressedObjectBytesPerEvaluation => Git,
    GitPackDirectoryEntries => Git,
    GitPackFiles => Git,
    GitPackIndexBytes => Git,
    AggregateGitPackIndexBytes => Git,
    GitDeltaDepth => Git,
    GitIndexBytes => Git,
    GitTreeEntriesPerSnapshot => Git,
    DocumentsPerSnapshot => Discovery,
    ControlInputBytes => Configuration,
    SelectedControlBlobBytes => Discovery,
    AggregateSelectedControlBytesPerSnapshot => Discovery,
    RepositoryPolicyEntries => Configuration,
    DebtItems => Configuration,
    WaiverItems => Configuration,
    RawPathBytes => Git,
    DocumentBlobBytes => Discovery,
    ReferencedTargetBlobBytes => Resolution,
    AggregateReferencedTargetBytesPerSnapshot => Resolution,
    IgnoreDeclarationBlobBytes => Resolution,
    AggregateIgnoreDeclarationBytesPerSnapshot => Resolution,
    AggregateLineFragmentEvaluationBytesPerSnapshot => Resolution,
    AggregateHeadingAnchorEvaluationBytesPerSnapshot => Resolution,
    ProjectionAssertionsPerSnapshot => Resolution,
    AggregateProjectionSelectedBytesPerSnapshot => Resolution,
    ProjectionRecordsComparedPerSnapshot => Resolution,
    AggregateProjectionProjectedBytesPerSnapshot => Resolution,
    AggregateProjectionPreviewBytesPerSnapshot => Resolution,
    AggregateDocumentBytesPerSnapshot => Discovery,
    RawLinkDestinationBytes => Parse,
    ParserNesting => Parse,
    ParserNodesPerDocument => Parse,
    ParserNodesPerSnapshot => Parse,
    AggregateEmbeddedCodeEvaluationBytesPerSnapshot => Parse,
    ReferencesPerDocument => Parse,
    ReferencesPerSnapshot => Parse,
    DeclaredLabelsPerSnapshot => Parse,
    OrganizationPolicyEntries => Configuration,
    CompleteFindings => Policy,
    TypedAnalysisErrorsRetained => Internal,
    MachineJsonBytes => Output,
    PrivateTemporaryStorageBytes => Internal,
    EvaluatorManagedMemoryBytes => Internal,
}
