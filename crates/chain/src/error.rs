//! Chain crate errors.

use alloy::primitives::Address;
use thiserror::Error;

/// Errors from config load or chain helpers.
#[derive(Debug, Error)]
pub enum ChainError {
    #[error("missing env var `{0}`")]
    MissingEnv(&'static str),

    #[error("invalid address for `{name}`: {source}")]
    InvalidAddress {
        name: &'static str,
        #[source]
        source: alloy::hex::FromHexError,
    },

    #[error("invalid PLATFORM_SIGNER_KEY: {0}")]
    InvalidSignerKey(String),

    #[error(
        "PLATFORM_SIGNER_ADDRESS {configured} does not match key-derived address {derived}"
    )]
    SignerAddressMismatch {
        configured: Address,
        derived: Address,
    },

    #[error("outcome must be Early or Late (got Unresolved)")]
    InvalidResolveOutcome,

    #[error("unknown side uint8={0}")]
    UnknownSide(u8),

    #[error("unknown outcome uint8={0}")]
    UnknownOutcome(u8),

    #[error(transparent)]
    Transport(#[from] alloy::transports::TransportError),

    #[error(transparent)]
    Contract(#[from] alloy::contract::Error),

    #[error(transparent)]
    PendingTx(#[from] alloy::providers::PendingTransactionError),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Result alias for chain helpers.
pub type Result<T> = std::result::Result<T, ChainError>;
