//! Routing: who on the relay should get a request.
//!
//! The requester's Nandi decides local or network: a named person goes to
//! the network, and a capability one of its own agents or workflows serves
//! runs locally with no relay. Otherwise it sends `RouteQuery` (a
//! [`RoutingQuery`]) and the relay answers `RoutePlan` (a [`RoutePlan`]):
//! the best few candidates, best first, each with its public keys and the
//! reasons it was picked.
//!
//! - **Metadata only.** The relay ranks by who serves the capability,
//!   presence, load, availability and the track record its ledger keeps.
//!   It never sees the request.
//! - **Reasons, not numbers.** A [`RouteReason`] is a typed code with its
//!   parameters, so a client renders it in its own language
//!   ([`RouteReason::sentence`] is the English). Reasons never carry
//!   another member's success rate: "Bob is free and usually quick", not
//!   "Bob 92% success".
//! - **A named person is never swapped**, except for their own
//!   away-delegate (for questions and assignments).
//! - `excluded` hints (the requester's paused and blocked members) are
//!   used for one plan and never stored.
//!
//! Offline: with the relay down there is no plan. A client reports "not
//! connected" for network requests and keeps running local ones.

use serde::{Deserialize, Serialize};

use crate::keys::PublicKeys;
use crate::presence::ProviderKind;
use crate::time::Timestamp;

/// Candidates a plan holds unless the query or the team policy says
/// otherwise
pub const DEFAULT_MAX_CANDIDATES: u32 = 3;

/// The most candidates any plan holds
pub const MAX_CANDIDATES: u32 = 10;

/// What kind of request is being routed
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    /// Work for an agent, answered while the requester waits
    #[default]
    Delegate,
    /// A question to a person (`ask-me`)
    Ask,
    /// Work assigned to a person (`assign-me`)
    Assign,
}

impl RequestKind {
    /// Questions and assignments wait for a person: someone offline is
    /// still a candidate, and their availability counts
    pub fn is_lasting(self) -> bool {
        matches!(self, Self::Ask | Self::Assign)
    }
}

/// Who the request is for
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
// Fields are renamed per variant (not `rename_all_fields`) so the
// TypeScript export matches the JSON
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RouteTarget {
    /// Whoever serves the capability best
    #[default]
    Anyone,
    /// One person (instance), by any of their agents that serves it
    #[serde(rename_all = "camelCase")]
    Person { instance_id: String },
    /// One agent card (`<instance_id>/<agent_id>`)
    #[serde(rename_all = "camelCase")]
    Agent { card_id: String },
}

/// How the relay weighs its candidates. Each one makes its own part of
/// the score dominant.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum RouteStrategy {
    #[default]
    Balanced,
    Fastest,
    HighestSuccessRate,
    LeastLoaded,
    /// Takes turns: whoever had the fewest requests today goes first
    RoundRobin,
}

/// What the requester adds to a query.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RoutingHints {
    /// Card or instance IDs the requester pinned; their score counts 1.5×
    #[serde(default)]
    pub preferred: Vec<String>,
    /// Card or instance IDs never to pick (paused or blocked by the
    /// requester, or already tried). Never stored.
    #[serde(default)]
    pub excluded: Vec<String>,
    /// `None`: the team policy's for the capability
    #[serde(default)]
    pub strategy: Option<RouteStrategy>,
    /// Tool IDs every candidate must have
    #[serde(default)]
    pub required_tools: Vec<String>,
    /// `None`: the team policy's (3 by default)
    #[serde(default)]
    pub max_candidates: Option<u32>,
}

/// A `RouteQuery`: who should get a `capability` request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RoutingQuery {
    pub kind: RequestKind,
    pub capability: String,
    #[serde(default)]
    pub target: RouteTarget,
    #[serde(default)]
    pub hints: RoutingHints,
}

impl RoutingQuery {
    /// A query for anyone serving `capability`
    pub fn anyone(kind: RequestKind, capability: impl Into<String>) -> Self {
        Self {
            kind,
            capability: capability.into(),
            ..Self::default()
        }
    }
}

/// Why a plan has no candidates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum NoRoute {
    /// Nobody on the relay serves the capability
    NobodyServes,
    /// Some serve it, but none takes requests from other Nandis
    NotTakingRequests,
    /// Some serve it, but none can take it now (offline, failing, missing
    /// a tool, or excluded)
    NobodyAvailable,
}

/// Why a candidate is where it is in the plan: a code and its
/// parameters. `name` is the candidate's display name.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
// Fields are renamed per variant (not `rename_all_fields`) so the
// TypeScript export matches the JSON
#[serde(tag = "code", rename_all = "snake_case")]
pub enum RouteReason {
    /// The requester asked for them by name
    Named { name: String },
    /// The requester pinned them for this capability
    Pinned { name: String },
    /// `away_name` is away (until `until`, epoch ms) and they cover
    #[serde(rename_all = "camelCase")]
    Covers {
        name: String,
        away_name: String,
        #[serde(default)]
        until: Option<Timestamp>,
    },
    /// Online with room
    Free { name: String },
    /// At or over capacity; `wait_minutes` when the relay can estimate it
    #[serde(rename_all = "camelCase")]
    Busy {
        name: String,
        load: u32,
        max: u32,
        #[serde(default)]
        wait_minutes: Option<u32>,
    },
    /// Offline: the request waits for them
    Offline { name: String },
    /// Away (until `until`) with nobody covering: moved down
    Away {
        name: String,
        #[serde(default)]
        until: Option<Timestamp>,
    },
    /// Usually quicker than the others
    UsuallyQuick { name: String },
    /// Usually gets this done
    Dependable { name: String },
    /// Has had fewer requests today than the others
    LighterDay { name: String },
    /// Their turn (round robin)
    TheirTurn { name: String },
    /// Back after recent failures: this request is the trial
    TryingAgain { name: String },
}

impl RouteReason {
    /// The code, as on the wire
    pub fn code(&self) -> &'static str {
        match self {
            Self::Named { .. } => "named",
            Self::Pinned { .. } => "pinned",
            Self::Covers { .. } => "covers",
            Self::Free { .. } => "free",
            Self::Busy { .. } => "busy",
            Self::Offline { .. } => "offline",
            Self::Away { .. } => "away",
            Self::UsuallyQuick { .. } => "usually_quick",
            Self::Dependable { .. } => "dependable",
            Self::LighterDay { .. } => "lighter_day",
            Self::TheirTurn { .. } => "their_turn",
            Self::TryingAgain { .. } => "trying_again",
        }
    }

    /// One short English sentence (logs, operators, clients without a
    /// translation). Times are left out: a client shows them in its own
    /// format.
    pub fn sentence(&self) -> String {
        match self {
            Self::Named { name } => format!("You asked for {name}"),
            Self::Pinned { name } => format!("You pinned {name}"),
            Self::Covers {
                name, away_name, ..
            } => format!("{away_name} is away; {name} covers"),
            Self::Free { name } => format!("{name} is free"),
            Self::Busy {
                name,
                load,
                max,
                wait_minutes: Some(minutes),
            } => format!("{name} {load}/{max} busy (about {minutes} min wait)"),
            Self::Busy {
                name, load, max, ..
            } => format!("{name} {load}/{max} busy"),
            Self::Offline { name } => format!("{name} is offline; it waits for them"),
            Self::Away { name, .. } => format!("{name} is away"),
            Self::UsuallyQuick { name } => format!("{name} is usually quick"),
            Self::Dependable { name } => format!("{name} usually gets this done"),
            Self::LighterDay { name } => format!("{name} has had less to do today"),
            Self::TheirTurn { name } => format!("It's {name}'s turn"),
            Self::TryingAgain { name } => format!("Trying {name} again after recent failures"),
        }
    }
}

/// One candidate in a plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RouteCandidate {
    /// The card to send to (`<instance_id>/<agent_id>`)
    pub target: String,
    pub instance_id: String,
    /// The owner's display name
    pub name: String,
    /// The owner's GitHub handle, when they publish one
    #[serde(default)]
    pub handle: Option<String>,
    /// The agent's (or workflow's) name
    pub agent_name: String,
    #[serde(default)]
    pub provider_kind: ProviderKind,
    /// Connected to the relay now
    pub online: bool,
    /// Running plus waiting at the relay
    pub load: u32,
    pub max_concurrent: u32,
    /// The expected wait (seconds) when it's busy and the relay can
    /// estimate it
    #[serde(default)]
    pub wait_seconds: Option<u32>,
    /// The instance ID of the person this candidate covers for
    #[serde(default)]
    pub redirected_from: Option<String>,
    /// Most important first
    pub reasons: Vec<RouteReason>,
    /// The keys the relay pinned for the candidate's instance; `None`
    /// until it sent some
    #[serde(default)]
    pub public_keys: Option<PublicKeys>,
}

/// The relay's answer to a [`RoutingQuery`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RoutePlan {
    pub plan_id: String,
    /// After this (epoch ms) ask again: load and presence will have moved
    pub expires_at: Timestamp,
    /// Best first, at most the query's `max_candidates`
    pub candidates: Vec<RouteCandidate>,
    /// Why there are none, when there are none
    #[serde(default)]
    pub no_route: Option<NoRoute>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_query_reads_with_only_its_kind_and_capability() {
        let query: RoutingQuery =
            serde_json::from_value(json!({"kind": "ask", "capability": "ask-me"})).unwrap();
        assert_eq!(query, RoutingQuery::anyone(RequestKind::Ask, "ask-me"));
        assert!(query.kind.is_lasting() && !RequestKind::Delegate.is_lasting());
    }

    #[test]
    fn targets_and_reasons_are_tagged() {
        assert_eq!(
            serde_json::to_value(RouteTarget::Agent {
                card_id: "i1/papers".into()
            })
            .unwrap(),
            json!({"type": "agent", "cardId": "i1/papers"})
        );
        assert_eq!(
            serde_json::to_value(RouteReason::Busy {
                name: "Alice".into(),
                load: 3,
                max: 3,
                wait_minutes: Some(20),
            })
            .unwrap(),
            json!({"code": "busy", "name": "Alice", "load": 3, "max": 3, "waitMinutes": 20})
        );
        assert_eq!(
            serde_json::to_value(RouteStrategy::HighestSuccessRate).unwrap(),
            "highest_success_rate"
        );
    }

    #[test]
    fn every_reason_has_its_code_and_a_sentence() {
        let name = || "Bob".to_string();
        let reasons = [
            RouteReason::Named { name: name() },
            RouteReason::Pinned { name: name() },
            RouteReason::Covers {
                name: name(),
                away_name: "Alice".into(),
                until: Some(5.0),
            },
            RouteReason::Free { name: name() },
            RouteReason::Busy {
                name: name(),
                load: 3,
                max: 3,
                wait_minutes: Some(20),
            },
            RouteReason::Busy {
                name: name(),
                load: 3,
                max: 3,
                wait_minutes: None,
            },
            RouteReason::Offline { name: name() },
            RouteReason::Away {
                name: name(),
                until: None,
            },
            RouteReason::UsuallyQuick { name: name() },
            RouteReason::Dependable { name: name() },
            RouteReason::LighterDay { name: name() },
            RouteReason::TheirTurn { name: name() },
            RouteReason::TryingAgain { name: name() },
        ];
        for reason in reasons {
            let value = serde_json::to_value(&reason).unwrap();
            assert_eq!(value["code"], reason.code(), "{value}");
            assert!(reason.sentence().contains("Bob"));
            // No number that reads as a track record
            assert!(!reason.sentence().contains('%'));
        }
        assert_eq!(
            RouteReason::Covers {
                name: "Bob".into(),
                away_name: "Alice".into(),
                until: None
            }
            .sentence(),
            "Alice is away; Bob covers"
        );
    }
}
