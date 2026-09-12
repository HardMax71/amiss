mod tests;

use std::fmt;

use serde::Deserializer;
use serde::de::{self, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};

pub(super) struct Tree<D>(pub(super) D);

macro_rules! delegate {
    (@shape $method:ident => $target:ident($wrapper:ident)) => {
        fn $method<V: Visitor<'de>>(
            self,
            _name: &'static str,
            _members: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, Self::Error> {
            self.0.$target($wrapper(visitor))
        }
    };
    ($($method:ident $(($($argument:ident: $kind:ty),+))?),+ $(,)?) => {
        $(fn $method<V: Visitor<'de>>(
            self,
            $($($argument: $kind,)+)?
            visitor: V,
        ) -> Result<V::Value, Self::Error> {
            self.0.$method($($($argument,)+)? Nested(visitor))
        })+
    };
}

impl<'de, D: Deserializer<'de>> Deserializer<'de> for Tree<D> {
    type Error = D::Error;

    delegate! {
        deserialize_any, deserialize_bool, deserialize_i8, deserialize_i16,
        deserialize_i32, deserialize_i64, deserialize_i128, deserialize_u8,
        deserialize_u16, deserialize_u32, deserialize_u64, deserialize_u128,
        deserialize_f32, deserialize_f64, deserialize_char, deserialize_str,
        deserialize_string, deserialize_bytes, deserialize_byte_buf,
        deserialize_option, deserialize_unit,
        deserialize_unit_struct(name: &'static str),
        deserialize_newtype_struct(name: &'static str),
        deserialize_seq, deserialize_tuple(length: usize),
        deserialize_tuple_struct(name: &'static str, length: usize),
        deserialize_map, deserialize_identifier, deserialize_ignored_any,
    }

    delegate!(@shape deserialize_struct => deserialize_map(Nested));
    delegate!(@shape deserialize_enum => deserialize_any(EnumVisitor));

    fn is_human_readable(&self) -> bool {
        self.0.is_human_readable()
    }
}

struct Nested<V>(V);

macro_rules! visit {
    ($($method:ident($kind:ty)),+ $(,)?) => {
        $(fn $method<E: de::Error>(self, value: $kind) -> Result<Self::Value, E> {
            self.0.$method(value)
        })+
    };
}

impl<'de, V: Visitor<'de>> Visitor<'de> for Nested<V> {
    type Value = V::Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.expecting(formatter)
    }

    visit! {
        visit_bool(bool), visit_i8(i8), visit_i16(i16), visit_i32(i32),
        visit_i64(i64), visit_i128(i128), visit_u8(u8), visit_u16(u16),
        visit_u32(u32), visit_u64(u64), visit_u128(u128), visit_f32(f32),
        visit_f64(f64), visit_char(char), visit_str(&str), visit_borrowed_str(&'de str),
        visit_string(String), visit_bytes(&[u8]), visit_borrowed_bytes(&'de [u8]),
        visit_byte_buf(Vec<u8>),
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        self.0.visit_none()
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        self.0.visit_unit()
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        self.0.visit_some(Tree(deserializer))
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        self.0.visit_newtype_struct(Tree(deserializer))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, access: A) -> Result<Self::Value, A::Error> {
        self.0.visit_seq(Elements(access))
    }

    fn visit_map<A: MapAccess<'de>>(self, access: A) -> Result<Self::Value, A::Error> {
        self.0.visit_map(Members(access))
    }
}

struct Seed<S>(S);

impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for Seed<S> {
    type Value = S::Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        self.0.deserialize(Tree(deserializer))
    }
}

struct Elements<A>(A);

impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for Elements<A> {
    type Error = A::Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Self::Error> {
        self.0.next_element_seed(Seed(seed))
    }

    fn size_hint(&self) -> Option<usize> {
        self.0.size_hint()
    }
}

struct Members<A>(A);

impl<'de, A: MapAccess<'de>> MapAccess<'de> for Members<A> {
    type Error = A::Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Self::Error> {
        self.0.next_key_seed(Seed(seed))
    }

    fn next_value_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<T::Value, Self::Error> {
        self.0.next_value_seed(Seed(seed))
    }

    fn size_hint(&self) -> Option<usize> {
        self.0.size_hint()
    }
}

struct EnumVisitor<V>(V);

impl<'de, V: Visitor<'de>> Visitor<'de> for EnumVisitor<V> {
    type Value = V::Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an enum string or an object with one enum member")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        self.0
            .visit_enum(de::value::StrDeserializer::<E>::new(value))
    }

    fn visit_borrowed_str<E: de::Error>(self, value: &'de str) -> Result<Self::Value, E> {
        self.0
            .visit_enum(de::value::BorrowedStrDeserializer::<E>::new(value))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        self.0
            .visit_enum(de::value::StringDeserializer::<E>::new(value))
    }

    fn visit_map<A: MapAccess<'de>>(self, access: A) -> Result<Self::Value, A::Error> {
        self.0.visit_enum(EnumMember(access))
    }
}

struct EnumMember<A>(A);

impl<'de, A: MapAccess<'de>> EnumAccess<'de> for EnumMember<A> {
    type Error = A::Error;
    type Variant = EnumMember<A>;

    fn variant_seed<V: DeserializeSeed<'de>>(
        mut self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Self::Error> {
        let variant = self
            .0
            .next_key_seed(Seed(seed))?
            .ok_or_else(|| de::Error::custom("expected one enum member"))?;
        Ok((variant, self))
    }
}

impl<'de, A: MapAccess<'de>> EnumMember<A> {
    fn finish<T>(&mut self, value: T) -> Result<T, A::Error> {
        if self.0.next_key::<de::IgnoredAny>()?.is_some() {
            Err(de::Error::custom("expected exactly one enum member"))
        } else {
            Ok(value)
        }
    }
}

impl<'de, A: MapAccess<'de>> VariantAccess<'de> for EnumMember<A> {
    type Error = A::Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        Err(de::Error::custom("unit enum variants must be strings"))
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(
        mut self,
        seed: T,
    ) -> Result<T::Value, Self::Error> {
        let value = self.0.next_value_seed(Seed(seed))?;
        self.finish(value)
    }

    fn tuple_variant<V: Visitor<'de>>(
        mut self,
        length: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let value = self.0.next_value_seed(Tuple { length, visitor })?;
        self.finish(value)
    }

    fn struct_variant<V: Visitor<'de>>(
        mut self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error> {
        let value = self.0.next_value_seed(Struct(visitor))?;
        self.finish(value)
    }
}

struct Tuple<V> {
    length: usize,
    visitor: V,
}

impl<'de, V: Visitor<'de>> DeserializeSeed<'de> for Tuple<V> {
    type Value = V::Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        Tree(deserializer).deserialize_tuple(self.length, self.visitor)
    }
}

struct Struct<V>(V);

impl<'de, V: Visitor<'de>> DeserializeSeed<'de> for Struct<V> {
    type Value = V::Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        Tree(deserializer).deserialize_map(self.0)
    }
}
