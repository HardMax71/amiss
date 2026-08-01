amiss_fixtures::bounded_memory!();

use amiss_wire::action::{executable_platform, host_platform};
use amiss_wire::controls::ConstraintPlatform;

fn elf(machine: [u8; 2]) -> Vec<u8> {
    let mut bytes = vec![0x7f, b'E', b'L', b'F', 2, 1];
    bytes.resize(18, 0);
    bytes.extend_from_slice(&machine);
    bytes
}

fn mach_o(cputype: [u8; 4]) -> Vec<u8> {
    let mut bytes = vec![0xcf, 0xfa, 0xed, 0xfe];
    bytes.extend_from_slice(&cputype);
    bytes
}

fn pe(machine: [u8; 2]) -> Vec<u8> {
    let mut bytes = b"MZ".to_vec();
    bytes.resize(0x3c, 0);
    bytes.extend_from_slice(&[0x40, 0, 0, 0]);
    bytes.extend_from_slice(b"PE\0\0");
    bytes.extend_from_slice(&machine);
    bytes
}

#[test]
fn every_supported_header_names_its_platform() {
    let cases = [
        (elf([0x3e, 0x00]), ConstraintPlatform::LinuxX8664),
        (elf([0xb7, 0x00]), ConstraintPlatform::LinuxAarch64),
        (
            mach_o([0x07, 0x00, 0x00, 0x01]),
            ConstraintPlatform::MacosX8664,
        ),
        (
            mach_o([0x0c, 0x00, 0x00, 0x01]),
            ConstraintPlatform::MacosAarch64,
        ),
        (pe([0x64, 0x86]), ConstraintPlatform::WindowsX8664),
        (pe([0x64, 0xaa]), ConstraintPlatform::WindowsAarch64),
    ];
    for (bytes, platform) in cases {
        assert_eq!(executable_platform(&bytes), Some(platform), "{platform:?}");
    }
}

#[test]
fn one_wrong_header_fact_refuses_the_read() {
    let mut wrong_magic = elf([0x3e, 0x00]);
    wrong_magic[0] = 0x7e;
    let mut thirty_two_bit = elf([0x3e, 0x00]);
    thirty_two_bit[4] = 1;
    let mut big_endian = elf([0x3e, 0x00]);
    big_endian[5] = 2;
    let mut truncated_elf = elf([0x3e, 0x00]);
    truncated_elf.truncate(18);

    let big_endian_magic = {
        let mut bytes = mach_o([0x07, 0x00, 0x00, 0x01]);
        bytes[..4].copy_from_slice(&[0xfe, 0xed, 0xfa, 0xcf]);
        bytes
    };
    let mut truncated_mach_o = mach_o([0x07, 0x00, 0x00, 0x01]);
    truncated_mach_o.truncate(7);

    let mut wrong_stub = pe([0x64, 0x86]);
    wrong_stub[..2].copy_from_slice(b"ZM");
    let mut wrong_signature = pe([0x64, 0x86]);
    wrong_signature[0x43] = 1;
    let mut offset_past_the_end = pe([0x64, 0x86]);
    offset_past_the_end[0x3c] = 0xff;

    let cases: [(&str, Vec<u8>); 13] = [
        ("empty input", Vec::new()),
        ("elf magic off by one bit", wrong_magic),
        ("32-bit elf", thirty_two_bit),
        ("big-endian elf", big_endian),
        ("elf ends before its machine field", truncated_elf),
        ("unknown elf machine", elf([0x3e, 0x01])),
        ("big-endian mach-o magic", big_endian_magic),
        ("mach-o ends before its cputype", truncated_mach_o),
        ("32-bit mach-o cputype", mach_o([0x07, 0x00, 0x00, 0x00])),
        ("no mz stub", wrong_stub),
        ("wrong pe signature", wrong_signature),
        ("pe offset past the end", offset_past_the_end),
        ("unknown pe machine", pe([0x64, 0x87])),
    ];
    for (reason, bytes) in cases {
        assert_eq!(executable_platform(&bytes), None, "{reason}");
    }
}

#[test]
fn the_host_names_itself_from_its_own_build_target() {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    let expected = Some(ConstraintPlatform::LinuxX8664);
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    let expected = Some(ConstraintPlatform::LinuxAarch64);
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    let expected = Some(ConstraintPlatform::MacosX8664);
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let expected = Some(ConstraintPlatform::MacosAarch64);
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    let expected = Some(ConstraintPlatform::WindowsX8664);
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    let expected = Some(ConstraintPlatform::WindowsAarch64);
    #[cfg(not(all(
        any(target_os = "linux", target_os = "macos", target_os = "windows"),
        any(target_arch = "x86_64", target_arch = "aarch64")
    )))]
    let expected = None;

    assert_eq!(host_platform(), expected);
}
