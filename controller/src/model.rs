use amiss_wire::model::{ObjectFormat, Oid};

/// Commit identity proven by the acquired Git objects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcquiredCommit {
    pub id: Oid,
    pub tree: Oid,
    pub parents: Vec<Oid>,
}

impl AcquiredCommit {
    pub fn has_format(&self, format: ObjectFormat) -> bool {
        [&self.id, &self.tree]
            .into_iter()
            .chain(&self.parents)
            .all(|oid| oid.object_format() == format)
    }
}
