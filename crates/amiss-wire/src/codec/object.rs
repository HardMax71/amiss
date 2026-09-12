use serde::Deserializer;
use serde::de::Visitor;

/// Restricts a derived object or internally tagged enum to JSON object syntax.
pub fn object<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> impl Deserializer<'de, Error = D::Error> {
    ObjectDeserializer(deserializer)
}

struct ObjectDeserializer<D>(D);

impl<'de, D: Deserializer<'de>> Deserializer<'de> for ObjectDeserializer<D> {
    type Error = D::Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Self::Error> {
        self.0.deserialize_map(visitor)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct
        map struct enum identifier ignored_any
    }

    fn is_human_readable(&self) -> bool {
        self.0.is_human_readable()
    }
}
