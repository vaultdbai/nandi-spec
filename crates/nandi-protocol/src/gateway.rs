//! Gateway cards: what the relay knows of an instance as a whole.

use serde::{Deserialize, Serialize};

use crate::card::OwnerRef;
use crate::directory::Availability;

/// The gateway's agent ID on its own instance (cards are
/// `<instance_id>/gateway`)
pub const GATEWAY_AGENT_ID: &str = "gateway";

/// Bumped when the card's shape changes
pub const GATEWAY_VERSION: u32 = 1;

/// What the relay knows of an instance: its gateway, not its agents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GatewayCard {
    pub instance_id: String,
    pub endpoint: String,
    pub display_name: String,
    pub description: String,
    /// The exposed capabilities (local agents' minus `hide`, within `expose`)
    pub capabilities: Vec<String>,
    /// The gateway takes routed requests (`relay.enabled`). Gateway-routed
    /// tasks don't need the agents' own `accepts_delegation`; the relay
    /// routes by this flag.
    pub accepts_delegation: bool,
    pub max_concurrent: u32,
    /// Tasks in `Working`
    pub current_load: u32,
    pub owner: OwnerRef,
    pub federation_enabled: bool,
    pub gateway_version: u32,
    /// Exposed agents, for member profiles (`agent_count`)
    #[serde(default)]
    pub agent_count: u32,
    /// The NANDI.md body, when the owner shares it (`relay.discoverable`)
    #[serde(default)]
    pub body: Option<String>,
    /// NANDI.md's `responsibilities:`, on every published card whether or
    /// not it is `discoverable`: they are short statements the owner wrote
    /// to be found by ("who is responsible for SOX audit?"), unlike the
    /// body. Empty from Nandis that predate it.
    #[serde(default)]
    pub responsibilities: Vec<String>,
    /// Some local agent takes tasks addressed to it directly
    /// (`accepts_delegation: true`); `None` from Nandis that predate it
    #[serde(default)]
    #[cfg_attr(feature = "specta", specta(optional))]
    pub direct_delegation: Option<bool>,
    /// Whether the owner is around (NANDI.md's `availability:`)
    #[serde(default)]
    pub availability: Availability,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::fixtures::gateway_card;

    #[test]
    fn cards_from_older_nandis_still_parse() {
        let mut value = serde_json::to_value(gateway_card()).unwrap();
        let object = value.as_object_mut().unwrap();
        for key in [
            "agentCount",
            "body",
            "responsibilities",
            "directDelegation",
            "availability",
        ] {
            object.remove(key);
        }
        let card: GatewayCard = serde_json::from_value(value).unwrap();
        assert_eq!(card.agent_count, 0);
        assert!(card.responsibilities.is_empty());
        assert_eq!(card.direct_delegation, None);
        assert_eq!(card.body, None);
        assert!(card.availability.is_available());
        assert_eq!(
            serde_json::to_value(&card).unwrap()["directDelegation"],
            json!(null)
        );
    }
}
