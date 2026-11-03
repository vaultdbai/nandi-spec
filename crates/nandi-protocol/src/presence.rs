//! Capability presence: which instance provides which capability, through
//! which agents or workflows, and how busy they are.

use serde::{Deserialize, Serialize};

use crate::time::Timestamp;

/// What implements a capability
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    Agent,
    Workflow,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Workflow => "workflow",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "agent" => Some(Self::Agent),
            "workflow" => Some(Self::Workflow),
            _ => None,
        }
    }
}

/// One capability an instance provides, with its providers
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CapabilityPresence {
    pub capability_id: String,
    pub capability_version: String,
    pub providers: Vec<ProviderPresence>,
}

/// One agent or workflow providing a capability
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ProviderPresence {
    pub provider_kind: ProviderKind,
    pub provider_id: String,
    pub load: u32,
    pub max_concurrent: u32,
    pub provided_tools: Vec<String>,
    /// Epoch ms
    pub online_since: Timestamp,
    /// Epoch ms
    pub last_heartbeat: Timestamp,
}

/// A provider as the relay reports it (which instance it's on)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct InstancePresence {
    pub instance_id: String,
    pub capability_version: String,
    pub provider: ProviderPresence,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kinds_round_trip_through_their_names() {
        for kind in [ProviderKind::Agent, ProviderKind::Workflow] {
            assert_eq!(ProviderKind::parse(kind.as_str()), Some(kind));
            assert_eq!(serde_json::to_value(kind).unwrap(), kind.as_str());
        }
        assert_eq!(ProviderKind::parse("bogus"), None);
    }
}
