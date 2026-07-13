use std::fs::File;
use std::io::Read as _;
use std::os::fd::OwnedFd;
use std::path::Path;
use std::sync::OnceLock;

use amiss_wire::controls::ResourceName;
use amiss_wire::model::{ObjectFormat, Oid};
use rustix::fs::{Mode, OFlags, openat};
use rustix::io::Errno;

use crate::Error;
use crate::object::{Object, ObjectKind, decode_loose, discard_to_unreadable, hex, verify_oid};
use crate::pack::{
    self, PackSet, apply_delta, inflate_exact, kind_of, parse_entry_header, parse_ofs_distance,
};
use crate::resources::{GitResources, ValueCap, crossing};

#[derive(Debug)]
pub struct Repository {
    git_dir: OwnedFd,
    objects: OwnedFd,
    object_format: ObjectFormat,
    packs: OnceLock<Result<PackSet, Error>>,
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
            git_dir: git_fd,
            objects,
            object_format,
            packs: OnceLock::new(),
        })
    }

    /// Total loose-first lookup for one full OID in the declared namespace.
    ///
    /// # Errors
    ///
    /// `ObjectMissing` when no loose or validated pack row holds the OID,
    /// `ObjectUnreadable` for any corruption or non-ordinary entry, and
    /// `ResourceLimit` for cap crossings.
    pub fn read_object(&self, resources: &mut GitResources, oid: &Oid) -> Result<Object, Error> {
        self.read_full(resources, oid, 1, None)
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

    /// Reads one object under a smaller contextual inflated cap, which applies
    /// before the general Git object cap when a header declares a larger
    /// value. The cap binds the requested object only, never a delta base.
    ///
    /// # Errors
    ///
    /// Everything `read_expected` fails with; a declared size past the cap is
    /// a `ResourceLimit` carrying the cap's own resource.
    pub fn read_expected_capped(
        &self,
        resources: &mut GitResources,
        oid: &Oid,
        expected: ObjectKind,
        cap: ValueCap,
    ) -> Result<Object, Error> {
        let object = self.read_full(resources, oid, 1, Some(&cap))?;
        if object.kind == expected {
            Ok(object)
        } else {
            Err(Error::ObjectWrongKind)
        }
    }

    fn read_full(
        &self,
        resources: &mut GitResources,
        oid: &Oid,
        depth: u64,
        value_cap: Option<&ValueCap>,
    ) -> Result<Object, Error> {
        let limit = resources.limits().delta_depth;
        if depth > limit {
            return Err(crossing(ResourceName::GitDeltaDepth, limit, depth));
        }
        let hex_text = oid.as_str();
        let fan = hex_text.get(..2).ok_or(Error::ObjectUnreadable)?;
        let rest = hex_text.get(2..).ok_or(Error::ObjectUnreadable)?;
        let fan_fd = match openat(&self.objects, fan, dir_flags(), Mode::empty()) {
            Ok(fd) => fd,
            Err(errno) if errno == Errno::NOENT => {
                return self.read_packed(resources, oid, depth, value_cap);
            }
            Err(_) => return Err(Error::ObjectUnreadable),
        };
        let file_fd = match openat(&fan_fd, rest, file_flags(), Mode::empty()) {
            Ok(fd) => fd,
            Err(errno) if errno == Errno::NOENT => {
                return self.read_packed(resources, oid, depth, value_cap);
            }
            Err(_) => return Err(Error::ObjectUnreadable),
        };
        self.decode(resources, oid, file_fd, value_cap)
    }

    fn pack_set(&self, resources: &mut GitResources) -> Result<&PackSet, Error> {
        let built = self
            .packs
            .get_or_init(|| pack::build(&self.objects, self.object_format, resources));
        match built {
            Ok(set) => {
                for (name, size) in &set.index_sizes {
                    resources.charge_index(name, *size)?;
                }
                Ok(set)
            }
            Err(defect) => Err(defect.clone()),
        }
    }

    fn read_packed(
        &self,
        resources: &mut GitResources,
        oid: &Oid,
        depth: u64,
        value_cap: Option<&ValueCap>,
    ) -> Result<Object, Error> {
        let raw = oid_raw(oid).ok_or(Error::ObjectUnreadable)?;
        let Some((pack_index, offset)) = self.pack_set(resources)?.locate(&raw) else {
            return Err(Error::ObjectMissing);
        };
        let (kind, body) = self.read_pack_at(resources, pack_index, offset, depth, value_cap)?;
        let raw_header = format!("{} {}\0", kind.as_str(), body.len()).into_bytes();
        verify_oid(self.object_format, oid, &raw_header, &body)?;
        Ok(Object { kind, body })
    }

    fn read_pack_at(
        &self,
        resources: &mut GitResources,
        pack_index: usize,
        offset: u64,
        depth: u64,
        value_cap: Option<&ValueCap>,
    ) -> Result<(ObjectKind, Vec<u8>), Error> {
        let limit = resources.limits().delta_depth;
        if depth > limit {
            return Err(crossing(ResourceName::GitDeltaDepth, limit, depth));
        }
        let inflated_cap = resources.limits().inflated_object_bytes;
        let entry = {
            let set = self.pack_set(resources)?;
            let pack = set.packs.get(pack_index).ok_or(Error::ObjectUnreadable)?;
            pack.read_interval(resources, offset)?
        };
        let header = parse_entry_header(&entry)?;
        let after_header = entry
            .get(header.header_len..)
            .ok_or(Error::ObjectUnreadable)?;

        match header.type_code {
            1..=4 => {
                if let Some(value) = value_cap
                    && header.size > value.limit
                {
                    return Err(crossing(value.resource, value.limit, header.size));
                }
                let body = inflate_exact(after_header, header.size, inflated_cap)?;
                Ok((kind_of(header.type_code)?, body))
            }
            6 => {
                let (distance, used) = parse_ofs_distance(after_header)?;
                let base_offset = offset
                    .checked_sub(distance)
                    .ok_or(Error::ObjectUnreadable)?;
                let base_known = {
                    let set = self.pack_set(resources)?;
                    let pack = set.packs.get(pack_index).ok_or(Error::ObjectUnreadable)?;
                    pack.row_at(base_offset).is_some()
                };
                if !base_known {
                    return Err(Error::ObjectUnreadable);
                }
                let (kind, base) = self.read_pack_at(
                    resources,
                    pack_index,
                    base_offset,
                    depth.saturating_add(1),
                    None,
                )?;
                let script_bytes = after_header.get(used..).ok_or(Error::ObjectUnreadable)?;
                let script = inflate_exact(script_bytes, header.size, inflated_cap)?;
                Ok((kind, apply_delta(&base, &script, inflated_cap, value_cap)?))
            }
            7 => {
                let width = self.oid_width();
                let base_raw = after_header.get(..width).ok_or(Error::ObjectUnreadable)?;
                let base_oid =
                    Oid::new(self.object_format, hex(base_raw)).ok_or(Error::ObjectUnreadable)?;
                let base = self.read_full(resources, &base_oid, depth.saturating_add(1), None)?;
                let script_bytes = after_header.get(width..).ok_or(Error::ObjectUnreadable)?;
                let script = inflate_exact(script_bytes, header.size, inflated_cap)?;
                Ok((
                    base.kind,
                    apply_delta(&base.body, &script, inflated_cap, value_cap)?,
                ))
            }
            _ => Err(Error::ObjectUnreadable),
        }
    }

    #[must_use]
    pub const fn object_format(&self) -> ObjectFormat {
        self.object_format
    }

    /// Reads the current raw `.git/index` bytes through the retained handle:
    /// an ordinary no-follow entry, bounded by the raw staged-index cap with
    /// the exact declared length observed.
    ///
    /// # Errors
    ///
    /// `IndexInvalid` for a missing or non-ordinary entry, and the
    /// `git-index-bytes` crossing for an oversized one.
    pub fn read_index_bytes(&self, resources: &mut GitResources) -> Result<Vec<u8>, Error> {
        let fd = openat(&self.git_dir, "index", file_flags(), Mode::empty())
            .map_err(|_errno| Error::IndexInvalid)?;
        let file = File::from(fd);
        let metadata = file.metadata().map_err(|_defect| Error::IndexInvalid)?;
        if !metadata.file_type().is_file() {
            return Err(Error::IndexInvalid);
        }
        let cap = resources.limits().index_bytes;
        if metadata.len() > cap {
            return Err(crossing(ResourceName::GitIndexBytes, cap, metadata.len()));
        }
        let mut bytes = Vec::new();
        let read = file
            .take(cap.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|_defect| Error::IndexInvalid)?;
        if u64::try_from(read).unwrap_or(u64::MAX) > cap {
            return Err(crossing(
                ResourceName::GitIndexBytes,
                cap,
                cap.saturating_add(1),
            ));
        }
        Ok(bytes)
    }

    /// Whether the primary object database holds the OID, without reading or
    /// reconstructing the object.
    ///
    /// # Errors
    ///
    /// Pack enumeration defects and their resource crossings.
    pub fn has_object(&self, resources: &mut GitResources, oid: &Oid) -> Result<bool, Error> {
        let hex_text = oid.as_str();
        let fan = hex_text.get(..2).ok_or(Error::ObjectUnreadable)?;
        let rest = hex_text.get(2..).ok_or(Error::ObjectUnreadable)?;
        if let Ok(fan_fd) = openat(&self.objects, fan, dir_flags(), Mode::empty())
            && openat(&fan_fd, rest, file_flags(), Mode::empty()).is_ok()
        {
            return Ok(true);
        }
        let Some(raw) = oid_raw(oid) else {
            return Err(Error::ObjectUnreadable);
        };
        Ok(self.pack_set(resources)?.locate(&raw).is_some())
    }

    /// The end-of-scan race check: reopens the current index entry, rereads
    /// it boundedly, and accepts byte identity or an equal reparsed logical
    /// projection. Anything else is solely a snapshot change.
    ///
    /// # Errors
    ///
    /// `SnapshotChanged`, or the index byte crossing during the reread.
    pub fn verify_index_unchanged(
        &self,
        resources: &mut GitResources,
        initial: &[u8],
    ) -> Result<(), Error> {
        let current = match self.read_index_bytes(resources) {
            Ok(bytes) => bytes,
            Err(Error::ResourceLimit {
                resource,
                configured_limit,
                observed_lower_bound,
            }) => {
                return Err(Error::ResourceLimit {
                    resource,
                    configured_limit,
                    observed_lower_bound,
                });
            }
            Err(
                Error::RepositoryUnavailable
                | Error::ObjectMissing
                | Error::ObjectWrongKind
                | Error::ObjectUnreadable
                | Error::IndexInvalid
                | Error::IndexUnmerged
                | Error::IntentToAdd
                | Error::SnapshotChanged,
            ) => return Err(Error::SnapshotChanged),
        };
        if current == initial {
            return Ok(());
        }
        let before = crate::index::parse_index_file(self.object_format, initial)
            .map_err(|_defect| Error::SnapshotChanged)?;
        let after = crate::index::parse_index_file(self.object_format, &current)
            .map_err(|_defect| Error::SnapshotChanged)?;
        if before == after {
            return Ok(());
        }
        Err(Error::SnapshotChanged)
    }

    const fn oid_width(&self) -> usize {
        match self.object_format {
            ObjectFormat::Sha1 => 20,
            ObjectFormat::Sha256 => 32,
        }
    }

    fn decode(
        &self,
        resources: &mut GitResources,
        oid: &Oid,
        fd: OwnedFd,
        value_cap: Option<&ValueCap>,
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
            return Err(crossing(
                ResourceName::GitCompressedObjectBytes,
                stream_cap,
                stream_cap.saturating_add(1),
            ));
        }
        decode_loose(
            &compressed,
            self.object_format,
            oid,
            resources.limits().inflated_object_bytes,
            value_cap,
        )
    }
}

fn oid_raw(oid: &Oid) -> Option<Vec<u8>> {
    let text = oid.as_str();
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(text.len().checked_div(2)?);
    for pair in text.as_bytes().chunks_exact(2) {
        let [high, low] = pair else { return None };
        let value = |byte: u8| match byte {
            b'0'..=b'9' => Some(byte.wrapping_sub(b'0')),
            b'a'..=b'f' => Some(byte.wrapping_sub(b'a').wrapping_add(10)),
            _ => None,
        };
        out.push(value(*high)?.wrapping_shl(4) | value(*low)?);
    }
    Some(out)
}
