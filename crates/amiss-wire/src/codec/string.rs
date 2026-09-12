use serde::Deserializer;
use serde::de::value::{BorrowedStrDeserializer, StrDeserializer, StringDeserializer};
use serde::de::{Error, Visitor};

/// Restricts a derived string enum to its JSON string spelling.
pub fn string<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> impl Deserializer<'de, Error = D::Error> {
    StringOnly(deserializer)
}

struct StringOnly<D>(D);

impl<'de, D: Deserializer<'de>> Deserializer<'de> for StringOnly<D> {
    type Error = D::Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.0.deserialize_str(visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0.deserialize_str(EnumVisitor(visitor))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct
        map struct identifier ignored_any
    }

    fn is_human_readable(&self) -> bool {
        self.0.is_human_readable()
    }
}

struct EnumVisitor<V>(V);

impl<'de, V: Visitor<'de>> Visitor<'de> for EnumVisitor<V> {
    type Value = V::Value;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a string enum")
    }

    fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
        self.0.visit_enum(StrDeserializer::new(value))
    }

    fn visit_borrowed_str<E: Error>(self, value: &'de str) -> Result<Self::Value, E> {
        self.0.visit_enum(BorrowedStrDeserializer::new(value))
    }

    fn visit_string<E: Error>(self, value: String) -> Result<Self::Value, E> {
        self.0.visit_enum(StringDeserializer::new(value))
    }
}
