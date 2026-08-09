//! Human wallet session via CLI EVM key (cast/alloy) — E2E 2 automated path.

use alloy::signers::local::PrivateKeySigner;
use alloy::signers::Signer;
use anyhow::{Context, Result};

use crate::http::BackendClient;

#[derive(Debug, Clone)]
pub struct HumanSession {
    pub address: String,
    pub session_token: String,
    /// Retained so callers can re-sign if needed.
    pub signer: PrivateKeySigner,
}

/// Random key → challenge → EIP-191 sign → session token.
/// Frontend Phantom connect may be stubbed; session Bearer is enough for HTTP buy/sell.
pub async fn establish_human_session(client: &BackendClient) -> Result<HumanSession> {
    let signer = PrivateKeySigner::random();
    let address = format!("{:#x}", signer.address());

    let challenge = client
        .auth_challenge(&address)
        .await
        .context("auth challenge")?;

    let sig = signer
        .sign_message(challenge.message.as_bytes())
        .await
        .context("sign challenge")?;
    let sig_hex = format!("0x{}", hex::encode(sig.as_bytes()));

    let verified = client
        .auth_verify(&challenge.challenge_id, &address, &sig_hex)
        .await
        .context("auth verify")?;

    Ok(HumanSession {
        address,
        session_token: verified.session_token,
        signer,
    })
}

/// Anvil account #0 — stable local identity when desired.
pub fn anvil_account0_signer() -> Result<PrivateKeySigner> {
    let pk = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
    pk.parse().context("parse anvil #0 key")
}

pub async fn establish_session_with_signer(
    client: &BackendClient,
    signer: PrivateKeySigner,
) -> Result<HumanSession> {
    let address = format!("{:#x}", signer.address());
    let challenge = client.auth_challenge(&address).await?;
    let sig = signer.sign_message(challenge.message.as_bytes()).await?;
    let sig_hex = format!("0x{}", hex::encode(sig.as_bytes()));
    let verified = client
        .auth_verify(&challenge.challenge_id, &address, &sig_hex)
        .await?;
    Ok(HumanSession {
        address,
        session_token: verified.session_token,
        signer,
    })
}
