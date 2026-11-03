//! The relay's WebSocket frames: JSON text frames, one [`RelayMessage`]
//! each, tagged by `type` (`{"type": "heartbeat", "instanceId": "…", …}`).
//!
//! `QueryCapability` with the capability [`ALL_CAPABILITIES`] (`*`) asks
//! for every registered card (presence).
//!
//! `RegisterInstance` carries the client's [`crate::PROTOCOL_VERSION`]. A
//! relay that speaks another version refuses it with an `Error` whose code
//! is [`VERSION_MISMATCH`].
//!
//! Offline: every request here has a reply or a timeout on the client; a
//! client that isn't connected reports "not connected" and keeps working
//! (cards it last saw, its own agents locally). `Peer` messages to an
//! offline instance come back as `Undeliverable`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::card::AgentCard;
use crate::clarification::{ClarificationRequest, ClarificationResponse};
use crate::directory::{MemberRecord, PeopleMatch};
use crate::gateway::GatewayCard;
use crate::presence::{CapabilityPresence, InstancePresence};
use crate::task::{ReasonKind, Task, TaskResult, TaskState};
use crate::time::Timestamp;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RelayMessage {
    // Provider → relay
    RegisterInstance {
        /// Signed instance token (HMAC by the relay's secret)
        instance_id: String,
        agent_cards: Vec<AgentCard>,
        signature: String,
        /// What this instance provides; left out when empty
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        capabilities: Vec<CapabilityPresence>,
        /// Tool IDs available on this instance; left out when empty
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tools: Vec<String>,
        /// The [`crate::PROTOCOL_VERSION`] the instance speaks
        #[serde(default)]
        protocol_version: u32,
    },
    /// Every 30s; `load` is running tasks per agent ID
    Heartbeat {
        instance_id: String,
        load: HashMap<String, u32>,
        ts: Timestamp,
    },
    Deregister {
        instance_id: String,
    },
    TaskResult {
        task_id: String,
        result: TaskResult,
        signature: String,
    },

    // Requester → relay
    QueryCapability {
        capability: String,
        requester_instance: String,
    },
    DelegateTask {
        task_id: String,
        /// Target card ID (`<instance_id>/<agent_id>`)
        target_agent: String,
        capability: String,
        input: Value,
        requester_card: AgentCard,
        deadline: Option<Timestamp>,
        signature: String,
    },

    // Relay → both
    /// Registration accepted
    Registered {
        instance_id: String,
    },
    CapabilityResult {
        capability: String,
        agents: Vec<AgentCard>,
    },
    /// To the requester: the task was routed. To the provider: run it.
    TaskAssigned(TaskAssigned),
    TaskCompleted(TaskCompleted),
    TaskFailed(TaskFailed),
    Error {
        code: String,
        message: String,
    },

    // Gateway cards (see `gateway`)
    /// Instance → relay; replaces its card. Re-sent on reconnect.
    RegisterGateway {
        instance_id: String,
        card: GatewayCard,
    },

    // The member directory and people search (see `directory`)
    /// Instance → relay: every member with a gateway card, online or not;
    /// with `since` (epoch ms), only those seen after it
    QueryDirectory {
        #[serde(default)]
        since: Option<Timestamp>,
    },
    /// Relay → instance, sorted by instance ID
    Directory {
        members: Vec<MemberRecord>,
    },
    /// Instance → relay: who is responsible for `topic` (empty: everyone),
    /// at most `limit`
    FindPeople {
        topic: String,
        limit: u32,
    },
    /// Relay → instance: the best `limit` matches for `topic`, best first
    PeopleMatches {
        topic: String,
        limit: u32,
        matches: Vec<PeopleMatch>,
        /// Members searched
        searched: u32,
        /// Members searched who list no responsibilities
        unstated: u32,
        /// Matches before `limit`
        total: u32,
    },

    // Wave final: the task lifecycle between two instances. The relay
    // forwards `Peer` to `target_instance` as is, filling in
    // `from_instance` (whatever the sender put there is overwritten), and
    // answers `Undeliverable` when the target isn't connected. Stateless,
    // so it survives relay restarts.
    Peer {
        #[serde(default)]
        from_instance: String,
        target_instance: String,
        body: PeerMessage,
    },
    /// Relay → sender: `target_instance` is offline
    Undeliverable {
        target_instance: String,
        body: PeerMessage,
    },

    // Wave A: capability presence (Track D)
    /// Instance → relay; replaces its presence. Also carried by
    /// `RegisterInstance` and refreshed with the heartbeat.
    UpdatePresence {
        instance_id: String,
        capabilities: Vec<CapabilityPresence>,
        tools: Vec<String>,
    },
    /// Instance → relay: who provides `capability_id` (`*`: everything)
    CapabilityPresenceQuery {
        capability_id: String,
        requester_instance: String,
    },
    /// Relay → instance
    CapabilityPresenceResponse {
        capability_id: String,
        providers: Vec<InstancePresence>,
    },
}

/// Task lifecycle and clarification traffic between requester and
/// provider (wave final, Tracks B and C), tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum PeerMessage {
    /// Provider → requester: accepted, with an estimate
    AcknowledgeTask {
        task_id: String,
        eta_seconds: Option<u32>,
        note: Option<String>,
    },
    /// Provider → requester: refused (policy, trust, capacity)
    RejectTask {
        task_id: String,
        reason: String,
        /// Why, as the requester's UI matches it to a fix; `None` when
        /// there is no typed reason
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason_kind: Option<ReasonKind>,
    },
    /// Provider → requester, only when the task asked for progress (a hold
    /// or a place in line is always sent)
    ProgressUpdate {
        task_id: String,
        percent: Option<u8>,
        note: Option<String>,
        /// Why it waits, when it waits (held for approval, queued)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason_kind: Option<ReasonKind>,
        /// 1-based place in the provider's line, while queued
        #[serde(default, skip_serializing_if = "Option::is_none")]
        queue_position: Option<u32>,
    },
    /// Requester → provider
    StatusQuery { task_id: String },
    /// Provider → requester
    StatusResponse {
        task_id: String,
        state: TaskState,
        eta_seconds: Option<u32>,
        elapsed_seconds: u32,
        /// Once it ended: its result, so a requester that was offline when
        /// it was sent still gets it (a lasting assignment)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Either side
    CancelTask {
        task_id: String,
        reason: Option<String>,
    },
    /// Provider → requester: the task reached the provider
    TaskDelivered { task_id: String },
    /// Requester → provider: gave up waiting
    TaskTimedOut {
        task_id: String,
        reason: Option<String>,
    },
    /// Provider → requester
    ClarificationRequest(ClarificationRequest),
    /// Requester → provider
    ClarificationResponse(ClarificationResponse),
    /// Requester → provider: nobody answered in time
    ClarificationTimeout { task_id: String },
    /// Provider → requester (wave EF): send this task to
    /// `new_provider_card` instead; the provider already let it go
    ReassignTask {
        task_id: String,
        new_provider_instance: String,
        new_provider_card: String,
        /// The new provider's display name, for the requester's thread
        #[serde(default)]
        new_provider_name: Option<String>,
        /// Who reassigned it (display name)
        by: String,
        reason: Option<String>,
    },
    /// A calendar trigger's backup → its primary: how is this duty going?
    /// (`duty_id` is `<trigger>/<date>/<time>`, see `domain::calendar`)
    DutyQuery { duty_id: String },
    /// Primary → backup
    DutyStatus { duty_id: String, state: DutyState },
}

/// How a calendar trigger's primary is doing with one scheduled fire, as
/// its own Nandi reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum DutyState {
    /// Not due yet on the primary's clock (epoch ms)
    Upcoming {
        due_at: i64,
    },
    /// Dispatched; its run hasn't finished (epoch ms)
    Running {
        since: i64,
    },
    Completed,
    Failed {
        error: Option<String>,
    },
    /// Due, but no run was started (trigger off, rate limited, or missing)
    NotRun,
}

impl PeerMessage {
    /// The task the message is about (a duty's ID for duty messages)
    pub fn task_id(&self) -> &str {
        match self {
            Self::AcknowledgeTask { task_id, .. }
            | Self::RejectTask { task_id, .. }
            | Self::ProgressUpdate { task_id, .. }
            | Self::StatusQuery { task_id }
            | Self::StatusResponse { task_id, .. }
            | Self::CancelTask { task_id, .. }
            | Self::TaskDelivered { task_id }
            | Self::TaskTimedOut { task_id, .. }
            | Self::ClarificationTimeout { task_id }
            | Self::ReassignTask { task_id, .. } => task_id,
            // Not a task; its ID never matches one
            Self::DutyQuery { duty_id } | Self::DutyStatus { duty_id, .. } => duty_id,
            Self::ClarificationRequest(request) => &request.task_id,
            Self::ClarificationResponse(response) => &response.task_id,
        }
    }
}

/// A `Peer` (or `Undeliverable`) message as the receiving client sees it
#[derive(Debug, Clone, PartialEq)]
pub struct PeerDelivery {
    /// Who sent it (for `Undeliverable`: us)
    pub from_instance: String,
    pub target_instance: String,
    pub body: PeerMessage,
    /// The relay couldn't reach `target_instance`; `body` is ours, returned
    pub undeliverable: bool,
}

/// A task reached a provider. The provider's handler runs it; the
/// requester's handler only learns where it went.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskAssigned {
    pub task_id: String,
    pub provider_instance: String,
    pub task: Option<Task>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCompleted {
    pub task_id: String,
    pub result: TaskResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskFailed {
    pub task_id: String,
    pub error: String,
}

/// The `QueryCapability` capability that matches every registered card
pub const ALL_CAPABILITIES: &str = "*";

/// The `Error` code of a registration refused for its protocol version
pub const VERSION_MISMATCH: &str = "version_mismatch";

impl RelayMessage {
    /// One JSON text frame
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("Failed to encode a relay message: {e}"))
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| format!("Invalid relay message: {e}"))
    }
}
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::clarification::Question;
    use crate::directory::{Availability, MatchedAgent, ServedCapability};
    use crate::fixtures::{card, gateway_card, result, task};

    fn every_message() -> Vec<RelayMessage> {
        vec![
            RelayMessage::RegisterInstance {
                instance_id: "i1".into(),
                agent_cards: vec![card()],
                signature: "sig".into(),
                capabilities: vec![],
                tools: vec![],
                protocol_version: crate::PROTOCOL_VERSION,
            },
            RelayMessage::Heartbeat {
                instance_id: "i1".into(),
                load: HashMap::from([("papers".to_string(), 1)]),
                ts: 42.0,
            },
            RelayMessage::Deregister {
                instance_id: "i1".into(),
            },
            RelayMessage::TaskResult {
                task_id: "t1".into(),
                result: result(),
                signature: "sig".into(),
            },
            RelayMessage::QueryCapability {
                capability: "find-papers".into(),
                requester_instance: "i2".into(),
            },
            RelayMessage::DelegateTask {
                task_id: "t1".into(),
                target_agent: "i2/writer".into(),
                capability: "write".into(),
                input: json!({"topic": "rust"}),
                requester_card: card(),
                deadline: None,
                signature: "sig".into(),
            },
            RelayMessage::Registered {
                instance_id: "i1".into(),
            },
            RelayMessage::CapabilityResult {
                capability: "find-papers".into(),
                agents: vec![card()],
            },
            RelayMessage::TaskAssigned(TaskAssigned {
                task_id: "t1".into(),
                provider_instance: "i2".into(),
                task: Some(task()),
            }),
            RelayMessage::TaskAssigned(TaskAssigned {
                task_id: "t1".into(),
                provider_instance: "i2".into(),
                task: None,
            }),
            RelayMessage::TaskCompleted(TaskCompleted {
                task_id: "t1".into(),
                result: result(),
            }),
            RelayMessage::TaskFailed(TaskFailed {
                task_id: "t1".into(),
                error: "boom".into(),
            }),
            RelayMessage::Error {
                code: "unauthorized".into(),
                message: "bad token".into(),
            },
            RelayMessage::RegisterGateway {
                instance_id: "i1".into(),
                card: gateway_card(),
            },
            RelayMessage::QueryDirectory { since: None },
            RelayMessage::QueryDirectory { since: Some(7.0) },
            RelayMessage::Directory {
                members: vec![MemberRecord {
                    card: gateway_card(),
                    online: false,
                    last_seen_ms: 9.0,
                }],
            },
            RelayMessage::FindPeople {
                topic: "SOX audit".into(),
                limit: 5,
            },
            RelayMessage::PeopleMatches {
                topic: "SOX audit".into(),
                limit: 5,
                matches: vec![PeopleMatch {
                    instance_id: "i1".into(),
                    name: "Ada".into(),
                    handle: Some("ada".into()),
                    online: true,
                    last_seen_ms: Some(9.0),
                    takes_requests: true,
                    ask_capability: crate::directory::ASK_ME.into(),
                    responsibilities: vec!["SOX audit".into()],
                    description: String::new(),
                    matched_on: vec!["responsibilities".into()],
                    excerpt: None,
                    agents: vec![MatchedAgent {
                        target: "i1/papers".into(),
                        name: "Papers".into(),
                        description: String::new(),
                        serves: vec![ServedCapability {
                            capability: "find-papers".into(),
                            description: String::new(),
                        }],
                    }],
                    agents_online: 1,
                    availability: Availability::available(),
                }],
                searched: 3,
                unstated: 1,
                total: 1,
            },
            RelayMessage::Undeliverable {
                target_instance: "i2".into(),
                body: PeerMessage::StatusQuery {
                    task_id: "t1".into(),
                },
            },
        ]
        .into_iter()
        .chain(
            every_peer_message()
                .into_iter()
                .map(|body| RelayMessage::Peer {
                    from_instance: "i1".into(),
                    target_instance: "i2".into(),
                    body,
                }),
        )
        .collect()
    }

    fn every_peer_message() -> Vec<PeerMessage> {
        vec![
            PeerMessage::AcknowledgeTask {
                task_id: "t1".into(),
                eta_seconds: Some(30),
                note: None,
            },
            PeerMessage::RejectTask {
                task_id: "t1".into(),
                reason: "busy".into(),
                reason_kind: Some(ReasonKind::NotTrusted),
            },
            PeerMessage::ProgressUpdate {
                task_id: "t1".into(),
                percent: Some(50),
                note: Some("halfway".into()),
                reason_kind: None,
                queue_position: None,
            },
            PeerMessage::ProgressUpdate {
                task_id: "t1".into(),
                percent: None,
                note: Some("in line".into()),
                reason_kind: Some(ReasonKind::Queued),
                queue_position: Some(2),
            },
            PeerMessage::StatusQuery {
                task_id: "t1".into(),
            },
            PeerMessage::StatusResponse {
                task_id: "t1".into(),
                state: TaskState::Working,
                eta_seconds: Some(10),
                elapsed_seconds: 20,
                result: None,
                error: None,
            },
            PeerMessage::StatusResponse {
                task_id: "t1".into(),
                state: TaskState::Completed,
                eta_seconds: None,
                elapsed_seconds: 90,
                result: Some(serde_json::json!({ "done": true })),
                error: None,
            },
            PeerMessage::CancelTask {
                task_id: "t1".into(),
                reason: None,
            },
            PeerMessage::TaskDelivered {
                task_id: "t1".into(),
            },
            PeerMessage::TaskTimedOut {
                task_id: "t1".into(),
                reason: Some("3x ETA".into()),
            },
            PeerMessage::ClarificationRequest(ClarificationRequest {
                task_id: "t1".into(),
                round: 1,
                questions: vec![Question {
                    id: "q1".into(),
                    question: "Which year?".into(),
                    topic: "scope".into(),
                    requires_human: false,
                }],
            }),
            PeerMessage::ClarificationResponse(ClarificationResponse {
                task_id: "t1".into(),
                round: 1,
                answers: HashMap::from([("q1".to_string(), "2024".to_string())]),
            }),
            PeerMessage::ClarificationTimeout {
                task_id: "t1".into(),
            },
        ]
    }

    #[test]
    fn peer_messages_name_their_task() {
        for body in every_peer_message() {
            assert_eq!(body.task_id(), "t1");
        }
        let value = serde_json::to_value(RelayMessage::Peer {
            from_instance: String::new(),
            target_instance: "i2".into(),
            body: PeerMessage::StatusQuery {
                task_id: "t1".into(),
            },
        })
        .unwrap();
        assert_eq!(
            value,
            json!({"type": "peer", "fromInstance": "", "targetInstance": "i2",
                   "body": {"kind": "status_query", "taskId": "t1"}})
        );
    }

    #[test]
    fn every_message_round_trips() {
        for message in every_message() {
            let text = message.to_json().unwrap();
            assert_eq!(RelayMessage::from_json(&text).unwrap(), message, "{text}");
        }
    }

    #[test]
    fn messages_are_tagged_snake_case_with_camel_case_fields() {
        let value = serde_json::to_value(RelayMessage::RegisterInstance {
            instance_id: "i1".into(),
            agent_cards: vec![],
            signature: "s".into(),
            capabilities: vec![],
            tools: vec![],
            protocol_version: 2,
        })
        .unwrap();
        assert_eq!(
            value,
            json!({"type": "register_instance", "instanceId": "i1", "agentCards": [],
                   "signature": "s", "protocolVersion": 2})
        );
        // A client that doesn't say reads as version 0
        let old = RelayMessage::from_json(
            r#"{"type": "register_instance", "instanceId": "i1", "agentCards": [], "signature": "s"}"#,
        )
        .unwrap();
        assert!(matches!(
            old,
            RelayMessage::RegisterInstance {
                protocol_version: 0,
                ..
            }
        ));
        assert_eq!(
            serde_json::to_value(RelayMessage::FindPeople {
                topic: "x".into(),
                limit: 3
            })
            .unwrap(),
            json!({"type": "find_people", "topic": "x", "limit": 3})
        );
        let value = serde_json::to_value(RelayMessage::TaskFailed(TaskFailed {
            task_id: "t1".into(),
            error: "e".into(),
        }))
        .unwrap();
        assert_eq!(
            value,
            json!({"type": "task_failed", "taskId": "t1", "error": "e"})
        );
        let value = serde_json::to_value(RelayMessage::QueryCapability {
            capability: "x".into(),
            requester_instance: "i2".into(),
        })
        .unwrap();
        assert_eq!(
            value,
            json!({"type": "query_capability", "capability": "x", "requesterInstance": "i2"})
        );
    }

    #[test]
    fn unknown_messages_are_rejected() {
        assert!(RelayMessage::from_json(r#"{"type": "nope"}"#).is_err());
        assert!(RelayMessage::from_json("not json").is_err());
    }

    #[test]
    fn reassign_and_duty_messages_round_trip() {
        let bodies = [
            PeerMessage::ReassignTask {
                task_id: "t1".into(),
                new_provider_instance: "i3".into(),
                new_provider_card: "i3/writer".into(),
                new_provider_name: Some("Writer".into()),
                by: "Ada".into(),
                reason: None,
            },
            PeerMessage::DutyQuery {
                duty_id: "standup/2026-11-01/09:00".into(),
            },
            PeerMessage::DutyStatus {
                duty_id: "standup/2026-11-01/09:00".into(),
                state: DutyState::Running { since: 7 },
            },
            PeerMessage::DutyStatus {
                duty_id: "standup/2026-11-01/09:00".into(),
                state: DutyState::NotRun,
            },
        ];
        for body in bodies {
            let message = RelayMessage::Peer {
                from_instance: "i1".into(),
                target_instance: "i2".into(),
                body,
            };
            let text = message.to_json().unwrap();
            assert_eq!(RelayMessage::from_json(&text).unwrap(), message, "{text}");
        }
        assert_eq!(
            serde_json::to_value(DutyState::Upcoming { due_at: 5 }).unwrap(),
            json!({"state": "upcoming", "dueAt": 5})
        );
    }
}
