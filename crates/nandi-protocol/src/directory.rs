//! The member directory and people search: who is on the relay (online or
//! not), whether they're around, and who is responsible for a topic.
//!
//! - `QueryDirectory` → `Directory`: every member with a gateway card,
//!   online or not, with when the relay last saw them
//! - `FindPeople` → `PeopleMatches`: the members whose card matches a
//!   topic, best first (the relay ranks them)
//!
//! Offline: with the relay down a client may show the members it last
//! listed, marked as last seen. People search answers "not connected";
//! there is no matching from a cached copy.

use serde::{Deserialize, Serialize};

use crate::gateway::GatewayCard;
use crate::time::Timestamp;

/// The capability a person's own agent serves for questions about them; a
/// gateway routes it straight to that agent
pub const ASK_ME: &str = "ask-me";

/// Whether someone is around, as they set it in their NANDI.md
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityStatus {
    #[default]
    Available,
    Busy,
    Away,
}

/// Someone's availability: busy or away until a time, and who covers for
/// them meanwhile.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    #[serde(default)]
    pub status: AvailabilityStatus,
    /// When it ends (epoch ms); `None`: until they change it
    #[serde(default)]
    pub until: Option<Timestamp>,
    /// The instance ID of who covers for them
    #[serde(default)]
    pub delegate_to: Option<String>,
}

impl Availability {
    /// Available, with no end time and nobody covering
    pub fn available() -> Self {
        Self::default()
    }

    /// What holds at `now`: once `until` has passed, available again
    pub fn at(&self, now: Timestamp) -> Self {
        match self.until {
            Some(until) if until <= now => Self::available(),
            _ => self.clone(),
        }
    }

    pub fn is_available(&self) -> bool {
        self.status == AvailabilityStatus::Available
    }
}

/// One member in the relay's directory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct MemberRecord {
    /// Their latest gateway card (availability included)
    pub card: GatewayCard,
    /// Connected to the relay right now
    pub online: bool,
    /// When the relay last heard from them (epoch ms)
    pub last_seen_ms: Timestamp,
}

/// What one of a person's agents serves
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ServedCapability {
    pub capability: String,
    #[serde(default)]
    pub description: String,
}

/// One of a person's online agents that matches the topic (every one when
/// listing everyone)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct MatchedAgent {
    /// `<instance>/<agent>`
    pub target: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub serves: Vec<ServedCapability>,
}

/// One person who matches a topic, with why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PeopleMatch {
    pub instance_id: String,
    pub name: String,
    #[serde(default)]
    pub handle: Option<String>,
    pub online: bool,
    /// When the relay last heard from them (epoch ms)
    #[serde(default)]
    pub last_seen_ms: Option<Timestamp>,
    /// Their gateway takes requests
    pub takes_requests: bool,
    /// The capability to ask them with: [`ASK_ME`] when they serve it,
    /// else `gateway`
    pub ask_capability: String,
    #[serde(default)]
    pub responsibilities: Vec<String>,
    #[serde(default)]
    pub description: String,
    /// Which parts matched (`responsibilities`, `capabilities`, `profile`,
    /// `NANDI.md`, `agents`)
    #[serde(default)]
    pub matched_on: Vec<String>,
    /// The NANDI.md sentence that matched, when they share it
    #[serde(default)]
    pub excerpt: Option<String>,
    #[serde(default)]
    pub agents: Vec<MatchedAgent>,
    /// All their agents online, matching or not
    #[serde(default)]
    pub agents_online: u32,
    #[serde(default)]
    pub availability: Availability,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn availability_ends_at_its_until() {
        let away = Availability {
            status: AvailabilityStatus::Away,
            until: Some(100.0),
            delegate_to: Some("bob-1".into()),
        };
        assert_eq!(away.at(99.0), away);
        assert!(away.at(100.0).is_available());
        assert_eq!(away.at(100.0).delegate_to, None);
        let open_ended = Availability {
            status: AvailabilityStatus::Busy,
            until: None,
            delegate_to: None,
        };
        assert_eq!(open_ended.at(f64::MAX), open_ended);
    }

    #[test]
    fn availability_is_snake_case_status_with_camel_case_fields() {
        let away = Availability {
            status: AvailabilityStatus::Away,
            until: Some(5.0),
            delegate_to: Some("bob-1".into()),
        };
        assert_eq!(
            serde_json::to_value(&away).unwrap(),
            json!({"status": "away", "until": 5.0, "delegateTo": "bob-1"})
        );
        let empty: Availability = serde_json::from_value(json!({})).unwrap();
        assert_eq!(empty, Availability::available());
    }

    #[test]
    fn a_people_match_reads_with_only_its_required_fields() {
        let found: PeopleMatch = serde_json::from_value(json!({
            "instanceId": "i1", "name": "Ada", "online": false,
            "takesRequests": true, "askCapability": "ask-me"
        }))
        .unwrap();
        assert!(found.agents.is_empty() && found.availability.is_available());
        assert_eq!(found.last_seen_ms, None);
    }
}
