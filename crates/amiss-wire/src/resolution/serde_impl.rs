macro_rules! guarded_serde {
    ($name:ident $(<$generic:ident>)?, $guard:ident) => {
        impl $(<$generic: serde::Serialize>)? serde::Serialize for $name $(<$generic>)? {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                Self::serialize(self, serializer)
            }
        }

        impl<'de $(, $generic: serde::Deserialize<'de>)?> serde::Deserialize<'de>
            for $name $(<$generic>)?
        {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Self::deserialize(crate::codec::$guard(deserializer))
            }
        }
    };
}

macro_rules! deserialize_shape {
    (
        $name:ident<$generic:ident> from $input:ident {
            tuple { $($tuple:ident($tuple_field:ident)),* $(,)? }
            newtype { $($newtype:ident),* $(,)? }
            named { $($named:ident { $($field:ident),* }),* $(,)? }
            unit { $($unit:ident),* $(,)? }
            checked { $($checked:ident($checked_field:ident) => $check:ident),* $(,)? }
        }
    ) => {
        impl<'de, $generic: serde::Deserialize<'de>> serde::Deserialize<'de> for $name<$generic> {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let input = $input::deserialize(crate::codec::object(deserializer))?;
                Ok(match input {
                    $($input::$tuple { $tuple_field } => Self::$tuple($tuple_field),)*
                    $($input::$newtype(value) => Self::$newtype(value),)*
                    $($input::$named { $($field),* } => Self::$named { $($field),* },)*
                    $($input::$unit {} => Self::$unit,)*
                    $($input::$checked { $checked_field } => Self::$checked($check($checked_field)?),)*
                })
            }
        }
    };
}

macro_rules! serialize_shape {
    (
        $name:ident<$generic:ident> as $view:ident {
            tuple { $($tuple:ident($tuple_field:ident)),* $(,)? }
            newtype { $($newtype:ident),* $(,)? }
            copied { $($copied:ident($copied_field:ident)),* $(,)? }
            unit { $($unit:ident),* $(,)? }
            converted { $($converted:ident($converted_field:ident) => $convert:ident),* $(,)? }
        }
    ) => {
        impl<$generic: serde::Serialize> serde::Serialize for $name<$generic> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let view = match self {
                    $(Self::$tuple(value) => $view::$tuple { $tuple_field: value },)*
                    $(Self::$newtype(value) => $view::$newtype(value),)*
                    $(Self::$copied(value) => $view::$copied { $copied_field: *value },)*
                    $(Self::$unit => $view::$unit,)*
                    $(Self::$converted(value) => $view::$converted { $converted_field: $convert(value) },)*
                };
                serde::Serialize::serialize(&view, serializer)
            }
        }
    };
}

pub(super) use {deserialize_shape, guarded_serde, serialize_shape};
