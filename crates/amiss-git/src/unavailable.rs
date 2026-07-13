use std::convert::Infallible;
use std::path::Path;

use amiss_wire::model::{ObjectFormat, Oid};

use crate::Error;
use crate::object::{Object, ObjectKind};
use crate::resources::{GitResources, ValueCap};

/// The handle/no-follow repository boundary exists only where the platform
/// can enforce it. Everywhere else the contract's projection applies
/// verbatim: the repository is unavailable, never pathname traversal, so
/// this `Repository` cannot be constructed and `open` always reports
/// `RepositoryUnavailable`.
#[derive(Debug)]
pub struct Repository {
    never: Infallible,
}

impl Repository {
    /// # Errors
    ///
    /// Always `RepositoryUnavailable`: this platform cannot enforce the
    /// handle/no-follow boundary.
    pub fn open(_root: &Path, _object_format: ObjectFormat) -> Result<Self, Error> {
        Err(Error::RepositoryUnavailable)
    }

    #[must_use]
    pub fn object_format(&self) -> ObjectFormat {
        match self.never {}
    }

    /// # Errors
    ///
    /// Unreachable: no value of this type exists.
    pub fn read_object(&self, _resources: &mut GitResources, _oid: &Oid) -> Result<Object, Error> {
        match self.never {}
    }

    /// # Errors
    ///
    /// Unreachable: no value of this type exists.
    pub fn read_expected(
        &self,
        _resources: &mut GitResources,
        _oid: &Oid,
        _expected: ObjectKind,
    ) -> Result<Object, Error> {
        match self.never {}
    }

    /// # Errors
    ///
    /// Unreachable: no value of this type exists.
    pub fn read_expected_capped(
        &self,
        _resources: &mut GitResources,
        _oid: &Oid,
        _expected: ObjectKind,
        _cap: ValueCap,
    ) -> Result<Object, Error> {
        match self.never {}
    }

    /// # Errors
    ///
    /// Unreachable: no value of this type exists.
    pub fn read_index_bytes(&self, _resources: &mut GitResources) -> Result<Vec<u8>, Error> {
        match self.never {}
    }

    /// # Errors
    ///
    /// Unreachable: no value of this type exists.
    pub fn has_object(&self, _resources: &mut GitResources, _oid: &Oid) -> Result<bool, Error> {
        match self.never {}
    }

    /// # Errors
    ///
    /// Unreachable: no value of this type exists.
    pub fn verify_index_unchanged(
        &self,
        _resources: &mut GitResources,
        _initial: &[u8],
    ) -> Result<(), Error> {
        match self.never {}
    }
}
