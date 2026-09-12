use std::collections::BTreeSet;
use std::fmt;

use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};

use super::{Error, ErrorKind};

/// The integer-only wire profile, independent of each document's typed schema.
pub struct JsonProfile;

impl JsonProfile {
    /// # Errors
    /// Refuses invalid JSON, duplicate keys, unsafe numbers, or over 512 nested containers.
    pub fn validate(bytes: &[u8]) -> Result<(), Error> {
        let text = std::str::from_utf8(bytes)
            .map_err(|defect| Error::new("$", ErrorKind::Json(defect.to_string())))?;
        let mut deserializer = serde_json::Deserializer::from_str(text);
        deserializer.disable_recursion_limit();
        Depth(0)
            .deserialize(&mut deserializer)
            .and_then(|()| deserializer.end())
            .map_err(|defect| Error::new("$", ErrorKind::Json(defect.to_string())))
    }
}

struct Depth(usize);

impl<'de> DeserializeSeed<'de> for Depth {
    type Value = ();

    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Depth {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value with safe integers and unique object keys")
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
        Ok(())
    }

    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<(), E> {
        js_int::Int::try_from(value).map(|_| ()).map_err(E::custom)
    }

    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<(), E> {
        js_int::UInt::try_from(value).map(|_| ()).map_err(E::custom)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<(), A::Error> {
        if self.0 >= 512 {
            return Err(A::Error::custom("nesting limit exceeded"));
        }
        while sequence
            .next_element_seed(Depth(self.0.saturating_add(1)))?
            .is_some()
        {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut object: A) -> Result<(), A::Error> {
        if self.0 >= 512 {
            return Err(A::Error::custom("nesting limit exceeded"));
        }
        let mut keys = BTreeSet::new();
        while let Some(key) = object.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(A::Error::custom("duplicate JSON key"));
            }
            object.next_value_seed(Depth(self.0.saturating_add(1)))?;
        }
        Ok(())
    }
}
