macro_rules! wire_fields {
    (
        $wire:ident <=> $domain:ident ($row:ident) {
            fields [$($field:ident),* $(,)?],
            mapped [$($mapped:ident),* $(,)?],
            wire { $($wire_field:ident: $wire_value:expr),* $(,)? },
            domain { $($domain_field:ident: $domain_value:expr),* $(,)? }
        }
    ) => {
        impl From<$domain> for $wire {
            fn from($row: $domain) -> Self {
                Self {
                    $($field: $row.$field,)*
                    $($mapped: $row.$mapped.into(),)*
                    $($wire_field: $wire_value,)*
                }
            }
        }

        impl From<$wire> for $domain {
            fn from($row: $wire) -> Self {
                Self {
                    $($field: $row.$field,)*
                    $($mapped: $row.$mapped.into(),)*
                    $($domain_field: $domain_value,)*
                }
            }
        }
    };
}

pub(super) use wire_fields;
