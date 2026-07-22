use serde::{Deserialize, Serialize};

use crate::{CheckConclusion, RunFailure};

macro_rules! define_stored_conclusion {
    (
        simple: [$($simple:ident),+ $(,)?],
        unavailable: [$($failure:ident),+ $(,)?]
    ) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "kebab-case")]
        pub(in crate::file_ledger::format) enum StoredRunFailure {
            $($failure),+
        }

        impl StoredRunFailure {
            const fn new(failure: RunFailure) -> Self {
                match failure {
                    $(RunFailure::$failure => Self::$failure),+
                }
            }

            const fn materialize(self) -> RunFailure {
                match self {
                    $(Self::$failure => RunFailure::$failure),+
                }
            }
        }

        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(tag = "conclusion", rename_all = "kebab-case", deny_unknown_fields)]
        pub(in crate::file_ledger::format) enum StoredConclusion {
            $($simple),+,
            Unavailable { failure: StoredRunFailure },
        }

        impl StoredConclusion {
            pub(in crate::file_ledger::format) const fn new(conclusion: CheckConclusion) -> Self {
                match conclusion {
                    $(CheckConclusion::$simple => Self::$simple),+,
                    CheckConclusion::Unavailable(failure) => Self::Unavailable {
                        failure: StoredRunFailure::new(failure),
                    },
                }
            }

            pub(in crate::file_ledger::format) const fn materialize(self) -> CheckConclusion {
                match self {
                    $(Self::$simple => CheckConclusion::$simple),+,
                    Self::Unavailable { failure } => {
                        CheckConclusion::Unavailable(failure.materialize())
                    }
                }
            }
        }
    };
}

define_stored_conclusion! {
    simple: [Pass, Block, Superseded],
    unavailable: [
        MissingOutput,
        Timeout,
        TamperedRuntime,
        Unavailable,
        OversizedOutput,
        WrongIdentity,
        WrongTree,
        AuthorizationRevoked,
        Closed,
    ]
}
