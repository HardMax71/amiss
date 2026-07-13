use std::fs::File;
use std::io::Read as _;
use std::path::Path;
use std::sync::OnceLock;

use amiss_wire::controls::ResourceName;
use amiss_wire::model::{ObjectFormat, Oid};

use crate::Error;
use crate::handle::{open_dir, open_file, open_root};
use crate::object::{Object, ObjectKind, decode_loose, discard_to_unreadable, hex, verify_oid};
use crate::pack::{
    self, PackSet, apply_delta, inflate_exact, kind_of, parse_entry_header, parse_ofs_distance,
};
use crate::resources::{GitResources, ValueCap, crossing};

#[derive(Debug)]
pub struct Repository {
    git_dir: File,
    objects: File,
    object_format: ObjectFormat,
    packs: OnceLock<Result<PackSet, Error>>,
    repacked: OnceLock<Result<PackSet, Error>>,
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
        let root_dir = open_root(root)?;
        let git_dir =
            open_dir(&root_dir, ".git").map_err(|_defect| Error::RepositoryUnavailable)?;
        let objects =
            open_dir(&git_dir, "objects").map_err(|_defect| Error::RepositoryUnavailable)?;
        Ok(Self {
            git_dir,
            objects,
            object_format,
            packs: OnceLock::new(),
            repacked: OnceLock::new(),
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
        let Some(fan_dir) = absent_to_none(open_dir(&self.objects, fan))? else {
            return self.read_packed(resources, oid, depth, value_cap);
        };
        let Some(loose) = absent_to_none(open_file(&fan_dir, rest))? else {
            return self.read_packed(resources, oid, depth, value_cap);
        };
        self.decode(resources, oid, loose, value_cap)
    }

    /// One pack enumeration, with its index sizes charged. Charging is by pack
    /// name and idempotent, so a second enumeration pays only for the packs the
    /// first one never saw.
    fn enumeration<'set>(
        &'set self,
        cell: &'set OnceLock<Result<PackSet, Error>>,
        resources: &mut GitResources,
    ) -> Result<&'set PackSet, Error> {
        let built = cell.get_or_init(|| pack::build(&self.objects, self.object_format, resources));
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

    /// Locates a packed OID, re-reading the pack directory once when the first
    /// enumeration misses.
    ///
    /// A concurrent repack moves an object out of the loose store and into a
    /// pack that did not exist when the first enumeration ran, which leaves a
    /// present object looking absent. Git re-reads its pack directory on a miss
    /// for exactly this reason, and so does this. The re-read goes through the
    /// same held objects handle, so the no-follow boundary is unchanged, and
    /// anything it finds is still content-addressed before use. A race can
    /// therefore cost one extra directory read, and can never yield another
    /// object's bytes. Exactly one re-read happens per repository, so a store
    /// that is genuinely missing an object still settles as missing.
    fn locate_packed(
        &self,
        resources: &mut GitResources,
        raw: &[u8],
    ) -> Result<Option<(&PackSet, usize, u64)>, Error> {
        let enumerated = self.enumeration(&self.packs, resources)?;
        if let Some((pack_index, offset)) = enumerated.locate(raw) {
            return Ok(Some((enumerated, pack_index, offset)));
        }
        let repacked = self.enumeration(&self.repacked, resources)?;
        Ok(repacked
            .locate(raw)
            .map(|(pack_index, offset)| (repacked, pack_index, offset)))
    }

    fn read_packed(
        &self,
        resources: &mut GitResources,
        oid: &Oid,
        depth: u64,
        value_cap: Option<&ValueCap>,
    ) -> Result<Object, Error> {
        let raw = oid_raw(oid).ok_or(Error::ObjectUnreadable)?;
        let Some((set, pack_index, offset)) = self.locate_packed(resources, &raw)? else {
            return Err(Error::ObjectMissing);
        };
        let (kind, body) =
            self.read_pack_at(resources, set, pack_index, offset, depth, value_cap)?;
        let raw_header = format!("{} {}\0", kind.as_str(), body.len()).into_bytes();
        verify_oid(self.object_format, oid, &raw_header, &body)?;
        Ok(Object { kind, body })
    }

    fn read_pack_at(
        &self,
        resources: &mut GitResources,
        set: &PackSet,
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
                    let pack = set.packs.get(pack_index).ok_or(Error::ObjectUnreadable)?;
                    pack.row_at(base_offset).is_some()
                };
                if !base_known {
                    return Err(Error::ObjectUnreadable);
                }
                let (kind, base) = self.read_pack_at(
                    resources,
                    set,
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
        let file = open_file(&self.git_dir, "index").map_err(|_defect| Error::IndexInvalid)?;
        let metadata = file.metadata().map_err(|_defect| Error::IndexInvalid)?;
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
        if let Ok(fan_dir) = open_dir(&self.objects, fan)
            && open_file(&fan_dir, rest).is_ok()
        {
            return Ok(true);
        }
        let Some(raw) = oid_raw(oid) else {
            return Err(Error::ObjectUnreadable);
        };
        Ok(self.locate_packed(resources, &raw)?.is_some())
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
        file: File,
        value_cap: Option<&ValueCap>,
    ) -> Result<Object, Error> {
        let metadata = file.metadata().map_err(discard_to_unreadable)?;
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

/// An entry that is simply absent falls through to the pack lookup; a
/// present but refused entry (a symlink, a directory where a file belongs) is
/// an unreadable object, never a silent miss.
fn absent_to_none(opened: std::io::Result<File>) -> Result<Option<File>, Error> {
    match opened {
        Ok(file) => Ok(Some(file)),
        Err(defect) if defect.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_defect) => Err(Error::ObjectUnreadable),
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
