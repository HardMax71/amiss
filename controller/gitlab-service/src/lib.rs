#![forbid(unsafe_code)]

mod acquisition;
mod config;
mod objects;
mod runtime;

pub use config::{ConfigError, ServiceConfig};
pub use runtime::{ServiceError, run};
