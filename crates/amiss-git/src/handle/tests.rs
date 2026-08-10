#![cfg(test)]

use std::io;

use super::read_full;

fn stepped(data: &'static [u8], step: usize) -> impl FnMut(&mut [u8], u64) -> io::Result<usize> {
    move |slice, at| {
        let at = usize::try_from(at).map_err(|_over| io::Error::other("offset"))?;
        let end = data.len().min(at.saturating_add(step.min(slice.len())));
        let chunk = data.get(at..end).unwrap_or(&[]);
        slice
            .get_mut(..chunk.len())
            .ok_or_else(|| io::Error::other("slice"))?
            .copy_from_slice(chunk);
        Ok(chunk.len())
    }
}

#[test]
fn fills_exactly_across_partial_reads() {
    let mut buf = [0_u8; 7];
    read_full(stepped(b"0123456789", 3), &mut buf, 2).expect("the fill completes");
    assert_eq!(&buf, b"2345678");
}

#[test]
fn refuses_end_of_file_short_of_the_length() {
    let mut buf = [0_u8; 8];
    let defect = read_full(stepped(b"0123", 4), &mut buf, 0).unwrap_err();
    assert_eq!(defect.kind(), io::ErrorKind::UnexpectedEof);
}

#[test]
fn retries_an_interruption_and_still_fills() {
    let mut interruptions = 1_u8;
    let mut inner = stepped(b"abcd", 4);
    let reader = move |slice: &mut [u8], at: u64| {
        if interruptions > 0 {
            interruptions -= 1;
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        inner(slice, at)
    };
    let mut buf = [0_u8; 4];
    read_full(reader, &mut buf, 0).expect("one interruption is retried");
    assert_eq!(&buf, b"abcd");
}

#[test]
fn surfaces_a_real_error_unretried() {
    let mut buf = [0_u8; 4];
    let defect = read_full(
        |_slice, _at| Err(io::Error::from(io::ErrorKind::PermissionDenied)),
        &mut buf,
        0,
    )
    .unwrap_err();
    assert_eq!(defect.kind(), io::ErrorKind::PermissionDenied);
}

#[test]
fn refuses_an_offset_past_the_addressable_range() {
    let mut buf = [0_u8; 2];
    let defect = read_full(
        |slice, _at| {
            if let Some(byte) = slice.first_mut() {
                *byte = 1;
            }
            Ok(1)
        },
        &mut buf,
        u64::MAX,
    )
    .unwrap_err();
    assert_eq!(defect.kind(), io::ErrorKind::InvalidInput);
}
