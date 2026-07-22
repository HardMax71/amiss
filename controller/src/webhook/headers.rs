use crate::DeliveryHeader;

use super::WebhookError;

const MAX_HEADERS: usize = 128;
const MAX_HEADER_NAME_BYTES: usize = 128;
const MAX_HEADER_BYTES: usize = 32 * 1_024;

pub(super) struct Headers<'headers, 'value> {
    values: &'headers [DeliveryHeader<'value>],
}

impl<'headers, 'value> Headers<'headers, 'value> {
    pub(super) fn new(values: &'headers [DeliveryHeader<'value>]) -> Result<Self, WebhookError> {
        if values.len() > MAX_HEADERS {
            return Err(WebhookError::Headers);
        }
        let total = values.iter().try_fold(0_usize, |total, header| {
            if header.name.is_empty()
                || header.name.len() > MAX_HEADER_NAME_BYTES
                || !header.name.bytes().all(http_token)
            {
                return None;
            }
            let total = total
                .checked_add(header.name.len())?
                .checked_add(header.value.len())?;
            if total > MAX_HEADER_BYTES
                || header
                    .value
                    .iter()
                    .any(|byte| matches!(byte, 0 | b'\r' | b'\n'))
            {
                return None;
            }
            Some(total)
        });
        if total.is_none_or(|total| total > MAX_HEADER_BYTES) {
            return Err(WebhookError::Headers);
        }
        Ok(Self { values })
    }

    pub(super) fn exact(
        &self,
        expected_name: &str,
        max_value_bytes: usize,
    ) -> Result<&'value [u8], WebhookError> {
        let mut matches = self
            .values
            .iter()
            .filter(|header| header.name.eq_ignore_ascii_case(expected_name));
        let value = matches.next().ok_or(WebhookError::Headers)?.value;
        if matches.next().is_some() || value.is_empty() || value.len() > max_value_bytes {
            return Err(WebhookError::Headers);
        }
        Ok(value)
    }
}

const fn http_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}
