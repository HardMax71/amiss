#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "hand-forged byte fixtures must fail loudly"
)]

use std::fs;
use std::io::Write as _;
use std::path::Path;

use amiss_git::{Error, GitLimits, GitResources, ObjectKind, Repository, ValueCap};
use amiss_wire::controls::ResourceName;
use amiss_wire::model::{ObjectFormat, Oid};
use flate2::Compression;
use flate2::write::ZlibEncoder;
use sha1_checked::Digest as _;
use tempfile::TempDir;

const BLOB: u8 = 3;
const OFS_DELTA: u8 = 6;

struct Entry {
    type_code: u8,
    payload: Vec<u8>,
    oid: [u8; 20],
    header_pad: usize,
    ofs_distance: Option<u64>,
    base_ref: Option<[u8; 20]>,
}

impl Entry {
    fn blob(payload: &[u8]) -> Self {
        Self {
            type_code: BLOB,
            payload: payload.to_vec(),
            oid: blob_oid(payload),
            header_pad: 0,
            ofs_distance: None,
            base_ref: None,
        }
    }
}

fn sha1(bytes: &[u8]) -> [u8; 20] {
    let mut hasher = sha1_checked::Sha1::builder()
        .detect_collision(false)
        .build();
    hasher.update(bytes);
    (*hasher.try_finalize().hash()).into()
}

fn blob_oid(payload: &[u8]) -> [u8; 20] {
    let mut framed = format!("blob {}\0", payload.len()).into_bytes();
    framed.extend_from_slice(payload);
    sha1(&framed)
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn trailer(pack: &[u8]) -> &[u8] {
    pack.get(pack.len().saturating_sub(20)..).unwrap()
}

fn entry_header(type_code: u8, size: u64, pad: usize) -> Vec<u8> {
    let mut first = type_code.wrapping_shl(4) | u8::try_from(size & 0x0f).unwrap();
    let mut rest = size.wrapping_shr(4);
    let mut chain: Vec<u8> = Vec::new();
    while rest > 0 {
        chain.push(u8::try_from(rest & 0x7f).unwrap());
        rest = rest.wrapping_shr(7);
    }
    chain.extend(std::iter::repeat_n(0_u8, pad));
    let mut out = Vec::new();
    if chain.is_empty() {
        out.push(first);
        return out;
    }
    first |= 0x80;
    out.push(first);
    let last = chain.len().saturating_sub(1);
    for (index, byte) in chain.iter().enumerate() {
        out.push(if index == last { *byte } else { byte | 0x80 });
    }
    out
}

fn ofs_distance(value: u64) -> Vec<u8> {
    let mut value = value;
    let mut bytes = vec![u8::try_from(value & 0x7f).unwrap()];
    value = value.wrapping_shr(7);
    while value > 0 {
        value = value.saturating_sub(1);
        bytes.push(u8::try_from(value & 0x7f).unwrap() | 0x80);
        value = value.wrapping_shr(7);
    }
    bytes.reverse();
    bytes
}

fn leb(value: u64, pad: usize) -> Vec<u8> {
    let mut value = value;
    let mut groups = vec![u8::try_from(value & 0x7f).unwrap()];
    value = value.wrapping_shr(7);
    while value > 0 {
        groups.push(u8::try_from(value & 0x7f).unwrap());
        value = value.wrapping_shr(7);
    }
    groups.extend(std::iter::repeat_n(0_u8, pad));
    let last = groups.len().saturating_sub(1);
    groups
        .iter()
        .enumerate()
        .map(|(index, byte)| if index == last { *byte } else { byte | 0x80 })
        .collect()
}

fn deflate(payload: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(payload).unwrap();
    encoder.finish().unwrap()
}

/// A delta script that ignores its base and inserts `target` literally.
fn insert_script(base_len: usize, target: &[u8], pad: usize) -> Vec<u8> {
    assert!(!target.is_empty() && target.len() <= 0x7f, "one literal op");
    let mut script = leb(u64::try_from(base_len).unwrap(), pad);
    script.extend_from_slice(&leb(u64::try_from(target.len()).unwrap(), 0));
    script.push(u8::try_from(target.len()).unwrap());
    script.extend_from_slice(target);
    script
}

fn write_pack(entries: &[Entry]) -> (Vec<u8>, Vec<u64>) {
    let mut pack = Vec::new();
    pack.extend_from_slice(b"PACK");
    pack.extend_from_slice(&2_u32.to_be_bytes());
    pack.extend_from_slice(&u32::try_from(entries.len()).unwrap().to_be_bytes());
    let mut offsets = Vec::with_capacity(entries.len());
    for entry in entries {
        let offset = u64::try_from(pack.len()).unwrap();
        offsets.push(offset);
        pack.extend_from_slice(&entry_header(
            entry.type_code,
            u64::try_from(entry.payload.len()).unwrap(),
            entry.header_pad,
        ));
        if let Some(distance) = entry.ofs_distance {
            pack.extend_from_slice(&ofs_distance(distance));
        }
        if let Some(base) = entry.base_ref {
            pack.extend_from_slice(&base);
        }
        pack.extend_from_slice(&deflate(&entry.payload));
    }
    let trailer = sha1(&pack);
    pack.extend_from_slice(&trailer);
    (pack, offsets)
}

fn fanout_for(rows: &[(u64, [u8; 20])]) -> Vec<u8> {
    let mut counts = [0_u32; 256];
    for (_, oid) in rows {
        let bucket = usize::from(*oid.first().unwrap());
        let slot = counts.get_mut(bucket).unwrap();
        *slot = slot.saturating_add(1);
    }
    let mut out = Vec::with_capacity(1024);
    let mut running = 0_u32;
    for count in counts {
        running = running.saturating_add(count);
        out.extend_from_slice(&running.to_be_bytes());
    }
    out
}

/// Rows must arrive in the order they should appear; the caller sorts, or
/// deliberately does not.
fn write_idx_v1(rows: &[(u64, [u8; 20])], pack: &[u8], fanout: Option<Vec<u8>>) -> Vec<u8> {
    let mut content = fanout.unwrap_or_else(|| fanout_for(rows));
    for (offset, oid) in rows {
        content.extend_from_slice(&u32::try_from(*offset).unwrap().to_be_bytes());
        content.extend_from_slice(oid);
    }
    content.extend_from_slice(trailer(pack));
    let digest = sha1(&content);
    content.extend_from_slice(&digest);
    content
}

/// An index v2 with one row routed through the large-offset table.
fn write_idx_v2_large(rows: &[(u64, [u8; 20])], pack: &[u8], data_end: u64) -> Vec<u8> {
    let mut content = Vec::new();
    content.extend_from_slice(&[0xff, b't', b'O', b'c']);
    content.extend_from_slice(&2_u32.to_be_bytes());
    content.extend_from_slice(&fanout_for(rows));
    for (_, oid) in rows {
        content.extend_from_slice(oid);
    }
    for (index, (offset, _)) in rows.iter().enumerate() {
        let end = rows
            .get(index.saturating_add(1))
            .map_or(data_end, |(next_offset, _)| *next_offset);
        let length = usize::try_from(end.checked_sub(*offset).unwrap()).unwrap();
        let start = usize::try_from(*offset).unwrap();
        let interval = pack.get(start..start.checked_add(length).unwrap()).unwrap();
        content.extend_from_slice(&crc32fast::hash(interval).to_be_bytes());
    }
    for index in 0..rows.len() {
        let row = u32::try_from(index).unwrap();
        content.extend_from_slice(&(0x8000_0000_u32 | row).to_be_bytes());
    }
    for (offset, _) in rows {
        content.extend_from_slice(&offset.to_be_bytes());
    }
    content.extend_from_slice(trailer(pack));
    let digest = sha1(&content);
    content.extend_from_slice(&digest);
    content
}

fn install(root: &Path, pack: &[u8], idx: &[u8]) {
    let name = to_hex(trailer(pack));
    let dir = root.join(".git/objects/pack");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(format!("pack-{name}.pack")), pack).unwrap();
    fs::write(dir.join(format!("pack-{name}.idx")), idx).unwrap();
}

fn forged_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    amiss_fixtures::init_repository(dir.path()).unwrap();
    dir
}

fn sorted_rows(entries: &[Entry], offsets: &[u64]) -> Vec<(u64, [u8; 20])> {
    let mut rows: Vec<(u64, [u8; 20])> = offsets
        .iter()
        .zip(entries)
        .map(|(offset, entry)| (*offset, entry.oid))
        .collect();
    rows.sort_by_key(|row| row.1);
    rows
}

fn read_with(root: &Path, oid: &[u8; 20], limits: GitLimits) -> Result<Vec<u8>, Error> {
    let repo = Repository::open(root, ObjectFormat::Sha1)?;
    let oid = Oid::new(ObjectFormat::Sha1, to_hex(oid)).unwrap();
    let mut resources = GitResources::new(limits);
    repo.read_expected(&mut resources, &oid, ObjectKind::Blob)
        .map(|object| object.body)
}

fn read(root: &Path, oid: &[u8; 20]) -> Result<Vec<u8>, Error> {
    read_with(root, oid, GitLimits::CONTRACT)
}

/// A payload whose blob oid starts with the wanted byte, found by nonce.
fn blob_with_first_byte(wanted: u8, salt: &str) -> Vec<u8> {
    (0..100_000_u32)
        .map(|nonce| format!("{salt} {nonce}\n").into_bytes())
        .find(|payload| blob_oid(payload).first() == Some(&wanted))
        .expect("a nonce reaches the bucket")
}

#[test]
fn a_forged_pack_reads_back_from_every_bucket_shape() {
    let dir = forged_repo();
    let zero = blob_with_first_byte(0x00, "zero bucket");
    let first = blob_with_first_byte(0xab, "shared bucket one");
    let second = blob_with_first_byte(0xab, "shared bucket two");
    let entries = [
        Entry::blob(&zero),
        Entry::blob(&first),
        Entry::blob(&second),
    ];
    let (pack, offsets) = write_pack(&entries);
    let idx = write_idx_v1(&sorted_rows(&entries, &offsets), &pack, None);
    install(dir.path(), &pack, &idx);

    assert_eq!(read(dir.path(), &entries[0].oid).unwrap(), zero);
    assert_eq!(read(dir.path(), &entries[1].oid).unwrap(), first);
    assert_eq!(read(dir.path(), &entries[2].oid).unwrap(), second);
}

#[test]
fn a_packed_tag_is_a_tag() {
    let dir = forged_repo();
    let payload = b"tagged object".to_vec();
    let mut preimage = format!("tag {}\0", payload.len()).into_bytes();
    preimage.extend_from_slice(&payload);
    let entry = Entry {
        type_code: 4,
        payload: payload.clone(),
        oid: sha1(&preimage),
        header_pad: 0,
        ofs_distance: None,
        base_ref: None,
    };
    let (pack, offsets) = write_pack(std::slice::from_ref(&entry));
    let idx = write_idx_v1(
        &sorted_rows(std::slice::from_ref(&entry), &offsets),
        &pack,
        None,
    );
    install(dir.path(), &pack, &idx);
    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    let mut res = GitResources::new(GitLimits::CONTRACT);
    let oid = Oid::new(ObjectFormat::Sha1, to_hex(&entry.oid)).unwrap();
    let object = repo.read_object(&mut res, &oid).unwrap();
    assert_eq!(object.kind, ObjectKind::Tag);
    assert_eq!(object.body, payload);
}

#[test]
fn an_empty_pack_is_exactly_its_frame() {
    let dir = forged_repo();
    let (pack, _) = write_pack(&[]);
    assert_eq!(pack.len(), 32, "twelve header bytes and one trailer");
    let idx = write_idx_v1(&[], &pack, None);
    install(dir.path(), &pack, &idx);

    let absent = [0x11_u8; 20];
    assert_eq!(read(dir.path(), &absent), Err(Error::ObjectMissing));
}

#[test]
fn the_index_byte_ceiling_is_inclusive() {
    let dir = forged_repo();
    let payload = b"index ceiling\n".to_vec();
    let entries = [Entry::blob(&payload)];
    let (pack, offsets) = write_pack(&entries);
    let idx = write_idx_v1(&sorted_rows(&entries, &offsets), &pack, None);
    let idx_len = u64::try_from(idx.len()).unwrap();
    install(dir.path(), &pack, &idx);

    ceiling_holds(
        dir.path(),
        &entries[0].oid,
        &payload,
        |value| GitLimits {
            pack_index_bytes: value,
            ..GitLimits::CONTRACT
        },
        idx_len,
        ResourceName::GitPackIndexBytes,
    );
}

/// At the ceiling the read settles whole; one byte under, it names `resource`.
fn ceiling_holds(
    root: &Path,
    oid: &[u8; 20],
    expected: &[u8],
    limits_at: impl Fn(u64) -> GitLimits,
    ceiling: u64,
    resource: ResourceName,
) {
    assert_eq!(read_with(root, oid, limits_at(ceiling)).unwrap(), expected);
    let under = limits_at(ceiling.checked_sub(1).unwrap());
    assert!(matches!(
        read_with(root, oid, under),
        Err(Error::ResourceLimit { resource: named, .. }) if named == resource
    ));
}

#[test]
fn the_pack_pair_ceiling_is_inclusive() {
    let dir = forged_repo();
    for salt in ["pair one\n", "pair two\n"] {
        let entries = [Entry::blob(salt.as_bytes())];
        let (pack, offsets) = write_pack(&entries);
        let idx = write_idx_v1(&sorted_rows(&entries, &offsets), &pack, None);
        install(dir.path(), &pack, &idx);
    }
    let probe = Entry::blob(b"pair one\n").oid;

    let exact = GitLimits {
        pack_files: 2,
        ..GitLimits::CONTRACT
    };
    assert!(read_with(dir.path(), &probe, exact).is_ok());

    let under = GitLimits {
        pack_files: 1,
        ..GitLimits::CONTRACT
    };
    assert!(matches!(
        read_with(dir.path(), &probe, under),
        Err(Error::ResourceLimit {
            resource: ResourceName::GitPackFiles,
            ..
        })
    ));
}

/// Three entries, the middle row poisoned, the last one read: its interval
/// stays exact, so only the load-time offset check separates refusal from a
/// clean read.
#[test]
fn a_poisoned_offset_refuses_the_whole_pack() {
    for poison in [5_u64, u64::MAX] {
        let dir = forged_repo();
        let last = b"stays readable\n".to_vec();
        let entries = [
            Entry::blob(b"first entry\n"),
            Entry::blob(b"gets poisoned\n"),
            Entry::blob(&last),
        ];
        let (pack, offsets) = write_pack(&entries);
        let data_end = u64::try_from(pack.len()).unwrap() - 20;
        let mut rows = sorted_rows(&entries, &offsets);
        let poisoned_oid = entries[1].oid;
        for row in &mut rows {
            if row.1 == poisoned_oid {
                row.0 = if poison == u64::MAX { data_end } else { poison };
            }
        }
        let idx = write_idx_v1(&rows, &pack, None);
        install(dir.path(), &pack, &idx);

        assert_eq!(
            read(dir.path(), &entries[2].oid),
            Err(Error::ObjectUnreadable),
            "a poisoned row for an unread object refuses the pack at {poison}"
        );
    }
}

#[test]
fn a_version_three_pack_reads_back() {
    let dir = forged_repo();
    let payload = b"version three\n".to_vec();
    let entries = [Entry::blob(&payload)];
    let (mut pack, offsets) = write_pack(&entries);
    *pack.get_mut(7).unwrap() = 3;
    pack.truncate(pack.len().saturating_sub(20));
    let resealed = sha1(&pack);
    pack.extend_from_slice(&resealed);
    let idx = write_idx_v1(&sorted_rows(&entries, &offsets), &pack, None);
    install(dir.path(), &pack, &idx);

    assert_eq!(read(dir.path(), &entries[0].oid).unwrap(), payload);
}

/// The dot entries the platform yields are filtered before counting, so a
/// limit of two admits exactly one pack pair.
#[test]
fn the_directory_entry_ceiling_counts_only_real_names() {
    let dir = forged_repo();
    let payload = b"counted exactly\n".to_vec();
    let entries = [Entry::blob(&payload)];
    let (pack, offsets) = write_pack(&entries);
    let idx = write_idx_v1(&sorted_rows(&entries, &offsets), &pack, None);
    install(dir.path(), &pack, &idx);

    ceiling_holds(
        dir.path(),
        &entries[0].oid,
        &payload,
        |value| GitLimits {
            pack_directory_entries: value,
            ..GitLimits::CONTRACT
        },
        2,
        ResourceName::GitPackDirectoryEntries,
    );
}

#[test]
fn a_junk_name_beside_a_pack_pair_is_ignored() {
    let dir = forged_repo();
    let payload = b"beside the junk\n".to_vec();
    let entries = [Entry::blob(&payload)];
    let (pack, offsets) = write_pack(&entries);
    let idx = write_idx_v1(&sorted_rows(&entries, &offsets), &pack, None);
    install(dir.path(), &pack, &idx);
    let pack_dir = dir.path().join(".git/objects/pack");
    fs::write(pack_dir.join("pack-abc.idx"), b"short stem\n").unwrap();
    fs::write(
        pack_dir.join(format!("pack-{}.idx", "z".repeat(40))),
        b"not hex\n",
    )
    .unwrap();

    assert_eq!(read(dir.path(), &entries[0].oid).unwrap(), payload);
}

#[test]
fn a_fanout_that_disowns_a_row_is_refused() {
    let dir = forged_repo();
    let first = blob_with_first_byte(0x40, "fanout one");
    let second = blob_with_first_byte(0x40, "fanout two");
    let entries = [Entry::blob(&first), Entry::blob(&second)];
    let (pack, offsets) = write_pack(&entries);
    let rows = sorted_rows(&entries, &offsets);
    let mut fanout = fanout_for(&rows);
    fanout[0x3f * 4 + 3] = 1;
    let idx = write_idx_v1(&rows, &pack, Some(fanout));
    install(dir.path(), &pack, &idx);

    assert_eq!(
        read(dir.path(), &entries[0].oid),
        Err(Error::ObjectUnreadable),
        "a fanout claiming an earlier row disowns the first oid"
    );
}

#[test]
fn an_unsorted_index_is_refused() {
    let dir = forged_repo();
    let first = blob_with_first_byte(0x40, "sorted one");
    let second = blob_with_first_byte(0x40, "sorted two");
    let entries = [Entry::blob(&first), Entry::blob(&second)];
    let (pack, offsets) = write_pack(&entries);
    let mut rows = sorted_rows(&entries, &offsets);
    rows.swap(0, 1);
    let fanout = fanout_for(&rows);
    let idx = write_idx_v1(&rows, &pack, Some(fanout));
    install(dir.path(), &pack, &idx);

    assert_eq!(
        read(dir.path(), &entries[0].oid),
        Err(Error::ObjectUnreadable)
    );
}

#[test]
fn a_large_offset_row_resolves_through_its_table() {
    let dir = forged_repo();
    let payload = b"large offset\n".to_vec();
    let entries = [Entry::blob(&payload)];
    let (pack, offsets) = write_pack(&entries);
    let data_end = u64::try_from(pack.len()).unwrap() - 20;
    let idx = write_idx_v2_large(&sorted_rows(&entries, &offsets), &pack, data_end);
    install(dir.path(), &pack, &idx);

    assert_eq!(read(dir.path(), &entries[0].oid).unwrap(), payload);
}

#[test]
fn a_padded_size_varint_is_bounded() {
    let tolerated = forged_repo();
    let payload = b"padded header\n".to_vec();
    let mut entry = Entry::blob(&payload);
    entry.header_pad = 1;
    let entries = [entry];
    let (pack, offsets) = write_pack(&entries);
    let idx = write_idx_v1(&sorted_rows(&entries, &offsets), &pack, None);
    install(tolerated.path(), &pack, &idx);
    assert_eq!(
        read(tolerated.path(), &entries[0].oid).unwrap(),
        payload,
        "one redundant continuation stays within the shift bound"
    );

    let refused = forged_repo();
    let mut entry = Entry::blob(&payload);
    entry.header_pad = 9;
    let entries = [entry];
    let (pack, offsets) = write_pack(&entries);
    let idx = write_idx_v1(&sorted_rows(&entries, &offsets), &pack, None);
    install(refused.path(), &pack, &idx);
    assert_eq!(
        read(refused.path(), &entries[0].oid),
        Err(Error::ObjectUnreadable),
        "a size varint padded past the shift bound is refused"
    );
}

fn delta_pair(base: &[u8], target: &[u8], leb_pad: usize) -> [Entry; 2] {
    let base_entry = Entry::blob(base);
    let script = insert_script(base.len(), target, leb_pad);
    let delta = Entry {
        type_code: OFS_DELTA,
        payload: script,
        oid: blob_oid(target),
        header_pad: 0,
        ofs_distance: None,
        base_ref: None,
    };
    [base_entry, delta]
}

fn install_delta(dir: &Path, base: &[u8], target: &[u8], leb_pad: usize) -> [u8; 20] {
    let mut entries = delta_pair(base, target, leb_pad);
    let (probe, base_offsets) = write_pack(&entries[..1]);
    let base_offset = *base_offsets.first().unwrap();
    let delta_offset = u64::try_from(probe.len()).unwrap().saturating_sub(20);
    entries[1].ofs_distance = Some(delta_offset.checked_sub(base_offset).unwrap());
    let (pack, offsets) = write_pack(&entries);
    let idx = write_idx_v1(&sorted_rows(&entries, &offsets), &pack, None);
    install(dir, &pack, &idx);
    entries[1].oid
}

#[test]
fn a_padded_delta_length_is_bounded() {
    let tolerated = forged_repo();
    let target = b"delta target\n";
    let oid = install_delta(tolerated.path(), b"delta base\n", target, 1);
    assert_eq!(
        read(tolerated.path(), &oid).unwrap(),
        target,
        "one redundant continuation stays within the shift bound"
    );

    let refused = forged_repo();
    let oid = install_delta(refused.path(), b"delta base\n", target, 10);
    assert_eq!(read(refused.path(), &oid), Err(Error::ObjectUnreadable));
}

#[test]
fn the_object_ceiling_is_exact() {
    let dir = forged_repo();
    let payload = b"exactly at the cap\n".to_vec();
    let entries = [Entry::blob(&payload)];
    let (pack, offsets) = write_pack(&entries);
    let idx = write_idx_v1(&sorted_rows(&entries, &offsets), &pack, None);
    install(dir.path(), &pack, &idx);
    let length = u64::try_from(payload.len()).unwrap();

    ceiling_holds(
        dir.path(),
        &entries[0].oid,
        &payload,
        |value| GitLimits {
            inflated_object_bytes: value,
            ..GitLimits::CONTRACT
        },
        length,
        ResourceName::GitObjectBytes,
    );
}

/// Copies the whole base and appends `suffix`, so the target outweighs both
/// the base and the script and can sit exactly on the object ceiling.
fn extend_script(base_len: usize, suffix: &[u8]) -> Vec<u8> {
    assert!((1..=0xff).contains(&base_len) && !suffix.is_empty() && suffix.len() <= 0x7f);
    let target_len = base_len.checked_add(suffix.len()).unwrap();
    let mut script = leb(u64::try_from(base_len).unwrap(), 0);
    script.extend_from_slice(&leb(u64::try_from(target_len).unwrap(), 0));
    script.push(0x80 | 0x10);
    script.push(u8::try_from(base_len).unwrap());
    script.push(u8::try_from(suffix.len()).unwrap());
    script.extend_from_slice(suffix);
    script
}

fn install_copy_delta(dir: &Path, base: &[u8], suffix: &[u8]) -> ([u8; 20], Vec<u8>) {
    let mut target = base.to_vec();
    target.extend_from_slice(suffix);
    let base_entry = Entry::blob(base);
    let mut entries = [
        base_entry,
        Entry {
            type_code: OFS_DELTA,
            payload: extend_script(base.len(), suffix),
            oid: blob_oid(&target),
            header_pad: 0,
            ofs_distance: None,
            base_ref: None,
        },
    ];
    let (probe, base_offsets) = write_pack(&entries[..1]);
    let base_offset = *base_offsets.first().unwrap();
    let delta_offset = u64::try_from(probe.len()).unwrap().saturating_sub(20);
    entries[1].ofs_distance = Some(delta_offset.checked_sub(base_offset).unwrap());
    let (pack, offsets) = write_pack(&entries);
    let idx = write_idx_v1(&sorted_rows(&entries, &offsets), &pack, None);
    install(dir, &pack, &idx);
    (entries[1].oid, target)
}

#[test]
fn the_delta_target_ceiling_is_exact() {
    let dir = forged_repo();
    let (oid, target) = install_copy_delta(dir.path(), b"copied base bytes\n", b"and a suffix\n");
    let length = u64::try_from(target.len()).unwrap();

    let exact = GitLimits {
        inflated_object_bytes: length,
        ..GitLimits::CONTRACT
    };
    assert_eq!(read_with(dir.path(), &oid, exact).unwrap(), target);

    let under = GitLimits {
        inflated_object_bytes: length - 1,
        ..GitLimits::CONTRACT
    };
    assert!(matches!(
        read_with(dir.path(), &oid, under),
        Err(Error::ResourceLimit { .. })
    ));
}

#[test]
fn the_delta_value_cap_is_exact() {
    let dir = forged_repo();
    let target = b"value capped delta\n";
    let oid = install_delta(dir.path(), b"value base\n", target, 0);
    let length = u64::try_from(target.len()).unwrap();

    let repo = Repository::open(dir.path(), ObjectFormat::Sha1).unwrap();
    let oid = Oid::new(ObjectFormat::Sha1, to_hex(&oid)).unwrap();
    let cap = |limit: u64| ValueCap {
        resource: ResourceName::DocumentBlobBytes,
        limit,
    };
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    assert_eq!(
        repo.read_expected_capped(&mut resources, &oid, ObjectKind::Blob, cap(length))
            .unwrap()
            .body,
        target
    );
    let mut resources = GitResources::new(GitLimits::CONTRACT);
    assert!(matches!(
        repo.read_expected_capped(&mut resources, &oid, ObjectKind::Blob, cap(length - 1)),
        Err(Error::ResourceLimit {
            resource: ResourceName::DocumentBlobBytes,
            ..
        })
    ));
}

#[cfg(unix)]
#[test]
fn an_unreadable_pack_directory_is_not_an_absent_one() {
    use std::os::unix::fs::PermissionsExt;

    let dir = forged_repo();
    let payload = b"behind the door\n".to_vec();
    let entries = [Entry::blob(&payload)];
    let (pack, offsets) = write_pack(&entries);
    let idx = write_idx_v1(&sorted_rows(&entries, &offsets), &pack, None);
    install(dir.path(), &pack, &idx);
    let pack_dir = dir.path().join(".git/objects/pack");
    fs::set_permissions(&pack_dir, fs::Permissions::from_mode(0o000)).unwrap();

    let outcome = read(dir.path(), &entries[0].oid);
    fs::set_permissions(&pack_dir, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        outcome,
        Err(Error::ObjectUnreadable),
        "an unreadable pack directory is an error, not an empty one"
    );
}

const REF_DELTA: u8 = 7;

#[test]
fn a_delta_against_a_named_base_reconstructs() {
    let dir = forged_repo();
    let base = b"the base object\n";
    let target = b"the target object\n";
    let base_entry = Entry::blob(base);
    let delta = Entry {
        type_code: REF_DELTA,
        payload: insert_script(base.len(), target, 0),
        oid: blob_oid(target),
        header_pad: 0,
        ofs_distance: None,
        base_ref: Some(base_entry.oid),
    };
    let entries = [base_entry, delta];
    let (pack, offsets) = write_pack(&entries);
    let idx = write_idx_v1(&sorted_rows(&entries, &offsets), &pack, None);
    install(dir.path(), &pack, &idx);

    let delta_oid = entries.get(1).map(|entry| entry.oid).unwrap();
    assert_eq!(read(dir.path(), &delta_oid).unwrap(), target);
}
