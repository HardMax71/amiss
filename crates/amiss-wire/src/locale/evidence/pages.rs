use std::collections::BTreeMap;
use std::marker::PhantomData;

use serde::de::{DeserializeOwned, Error, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::de::ErrorKind;
use crate::digest::Digest;

use super::{LocaleTargetOrigin, LocaleTargetPage, PAGE_ITEMS_LIMIT};

pub(super) trait Page: Sized {
    type Row: DeserializeOwned;
    type Borrowed<'a>: Serialize
    where
        Self: 'a;

    fn split(row: Self::Row) -> (String, Self);
    fn row<'a>(key: &'a str, page: &'a Self) -> Self::Borrowed<'a>;
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SourceRow {
    key: String,
    resource_digest: Digest,
}

#[derive(Serialize)]
pub(super) struct SourceRef<'a> {
    key: &'a str,
    resource_digest: &'a Digest,
}

impl Page for Digest {
    type Row = SourceRow;
    type Borrowed<'a> = SourceRef<'a>;

    fn split(row: Self::Row) -> (String, Self) {
        (row.key, row.resource_digest)
    }

    fn row<'a>(key: &'a str, page: &'a Self) -> Self::Borrowed<'a> {
        SourceRef {
            key,
            resource_digest: page,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TargetRow {
    key: String,
    resource_digest: Digest,
    origin: LocaleTargetOrigin,
}

#[derive(Serialize)]
pub(super) struct TargetRef<'a> {
    key: &'a str,
    resource_digest: &'a Digest,
    origin: &'a LocaleTargetOrigin,
}

impl Page for LocaleTargetPage {
    type Row = TargetRow;
    type Borrowed<'a> = TargetRef<'a>;

    fn split(row: Self::Row) -> (String, Self) {
        (
            row.key,
            Self {
                resource_digest: row.resource_digest,
                origin: row.origin,
            },
        )
    }

    fn row<'a>(key: &'a str, page: &'a Self) -> Self::Borrowed<'a> {
        TargetRef {
            key,
            resource_digest: &page.resource_digest,
            origin: &page.origin,
        }
    }
}

pub(super) fn serialize<S: Serializer, T: Page>(
    pages: &BTreeMap<String, T>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(pages.iter().map(|(key, page)| T::row(key, page)))
}

pub(super) fn deserialize<'de, D: Deserializer<'de>, T: Page>(
    deserializer: D,
) -> Result<BTreeMap<String, T>, D::Error> {
    deserializer.deserialize_seq(Pages(PhantomData))
}

struct Pages<T>(PhantomData<T>);

impl<'de, T: Page> Visitor<'de> for Pages<T> {
    type Value = BTreeMap<String, T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a bounded array of pages in strictly increasing key order")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        if sequence
            .size_hint()
            .is_some_and(|length| length > PAGE_ITEMS_LIMIT)
        {
            return Err(A::Error::custom(ErrorKind::LimitExceeded));
        }
        let mut pages: BTreeMap<String, T> = BTreeMap::new();
        while let Some(row) = sequence.next_element::<T::Row>()? {
            if pages.len() >= PAGE_ITEMS_LIMIT {
                return Err(A::Error::custom(ErrorKind::LimitExceeded));
            }
            let (key, page) = T::split(row);
            if let Some((previous, _)) = pages.last_key_value() {
                match previous.cmp(&key) {
                    std::cmp::Ordering::Equal => {
                        return Err(A::Error::custom(ErrorKind::DuplicateMember));
                    }
                    std::cmp::Ordering::Greater => {
                        return Err(A::Error::custom(ErrorKind::UnsortedSet));
                    }
                    std::cmp::Ordering::Less => {}
                }
            }
            pages.insert(key, page);
        }
        Ok(pages)
    }
}
