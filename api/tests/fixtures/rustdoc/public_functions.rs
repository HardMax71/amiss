#[cfg(feature = "unrelated")]
const UNRELATED: u64 = 1;

#[cfg(feature = "api")]
pub fn feature_only(value: usize) -> usize {
    value
}

macro_rules! public_function {
    ($name:ident) => {
        pub fn $name<T>(value: T) -> T
        where
            T: Clone,
        {
            value.clone()
        }
    };
}

public_function!(generated);

mod hidden {
    pub fn original(value: u64) -> bool {
        value != 0
    }

    pub struct Owner;

    impl Owner {
        pub fn create() -> Self {
            Self
        }

        pub fn generic<T>(value: T) -> T {
            value
        }

        fn private() {}
    }

    pub trait Service {
        fn execute(&self) -> bool;
    }
}

pub use hidden::{Owner as PublicOwner, Service as PublicService, original as alias};
