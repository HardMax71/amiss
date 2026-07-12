use std::fs::File;
use std::io::Read as _;
use std::os::fd::OwnedFd;
use std::path::Path;

use amiss_wire::model::{ObjectFormat, Oid};
use rustix::fs::{Mode, OFlags, openat};
use rustix::io::Errno;

use crate::Error;
use crate::object::{Object, ObjectKind, decode_loose, discard_to_unreadable};
use crate::resources::GitResources;

#[derive(Debug)]
pub struct Repository {
    objects: OwnedFd,
    object_format: ObjectFormat,
}

fn dir_flags() -> OFlags {
    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::DIRECTORY | OFlags::CLOEXEC
}

fn file_flags() -> OFlags {
    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC
}

fn unavailable(_errno: Errno) -> Error {
    Error::RepositoryUnavailable
}

impl Repository {
    /// Opens the primary non-bare form: the root's final entry and its direct
    /// `.git` child as no-follow directory handles; every later object access
    /// is relative to those handles.
    ///
    /// # Errors
    ///
    /// `RepositoryUnavailable` for a bare repository, `.git` file, symlink,
    /// or missing primary object database.
    pub fn open(root: &Path, object_format: ObjectFormat) -> Result<Self, Error> {
        let root_fd = rustix::fs::open(root, dir_flags(), Mode::empty()).map_err(unavailable)?;
        let git_fd = openat(&root_fd, ".git", dir_flags(), Mode::empty()).map_err(unavailable)?;
        let objects =
            openat(&git_fd, "objects", dir_flags(), Mode::empty()).map_err(unavailable)?;
        Ok(Self {
            objects,
            object_format,
        })
    }

    /// Total loose-first lookup for one full OID in the declared namespace.
    ///
    /// # Errors
    ///
    /// `ObjectMissing` when no loose or pack row holds the OID,
    /// `ObjectUnreadable` for any corruption or non-ordinary entry, and
    /// `ResourceLimit` for cap crossings; pack lookup is the next slice.
    pub fn read_object(&self, resources: &mut GitResources, oid: &Oid) -> Result<Object, Error> {
        let hex = oid.as_str();
        let fan = hex.get(..2).ok_or(Error::ObjectUnreadable)?;
        let rest = hex.get(2..).ok_or(Error::ObjectUnreadable)?;
        let fan_fd = match openat(&self.objects, fan, dir_flags(), Mode::empty()) {
            Ok(fd) => fd,
            Err(errno) if errno == Errno::NOENT => return Err(self.absent()),
            Err(_) => return Err(Error::ObjectUnreadable),
        };
        let file_fd = match openat(&fan_fd, rest, file_flags(), Mode::empty()) {
            Ok(fd) => fd,
            Err(errno) if errno == Errno::NOENT => return Err(self.absent()),
            Err(_) => return Err(Error::ObjectUnreadable),
        };
        self.decode(resources, oid, file_fd)
    }

    /// # Errors
    ///
    /// Everything `read_object` fails with, plus `ObjectWrongKind` when the
    /// reconstructed type differs from `expected`.
    pub fn read_expected(
        &self,
        resources: &mut GitResources,
        oid: &Oid,
        expected: ObjectKind,
    ) -> Result<Object, Error> {
        let object = self.read_object(resources, oid)?;
        if object.kind == expected {
            Ok(object)
        } else {
            Err(Error::ObjectWrongKind)
        }
    }

    fn decode(
        &self,
        resources: &mut GitResources,
        oid: &Oid,
        fd: OwnedFd,
    ) -> Result<Object, Error> {
        let file = File::from(fd);
        let metadata = file.metadata().map_err(discard_to_unreadable)?;
        if !metadata.file_type().is_file() {
            return Err(Error::ObjectUnreadable);
        }
        resources.charge_compressed(oid.as_str(), metadata.len())?;

        let stream_cap = resources.limits().compressed_stream_bytes;
        let mut compressed = Vec::new();
        let read = file
            .take(stream_cap.saturating_add(1))
            .read_to_end(&mut compressed)
            .map_err(discard_to_unreadable)?;
        if u64::try_from(read).unwrap_or(u64::MAX) > stream_cap {
            return Err(Error::ResourceLimit {
                resource: amiss_wire::controls::ResourceName::GitCompressedObjectBytes,
                configured_limit: stream_cap,
                observed_lower_bound: stream_cap.saturating_add(1),
            });
        }
        decode_loose(
            &compressed,
            self.object_format,
            oid,
            resources.limits().inflated_object_bytes,
        )
    }

    fn absent(&self) -> Error {
        match openat(&self.objects, "pack", dir_flags(), Mode::empty()) {
            Err(errno) if errno == Errno::NOENT => Error::ObjectMissing,
            Err(_) => Error::ObjectUnreadable,
            Ok(fd) => match rustix::fs::Dir::read_from(&fd) {
                Err(_) => Error::ObjectUnreadable,
                Ok(mut dir) => {
                    let has_entries = dir.any(|entry| {
                        entry.is_ok_and(|e| {
                            let name = e.file_name().to_bytes();
                            name != b"." && name != b".."
                        })
                    });
                    if has_entries {
                        Error::PackLookupUnimplemented
                    } else {
                        Error::ObjectMissing
                    }
                }
            },
        }
    }
}
