//! Agent cards: what other Nandi instances see of an agent that serves
//! capabilities. A card is derived from the agent's definition; it is never
//! stored as one.
//!
//! `id` is `<instance_id>/<agent_id>`, unique across instances.

use serde::{Deserialize, Serialize};

/// Milliseconds since the epoch (`f64`, like [`crate::Timestamp`])
pub type CardTimestamp = f64;

/// Who runs the instance an agent lives on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct OwnerRef {
    pub github_handle: String,
    pub display_name: String,
}

/// One thing an agent does for others, by name (`summarize-paper`).
/// Requesters find providers by this name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ServesCapability {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// JSON Schema of the task input
    #[serde(
        default,
        alias = "input_schema",
        skip_serializing_if = "Option::is_none"
    )]
    pub input_schema: Option<serde_json::Value>,
    /// JSON Schema of the result
    #[serde(
        default,
        alias = "output_schema",
        skip_serializing_if = "Option::is_none"
    )]
    pub output_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    /// `<instance_id>/<agent_id>`
    pub id: String,
    pub name: String,
    pub description: String,
    pub instance_id: String,
    /// Where the relay reaches it, e.g. `wss://relay.example/a/<instance>/<agent>`
    pub endpoint: String,
    pub capabilities: Vec<ServesCapability>,
    pub accepts_delegation: bool,
    pub max_concurrent: u32,
    pub current_load: u32,
    pub owner: OwnerRef,
    /// Instance signature over the card (set by the relay client)
    pub signature: Option<String>,
    pub issued_at: CardTimestamp,
}

impl AgentCard {
    /// `<instance_id>/<agent_id>`
    pub fn card_id(instance_id: &str, agent_id: &str) -> String {
        format!("{instance_id}/{agent_id}")
    }

    /// The agent ID part of `id`
    pub fn agent_id(&self) -> &str {
        self.id
            .strip_prefix(&format!("{}/", self.instance_id))
            .unwrap_or(&self.id)
    }

    pub fn serves(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c.name == capability)
    }

    pub fn with_owner(mut self, owner: OwnerRef) -> Self {
        self.owner = owner;
        self
    }

    pub fn with_issued_at(mut self, issued_at: CardTimestamp) -> Self {
        self.issued_at = issued_at;
        self
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::fixtures::card;

    #[test]
    fn cards_name_their_agent_and_what_it_serves() {
        let card = card();
        assert_eq!(AgentCard::card_id("i1", "papers"), card.id);
        assert_eq!(card.agent_id(), "papers");
        assert!(card.serves("find-papers") && !card.serves("other"));
        let owner = OwnerRef {
            github_handle: "bob".into(),
            display_name: "Bob".into(),
        };
        let card = card.with_owner(owner.clone()).with_issued_at(9.0);
        assert_eq!((card.owner, card.issued_at), (owner, 9.0));
    }

    #[test]
    fn an_id_without_the_instance_prefix_is_the_agent_id() {
        let card = AgentCard {
            id: "papers".into(),
            ..card()
        };
        assert_eq!(card.agent_id(), "papers");
    }

    #[test]
    fn serves_reads_snake_case_schema_keys() {
        let serves: ServesCapability =
            serde_json::from_value(json!({"name": "x", "input_schema": {"type": "object"}}))
                .unwrap();
        assert_eq!(serves.input_schema, Some(json!({"type": "object"})));
        assert_eq!(
            serde_json::to_value(&serves).unwrap(),
            json!({"name": "x", "description": "", "inputSchema": {"type": "object"}})
        );
    }
}
