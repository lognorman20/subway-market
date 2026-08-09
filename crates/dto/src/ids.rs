use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable unique id for every bus event (ledger UNIQUE).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(pub String);

impl EventId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<String> for EventId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl AsRef<str> for EventId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Agent identity (HTTP Bearer auth).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<String> for AgentId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl AsRef<str> for AgentId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// On-chain LMSR market id (decimal string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MarketId(pub String);

impl MarketId {
    pub fn from_on_chain(id: u64) -> Self {
        Self(id.to_string())
    }

    pub fn as_u64(&self) -> Option<u64> {
        self.0.parse().ok()
    }
}

impl From<String> for MarketId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl AsRef<str> for MarketId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Stable trade tape id (distinct from bus `event_id`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TradeId(pub String);

impl TradeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for TradeId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<String> for TradeId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl AsRef<str> for TradeId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Joins DiffEvent ↔ ledger (and ledger column).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiffId(pub String);

impl DiffId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for DiffId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<String> for DiffId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl AsRef<str> for DiffId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Common envelope fields present on every bus payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventMeta {
    pub event_id: EventId,
}

impl EventMeta {
    pub fn new() -> Self {
        Self {
            event_id: EventId::new(),
        }
    }
}

impl Default for EventMeta {
    fn default() -> Self {
        Self::new()
    }
}
