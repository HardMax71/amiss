use serde::{Deserialize, Serialize};

const MIN_EPOCH_SECONDS: i64 = -62_167_219_200;
const MAX_EPOCH_SECONDS: i64 = 253_402_300_799;

/// Whole-second UTC instant; the fixed-width form makes lexicographic order
/// chronological, so ordering derives from the raw string.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UtcInstant(String);

impl UtcInstant {
    #[must_use]
    pub fn new(raw: String) -> Option<Self> {
        let bytes = raw.as_bytes();
        if bytes.len() != 20 {
            return None;
        }
        for (index, byte) in bytes.iter().enumerate() {
            let expected_digit = !matches!(index, 4 | 7 | 10 | 13 | 16 | 19);
            if expected_digit != byte.is_ascii_digit() {
                return None;
            }
        }
        if bytes.get(4) != Some(&b'-')
            || bytes.get(7) != Some(&b'-')
            || bytes.get(10) != Some(&b'T')
            || bytes.get(13) != Some(&b':')
            || bytes.get(16) != Some(&b':')
            || bytes.get(19) != Some(&b'Z')
        {
            return None;
        }
        let year = field(bytes, 0, 4)?;
        let month = field(bytes, 5, 2)?;
        let day = field(bytes, 8, 2)?;
        let hour = field(bytes, 11, 2)?;
        let minute = field(bytes, 14, 2)?;
        let second = field(bytes, 17, 2)?;
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        if hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        Some(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Builds the fixed UTC wire form from whole Unix seconds.
    #[must_use]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "the accepted year range bounds every civil-date term far inside i64"
    )]
    pub fn from_epoch_seconds(seconds: i64) -> Option<Self> {
        if !(MIN_EPOCH_SECONDS..=MAX_EPOCH_SECONDS).contains(&seconds) {
            return None;
        }
        let days = seconds.div_euclid(86_400);
        let day_seconds = seconds.rem_euclid(86_400);
        let shifted_days = days + 719_468;
        let era = shifted_days.div_euclid(146_097);
        let day_of_era = shifted_days - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let mut year = year_of_era + era * 400;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month_piece = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * month_piece + 2) / 5 + 1;
        let month = month_piece + if month_piece < 10 { 3 } else { -9 };
        if month <= 2 {
            year += 1;
        }
        let hour = day_seconds / 3_600;
        let minute = day_seconds % 3_600 / 60;
        let second = day_seconds % 60;
        Self::new(format!(
            "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
        ))
    }

    /// Whole seconds since 1970-01-01T00:00:00Z, computed from the already
    /// validated calendar fields with the days-from-civil identity.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "every field is bounded by the validated 20-byte format, so all terms stay far inside i64"
    )]
    #[must_use]
    pub fn epoch_seconds(&self) -> i64 {
        let bytes = self.0.as_bytes();
        let part = |start: usize, len: usize| i64::from(field(bytes, start, len).unwrap_or(0));
        let year = part(0, 4);
        let month = part(5, 2);
        let day = part(8, 2);
        let shifted_year = if month <= 2 { year - 1 } else { year };
        let era = shifted_year.div_euclid(400);
        let year_of_era = shifted_year - era * 400;
        let month_index = if month > 2 { month - 3 } else { month + 9 };
        let day_of_year = (153 * month_index + 2) / 5 + day - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        let days = era * 146_097 + day_of_era - 719_468;
        days * 86_400 + part(11, 2) * 3_600 + part(14, 2) * 60 + part(17, 2)
    }
}

fn field(bytes: &[u8], start: usize, len: usize) -> Option<u32> {
    let end = start.checked_add(len)?;
    bytes.get(start..end)?.iter().try_fold(0_u32, |acc, byte| {
        let digit = u32::from(byte.wrapping_sub(b'0'));
        acc.checked_mul(10)?.checked_add(digit)
    })
}

fn days_in_month(year: u32, month: u32) -> u32 {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 0,
    }
}
