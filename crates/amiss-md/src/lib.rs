pub mod accounting;
pub mod corpus;
pub mod extract;
pub mod frontmatter;
mod js;
pub mod lines;
pub mod profile;

pub use accounting::charge;
pub use amiss_wire::extraction::{
    Analysis, AnalyzeError, BlockKind, Extraction, Fault, GovernedDefinition, Heading,
    HeadingAttribute, HeadingSource, Occurrence, Opaque, Transclusion, TransclusionKind,
    TransclusionRefusal, Work,
};
pub use extract::analyze;
pub use frontmatter::Region;
