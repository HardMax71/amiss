use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read as _;

use amiss_wire::controls::ResourceName;
use amiss_wire::model::ObjectFormat;

use crate::Error;
use crate::handle::{names, open_dir, open_file, read_exact_at};
use crate::object::discard_to_unreadable;
use crate::resources::{GitResources, crossing};

mod entry;
mod index;

pub(crate) use entry::{
    EntryKind, apply_delta, inflate_exact, parse_entry_header, parse_ofs_distance,
};
use index::parse_index;

#[derive(Debug)]
pub(crate) struct PackSet {
    pub(crate) packs: Vec<Pack>,
    pub(crate) index_sizes: Vec<(String, u64)>,
}

#[derive(Debug)]
pub(crate) struct Pack {
    pub(crate) name_hex: String,
    file: File,
    width: usize,
    oids: Vec<u8>,
    fanout_bounds: [u32; 257],
    offset_rows: Vec<usize>,
    offsets: Vec<u64>,
    crcs: Option<Vec<u32>>,
    data_end: u64,
}

fn oid_width(object_format: ObjectFormat) -> usize {
    match object_format {
        ObjectFormat::Sha1 => 20,
        ObjectFormat::Sha256 => 32,
    }
}

pub(crate) fn build(
    objects: &File,
    object_format: ObjectFormat,
    resources: &mut GitResources,
    known: Option<&PackSet>,
) -> Result<PackSet, Error> {
    let mut pack_dir = match open_dir(objects, "pack") {
        Ok(dir) => dir,
        Err(defect) if defect.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PackSet {
                packs: Vec::new(),
                index_sizes: Vec::new(),
            });
        }
        Err(_defect) => return Err(Error::ObjectUnreadable),
    };

    let limits = resources.limits();
    let mut entries: Vec<Vec<u8>> = Vec::new();
    let mut seen: u64 = 0;
    for name in names(&mut pack_dir).map_err(discard_to_unreadable)? {
        seen = seen.saturating_add(1);
        if seen > limits.pack_directory_entries {
            return Err(crossing(
                ResourceName::GitPackDirectoryEntries,
                limits.pack_directory_entries,
                seen,
            ));
        }
        entries.push(name.into_bytes());
    }
    entries.sort_unstable();

    let hex_len = oid_width(object_format).saturating_mul(2);
    let mut pairs: BTreeMap<String, (bool, bool)> = BTreeMap::new();
    for name in &entries {
        let Some((hex_part, is_pack)) = classify(name, hex_len) else {
            continue;
        };
        let slot = pairs.entry(hex_part).or_insert((false, false));
        if is_pack {
            slot.0 = true;
        } else {
            slot.1 = true;
        }
    }
    if pairs.values().any(|(pack, idx)| !(*pack && *idx)) {
        return Err(Error::ObjectUnreadable);
    }
    let pair_count = u64::try_from(pairs.len()).unwrap_or(u64::MAX);
    if pair_count > limits.pack_files {
        return Err(crossing(
            ResourceName::GitPackFiles,
            limits.pack_files,
            pair_count,
        ));
    }

    let mut packs = Vec::new();
    let mut index_sizes = Vec::new();
    debug_assert!(known.is_none_or(|set| {
        set.packs
            .is_sorted_by(|left, right| left.name_hex < right.name_hex)
    }));
    for name_hex in pairs.keys() {
        if known.is_some_and(|set| {
            set.packs
                .binary_search_by(|pack| pack.name_hex.as_str().cmp(name_hex))
                .is_ok()
        }) {
            continue;
        }
        let (pack, index_bytes) = load_pack(&pack_dir, object_format, resources, name_hex)?;
        index_sizes.push((name_hex.clone(), index_bytes));
        packs.push(pack);
    }
    Ok(PackSet { packs, index_sizes })
}

fn classify(name: &[u8], hex_len: usize) -> Option<(String, bool)> {
    let rest = name.strip_prefix(b"pack-")?;
    let (hex_part, suffix) = match rest.strip_suffix(b".pack") {
        Some(stem) => (stem, true),
        None => (rest.strip_suffix(b".idx")?, false),
    };
    if hex_part.len() != hex_len
        || !hex_part
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    {
        return None;
    }
    let text = std::str::from_utf8(hex_part).ok()?;
    Some((text.to_owned(), suffix))
}

fn load_pack(
    pack_dir: &File,
    object_format: ObjectFormat,
    resources: &mut GitResources,
    name_hex: &str,
) -> Result<(Pack, u64), Error> {
    let idx_file =
        open_file(pack_dir, &format!("pack-{name_hex}.idx")).map_err(discard_to_unreadable)?;
    let idx_meta = idx_file.metadata().map_err(discard_to_unreadable)?;
    resources.charge_index(name_hex, idx_meta.len())?;
    let mut idx_bytes = Vec::new();
    let cap = resources.limits().pack_index_bytes;
    let read = idx_file
        .take(cap.saturating_add(1))
        .read_to_end(&mut idx_bytes)
        .map_err(discard_to_unreadable)?;
    if u64::try_from(read).unwrap_or(u64::MAX) > cap {
        return Err(crossing(
            ResourceName::GitPackIndexBytes,
            cap,
            cap.saturating_add(1),
        ));
    }
    let parsed = parse_index(&idx_bytes, object_format)?;

    let file =
        open_file(pack_dir, &format!("pack-{name_hex}.pack")).map_err(discard_to_unreadable)?;
    let meta = file.metadata().map_err(discard_to_unreadable)?;
    let width = oid_width(object_format);
    let trailer = u64::try_from(width).unwrap_or(u64::MAX);
    let size = meta.len();
    if size < 12_u64.saturating_add(trailer) {
        return Err(Error::ObjectUnreadable);
    }

    let mut header = [0_u8; 12];
    read_exact_at(&file, &mut header, 0).map_err(discard_to_unreadable)?;
    let (magic, rest) = header.split_at(4);
    let (version, count_bytes) = rest.split_at(4);
    if magic != b"PACK" {
        return Err(Error::ObjectUnreadable);
    }
    let version = u32::from_be_bytes(version.try_into().map_err(discard_to_unreadable)?);
    if version != 2 && version != 3 {
        return Err(Error::ObjectUnreadable);
    }
    let count = u32::from_be_bytes(count_bytes.try_into().map_err(discard_to_unreadable)?);
    if usize::try_from(count).map_err(discard_to_unreadable)? != parsed.offsets.len() {
        return Err(Error::ObjectUnreadable);
    }

    let mut trailer_bytes = vec![0_u8; width];
    read_exact_at(&file, &mut trailer_bytes, size.saturating_sub(trailer))
        .map_err(discard_to_unreadable)?;
    if trailer_bytes != parsed.stored_pack_checksum {
        return Err(Error::ObjectUnreadable);
    }
    let name_raw = decode_hex(name_hex).ok_or(Error::ObjectUnreadable)?;
    if name_raw != trailer_bytes {
        return Err(Error::ObjectUnreadable);
    }

    let data_end = size.saturating_sub(trailer);
    let mut offset_rows: Vec<usize> = (0..parsed.offsets.len()).collect();
    offset_rows.sort_unstable_by_key(|row| parsed.offsets.get(*row).copied());
    let mut previous_offset = None;
    for row in &offset_rows {
        let offset = *parsed.offsets.get(*row).ok_or(Error::ObjectUnreadable)?;
        if offset < 12 || offset >= data_end || previous_offset == Some(offset) {
            return Err(Error::ObjectUnreadable);
        }
        previous_offset = Some(offset);
    }

    Ok((
        Pack {
            name_hex: name_hex.to_owned(),
            file,
            width,
            oids: parsed.oids,
            fanout_bounds: parsed.fanout_bounds,
            offset_rows,
            offsets: parsed.offsets,
            crcs: parsed.crcs,
            data_end,
        },
        idx_meta.len(),
    ))
}

impl PackSet {
    pub(crate) fn locate(&self, oid_raw: &[u8]) -> Option<(usize, u64)> {
        for (pack_index, pack) in self.packs.iter().enumerate() {
            if let Some(row) = pack.find(oid_raw) {
                let offset = *pack.offsets.get(row)?;
                return Some((pack_index, offset));
            }
        }
        None
    }
}

impl Pack {
    fn find(&self, oid_raw: &[u8]) -> Option<usize> {
        if oid_raw.len() != self.width {
            return None;
        }
        let bucket = usize::from(*oid_raw.first()?);
        let mut low = usize::try_from(*self.fanout_bounds.get(bucket)?).ok()?;
        let mut high = usize::try_from(*self.fanout_bounds.get(bucket.saturating_add(1))?).ok()?;
        while low < high {
            let middle = low.midpoint(high);
            let start = middle.saturating_mul(self.width);
            let candidate = self.oids.get(start..start.saturating_add(self.width))?;
            match candidate.cmp(oid_raw) {
                std::cmp::Ordering::Less => low = middle.saturating_add(1),
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => return Some(middle),
            }
        }
        None
    }

    pub(crate) fn row_at(&self, offset: u64) -> Option<usize> {
        let position = self
            .offset_rows
            .binary_search_by(|row| self.offsets.get(*row).cmp(&Some(&offset)))
            .ok()?;
        self.offset_rows.get(position).copied()
    }

    pub(crate) fn read_interval(
        &self,
        resources: &mut GitResources,
        offset: u64,
    ) -> Result<Vec<u8>, Error> {
        let position = self
            .offset_rows
            .binary_search_by(|row| self.offsets.get(*row).cmp(&Some(&offset)))
            .map_err(|_missing| Error::ObjectUnreadable)?;
        let row = *self
            .offset_rows
            .get(position)
            .ok_or(Error::ObjectUnreadable)?;
        let end = match self.offset_rows.get(position.saturating_add(1)) {
            Some(next_row) => *self.offsets.get(*next_row).ok_or(Error::ObjectUnreadable)?,
            None => self.data_end,
        };
        let length = end.checked_sub(offset).ok_or(Error::ObjectUnreadable)?;
        let member = format!("pack:{}:{offset}", self.name_hex);
        resources.charge_compressed(&member, length)?;
        let mut bytes = vec![0_u8; usize::try_from(length).map_err(discard_to_unreadable)?];
        read_exact_at(&self.file, &mut bytes, offset).map_err(discard_to_unreadable)?;
        if let Some(crcs) = &self.crcs {
            let expected = *crcs.get(row).ok_or(Error::ObjectUnreadable)?;
            if crc32fast::hash(&bytes) != expected {
                return Err(Error::ObjectUnreadable);
            }
        }
        Ok(bytes)
    }
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(text.len().checked_div(2)?);
    for pair in text.as_bytes().chunks_exact(2) {
        let [high, low] = pair else { return None };
        out.push(hex_value(*high)?.wrapping_shl(4) | hex_value(*low)?);
    }
    Some(out)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte.wrapping_sub(b'0')),
        b'a'..=b'f' => Some(byte.wrapping_sub(b'a').wrapping_add(10)),
        _ => None,
    }
}
