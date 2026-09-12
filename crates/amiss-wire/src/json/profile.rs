use serde::Serialize;
use serde::ser::{self, Error as _};

pub(crate) fn check<T: Serialize + ?Sized>(value: &T) -> Result<(), serde_json::Error> {
    value.serialize(Profile {
        key: false,
        depth: 0,
    })
}

#[derive(Clone, Copy)]
struct Profile {
    key: bool,
    depth: usize,
}

impl Profile {
    fn container(mut self) -> Result<Self, serde_json::Error> {
        self.value()?;
        if self.depth >= 512 {
            return Err(ser::Error::custom("nesting limit exceeded"));
        }
        self.depth = self.depth.saturating_add(1);
        Ok(self)
    }

    fn value(self) -> Result<(), serde_json::Error> {
        self.scalar(false)
    }

    fn scalar(self, string: bool) -> Result<(), serde_json::Error> {
        if self.key && !string {
            Err(ser::Error::custom("object keys must be strings"))
        } else {
            Ok(())
        }
    }

    fn integer(self, value: u128) -> Result<(), serde_json::Error> {
        self.value()?;
        if value > u128::from(super::MAX_SAFE_INTEGER.unsigned_abs()) {
            Err(ser::Error::custom("integer is outside the safe range"))
        } else {
            Ok(())
        }
    }
}

macro_rules! scalar_checks {
    ($($method:ident($($argument:ident: $kind:ty),*) => $string:literal);+ $(;)?) => {
        $(fn $method(self, $($argument: $kind),*) -> Result<(), Self::Error> {
            self.scalar($string)
        })+
    };
}

macro_rules! variant_containers {
    ($($method:ident),+ $(,)?) => {$ (
        fn $method(
            self,
            _name: &'static str,
            _index: u32,
            _variant: &'static str,
            _len: usize,
        ) -> Result<Self, Self::Error> {
            self.container()?.container()
        }
    )+};
}

impl ser::Serializer for Profile {
    type Ok = ();
    type Error = serde_json::Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_i8(self, value: i8) -> Result<(), Self::Error> {
        self.integer(u128::from(value.unsigned_abs()))
    }
    fn serialize_i16(self, value: i16) -> Result<(), Self::Error> {
        self.integer(u128::from(value.unsigned_abs()))
    }
    fn serialize_i32(self, value: i32) -> Result<(), Self::Error> {
        self.integer(u128::from(value.unsigned_abs()))
    }
    fn serialize_i64(self, value: i64) -> Result<(), Self::Error> {
        self.integer(u128::from(value.unsigned_abs()))
    }
    fn serialize_u8(self, value: u8) -> Result<(), Self::Error> {
        self.integer(u128::from(value))
    }
    fn serialize_u16(self, value: u16) -> Result<(), Self::Error> {
        self.integer(u128::from(value))
    }
    fn serialize_u32(self, value: u32) -> Result<(), Self::Error> {
        self.integer(u128::from(value))
    }
    fn serialize_u64(self, value: u64) -> Result<(), Self::Error> {
        self.integer(u128::from(value))
    }
    fn serialize_i128(self, value: i128) -> Result<(), Self::Error> {
        self.integer(value.unsigned_abs())
    }
    fn serialize_u128(self, value: u128) -> Result<(), Self::Error> {
        self.integer(value)
    }
    fn serialize_f32(self, _value: f32) -> Result<(), Self::Error> {
        Err(Self::Error::custom("floating point values are forbidden"))
    }
    fn serialize_f64(self, _value: f64) -> Result<(), Self::Error> {
        Err(Self::Error::custom("floating point values are forbidden"))
    }
    scalar_checks! {
        serialize_bool(_value: bool) => false;
        serialize_char(_value: char) => true;
        serialize_str(_value: &str) => true;
        serialize_none() => false;
        serialize_unit() => false;
        serialize_unit_struct(_name: &'static str) => false;
        serialize_unit_variant(_name: &'static str, _index: u32, _variant: &'static str) => true;
    }
    fn serialize_bytes(self, _value: &[u8]) -> Result<(), Self::Error> {
        self.container().map(|_profile| ())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), Self::Error> {
        self.value()?;
        value.serialize(self)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        value.serialize(self.container()?)
    }
    fn serialize_seq(self, _len: Option<usize>) -> Result<Self, Self::Error> {
        self.container()
    }
    fn serialize_tuple(self, _len: usize) -> Result<Self, Self::Error> {
        self.container()
    }
    fn serialize_tuple_struct(self, _name: &'static str, _len: usize) -> Result<Self, Self::Error> {
        self.container()
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<Self, Self::Error> {
        self.container()
    }
    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self, Self::Error> {
        self.container()
    }
    variant_containers!(serialize_tuple_variant, serialize_struct_variant);
}

impl ser::SerializeSeq for Profile {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl ser::SerializeTuple for Profile {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl ser::SerializeTupleStruct for Profile {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl ser::SerializeTupleVariant for Profile {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl ser::SerializeStruct for Profile {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        _key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl ser::SerializeStructVariant for Profile {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        _key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl ser::SerializeMap for Profile {
    type Ok = ();
    type Error = serde_json::Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Self::Error> {
        key.serialize(Self {
            key: true,
            depth: self.depth,
        })
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        value.serialize(*self)
    }
    fn end(self) -> Result<(), Self::Error> {
        Ok(())
    }
}
