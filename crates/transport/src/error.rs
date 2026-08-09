use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("nats: {0}")]
    Nats(#[from] async_nats::Error),
    #[error("serialize: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("handler: {0}")]
    Handler(#[from] anyhow::Error),
    #[error("{0}")]
    Message(String),
}
