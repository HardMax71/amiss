use std::error::Error;

#[derive(Debug, thiserror::Error)]
#[error("{context}")]
pub struct ConfigError {
    context: &'static str,
    #[source]
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl ConfigError {
    pub const fn invalid(context: &'static str) -> Self {
        Self {
            context,
            source: None,
        }
    }

    pub fn caused_by(context: &'static str, source: impl Error + Send + Sync + 'static) -> Self {
        Self {
            context,
            source: Some(Box::new(source)),
        }
    }
}
