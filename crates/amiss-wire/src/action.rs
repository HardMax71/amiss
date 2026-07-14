use crate::controls::{ConstraintPlatform, root};
use crate::de::{self, Error, ErrorKind, Obj, fail};
use crate::json::canonical;
use crate::model::RepoPath;

/// The root `action.yml`: JCS JSON plus LF, using JSON's YAML-compatible
/// subset rather than a general YAML parser. Exactly `name`, `description`,
/// and `runs`; `runs` is exactly `{"main": <RepoPath>, "using": "node20"}`.
/// Anchors, aliases, tags, merge keys, `pre`, `post`, composite steps,
/// containers, and any other field are unrepresentable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionMetadata {
    pub name: String,
    pub description: String,
    pub main: RepoPath,
}

impl ActionMetadata {
    /// Parses the metadata blob: the bytes must be exactly
    /// `JCS(metadata) || LF`, so a reordered or reindented file is rejected
    /// before its content is read.
    ///
    /// # Errors
    ///
    /// A noncanonical encoding, a missing trailing newline, or any shape
    /// defect.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let trimmed = bytes
            .strip_suffix(b"\n")
            .ok_or_else(|| Error::new("$", ErrorKind::InvalidValue))?;
        let value = root(trimmed)?;
        if canonical(&value) != trimmed {
            return fail("$", ErrorKind::InvalidValue);
        }
        let mut obj = Obj::new("$", value)?;
        let name = de::string(&obj.field("name"), obj.take("name")?)?;
        let description = de::string(&obj.field("description"), obj.take("description")?)?;
        let runs_path = obj.field("runs");
        let mut runs = Obj::new(&runs_path, obj.take("runs")?)?;
        let main_path = runs.field("main");
        let main = RepoPath::new(de::string(&main_path, runs.take("main")?)?)
            .ok_or_else(|| Error::new(&main_path, ErrorKind::InvalidValue))?;
        de::const_str(&runs.field("using"), runs.take("using")?, "node20")?;
        runs.finish()?;
        obj.finish()?;
        if name.is_empty() || description.is_empty() {
            return fail("$", ErrorKind::InvalidValue);
        }
        Ok(Self {
            name,
            description,
            main,
        })
    }
}

/// The platform an executable's own header declares. The bootstrap derives
/// the target this way, from the protected artifact bytes, never from an
/// environment value or a candidate field, and requires equality with the
/// constraint, the manifest row, and the sandbox verification.
#[must_use]
pub fn executable_platform(bytes: &[u8]) -> Option<ConstraintPlatform> {
    elf_platform(bytes)
        .or_else(|| mach_o_platform(bytes))
        .or_else(|| pe_platform(bytes))
}

/// ELF: `7f 45 4c 46`, 64-bit little-endian only, with `e_machine` at offset
/// 18 naming `x86-64` (0x3e) or `AArch64` (0xb7).
fn elf_platform(bytes: &[u8]) -> Option<ConstraintPlatform> {
    if bytes.get(..4) != Some(&[0x7f, b'E', b'L', b'F']) {
        return None;
    }
    if bytes.get(4) != Some(&2) || bytes.get(5) != Some(&1) {
        return None;
    }
    match bytes.get(18..20)? {
        [0x3e, 0x00] => Some(ConstraintPlatform::LinuxX8664),
        [0xb7, 0x00] => Some(ConstraintPlatform::LinuxAarch64),
        _ => None,
    }
}

/// Mach-O: the 64-bit little-endian magic `cf fa ed fe`, with `cputype` at
/// offset 4 naming `x86-64` (0x01000007) or `ARM64` (0x0100000c).
fn mach_o_platform(bytes: &[u8]) -> Option<ConstraintPlatform> {
    if bytes.get(..4) != Some(&[0xcf, 0xfa, 0xed, 0xfe]) {
        return None;
    }
    match bytes.get(4..8)? {
        [0x07, 0x00, 0x00, 0x01] => Some(ConstraintPlatform::MacosX8664),
        [0x0c, 0x00, 0x00, 0x01] => Some(ConstraintPlatform::MacosAarch64),
        _ => None,
    }
}

/// PE: `MZ`, the PE signature at the offset stored at `0x3c`, and the COFF
/// machine field naming AMD64 (0x8664) or ARM64 (0xaa64).
fn pe_platform(bytes: &[u8]) -> Option<ConstraintPlatform> {
    if bytes.get(..2) != Some(b"MZ") {
        return None;
    }
    let header = bytes.get(0x3c..0x40)?;
    let [a, b, c, d] = header else { return None };
    let offset = usize::try_from(u32::from_le_bytes([*a, *b, *c, *d])).ok()?;
    if bytes.get(offset..offset.checked_add(4)?) != Some(b"PE\0\0") {
        return None;
    }
    let machine = offset.checked_add(4)?;
    match bytes.get(machine..machine.checked_add(2)?)? {
        [0x64, 0x86] => Some(ConstraintPlatform::WindowsX8664),
        [0x64, 0xaa] => Some(ConstraintPlatform::WindowsAarch64),
        _ => None,
    }
}

/// The platform of the running process, taken from the protected runtime's
/// own build target rather than any ambient value. `None` where the closed
/// six-value table has no row, which the bootstrap treats as unsupported.
#[must_use]
pub const fn host_platform() -> Option<ConstraintPlatform> {
    match (
        cfg!(target_os = "linux"),
        cfg!(target_os = "macos"),
        cfg!(target_os = "windows"),
    ) {
        (true, _, _) => arch(
            ConstraintPlatform::LinuxX8664,
            ConstraintPlatform::LinuxAarch64,
        ),
        (_, true, _) => arch(
            ConstraintPlatform::MacosX8664,
            ConstraintPlatform::MacosAarch64,
        ),
        (_, _, true) => arch(
            ConstraintPlatform::WindowsX8664,
            ConstraintPlatform::WindowsAarch64,
        ),
        _ => None,
    }
}

const fn arch(
    x86_64: ConstraintPlatform,
    aarch64: ConstraintPlatform,
) -> Option<ConstraintPlatform> {
    if cfg!(target_arch = "x86_64") {
        Some(x86_64)
    } else if cfg!(target_arch = "aarch64") {
        Some(aarch64)
    } else {
        None
    }
}
