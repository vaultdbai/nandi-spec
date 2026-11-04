//! The relay's WebSocket frames: JSON text frames, one [`RelayMessage`]
//! each, tagged by `type` (`{"type": "heartbeat", "instanceId": "…", …}`).
//!
//! `QueryCapability` with the capability [`ALL_CAPABILITIES`] (`*`) asks
//! for every registered card (presence). Picking who gets a request is
//! `RouteQuery` → `RoutePlan` (see [`crate::routing`]).
//!
//! `RegisterInstance` carries the client's [`crate::PROTOCOL_VERSION`]. A
//! relay that speaks another version refuses it with an `Error` whose code
//! is [`VERSION_MISMATCH`]. It also carries the instance's public keys,
//! which the relay pins (see [`crate::keys`]).
//!
//! Content is sealed. Every person-to-person payload (a task's input, its
//! output and error, questions, answers, notes, reasons) travels in an
//! [`Envelope`] sealed for the recipient; the relay reads only the
//! metadata beside it (IDs, capability, kind, state, typed reason, ETA,
//! progress, place in line, deadline, display names). What each envelope
//! holds is in [`crate::content`]. A relay refuses a frame whose envelope
//! is over [`crate::MAX_ENVELOPE_BYTES`] ([`crate::ENVELOPE_TOO_LARGE`]).
//!
//! Offline: every request here has a reply or a timeout on the client; a
//! client that isn't connected reports "not connected" and keeps working
//! (cards it last saw, its own agents locally). `Peer` messages to an
//! offline instance come back as `Undeliverable`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::card::AgentCard;
use crate::directory::{MemberRecord, PeopleMatch};
use crate::gateway::GatewayCard;
use crate::keys::{Envelope, InstanceKeys, PublicKeys};
use crate::presence::{CapabilityPresence, InstancePresence};
use crate::routing::{RoutePlan, RoutingQuery};
use crate::task::{ReasonKind, ResultReport, SealedTask, TaskState};
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
        /// The instance's keys. The relay pins them on first sight and
        /// refuses others with [`crate::KEY_MISMATCH`]; left out, the pin
        /// stays as it is.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        public_keys: Option<PublicKeys>,
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
    /// The output and error sealed for the requester
    TaskResult {
        task_id: String,
        result: ResultReport,
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
        requester_card: AgentCard,
        deadline: Option<Timestamp>,
        signature: String,
        /// The input, sealed for the target's instance
        envelope: Envelope,
        /// See [`SealedTask::continues`]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        continues: Option<String>,
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

    // Keys (see `keys`)
    /// Instance → relay: the keys pinned for these instances
    PublicKeysQuery {
        instance_ids: Vec<String>,
    },
    /// Relay → instance: one entry per ID asked, in that order
    PublicKeysResult {
        keys: Vec<InstanceKeys>,
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

    // Routing (see `routing`)
    /// Instance → relay: who should get a request. `query_id` is the
    /// client's, echoed in the answer.
    RouteQuery {
        query_id: String,
        query: RoutingQuery,
    },
    /// Relay → instance: the candidates, best first
    RoutePlan {
        query_id: String,
        plan: RoutePlan,
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
/// provider (wave final, Tracks B and C), tagged by `kind`. Free text,
/// questions, answers and results go in an `envelope` sealed for the
/// target (see [`crate::content`]); the rest is metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum PeerMessage {
    /// Provider → requester: accepted, with an estimate (and a sealed
    /// note)
    AcknowledgeTask {
        task_id: String,
        eta_seconds: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        envelope: Option<Envelope>,
    },
    /// Provider → requester: refused (policy, trust, capacity), with the
    /// reason sealed
    RejectTask {
        task_id: String,
        /// Why, as the requester's UI matches it to a fix; `None` when
        /// there is no typed reason
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason_kind: Option<ReasonKind>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        envelope: Option<Envelope>,
    },
    /// Provider → requester, only when the task asked for progress (a hold
    /// or a place in line is always sent)
    ProgressUpdate {
        task_id: String,
        percent: Option<u8>,
        /// Why it waits, when it waits (held for approval, queued)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason_kind: Option<ReasonKind>,
        /// 1-based place in the provider's line, while queued
        #[serde(default, skip_serializing_if = "Option::is_none")]
        queue_position: Option<u32>,
        /// The note
        #[serde(default, skip_serializing_if = "Option::is_none")]
        envelope: Option<Envelope>,
    },
    /// Requester → provider
    StatusQuery { task_id: String },
    /// Provider → requester
    StatusResponse {
        task_id: String,
        state: TaskState,
        eta_seconds: Option<u32>,
        elapsed_seconds: u32,
        /// Once it ended: its output and error, so a requester that was
        /// offline when the result was sent still gets it (a lasting
        /// assignment)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        envelope: Option<Envelope>,
    },
    /// Either side, with the reason sealed
    CancelTask {
        task_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        envelope: Option<Envelope>,
    },
    /// Provider → requester: the task reached the provider
    TaskDelivered { task_id: String },
    /// Requester → provider: gave up waiting, with the reason sealed
    TaskTimedOut {
        task_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        envelope: Option<Envelope>,
    },
    /// Provider → requester: questions, sealed
    ClarificationRequest {
        task_id: String,
        /// 1-based
        round: u32,
        envelope: Envelope,
    },
    /// Requester → provider: answers, sealed
    ClarificationResponse {
        task_id: String,
        round: u32,
        envelope: Envelope,
    },
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
        /// The reason, sealed for the requester
        #[serde(default, skip_serializing_if = "Option::is_none")]
        envelope: Option<Envelope>,
        /// The task's request, resealed by the provider for the new
        /// provider (the requester's signature kept). The requester sends
        /// it on as the next attempt's envelope, with `continues` set to
        /// this task.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request: Option<Box<Envelope>>,
    },
    /// A calendar trigger's backup → its primary: how is this duty going?
    /// (`duty_id` is `<trigger>/<date>/<time>`, see `domain::calendar`)
    DutyQuery { duty_id: String },
    /// Primary → backup; a failure's error is sealed (a note bound to the
    /// duty ID)
    DutyStatus {
        duty_id: String,
        state: DutyState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        envelope: Option<Envelope>,
    },
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
    /// Its error, if any, is in the `DutyStatus` envelope
    Failed,
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
            | Self::ClarificationRequest { task_id, .. }
            | Self::ClarificationResponse { task_id, .. }
            | Self::ReassignTask { task_id, .. } => task_id,
            // Not a task; its ID never matches one
            Self::DutyQuery { duty_id } | Self::DutyStatus { duty_id, .. } => duty_id,
        }
    }

    /// Every envelope it carries
    pub fn envelopes(&self) -> Vec<&Envelope> {
        match self {
            Self::AcknowledgeTask { envelope, .. }
            | Self::RejectTask { envelope, .. }
            | Self::ProgressUpdate { envelope, .. }
            | Self::StatusResponse { envelope, .. }
            | Self::CancelTask { envelope, .. }
            | Self::TaskTimedOut { envelope, .. }
            | Self::DutyStatus { envelope, .. } => envelope.iter().collect(),
            Self::ClarificationRequest { envelope, .. }
            | Self::ClarificationResponse { envelope, .. } => vec![envelope],
            Self::ReassignTask {
                envelope, request, ..
            } => envelope.iter().chain(request.as_deref()).collect(),
            Self::StatusQuery { .. }
            | Self::TaskDelivered { .. }
            | Self::ClarificationTimeout { .. }
            | Self::DutyQuery { .. } => Vec::new(),
        }
    }
}

/// A task reached a provider. The provider's Nandi opens and runs it; the
/// requester's (with no task) only learns where it went.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskAssigned {
    pub task_id: String,
    pub provider_instance: String,
    pub task: Option<SealedTask>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskCompleted {
    pub task_id: String,
    pub result: ResultReport,
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
    /// Every envelope it carries (to check [`crate::MAX_ENVELOPE_BYTES`])
    pub fn envelopes(&self) -> Vec<&Envelope> {
        match self {
            Self::DelegateTask { envelope, .. } => vec![envelope],
            Self::TaskResult { result, .. } => vec![&result.envelope],
            Self::TaskAssigned(assigned) => assigned
                .task
                .as_ref()
                .map(|task| vec![&task.envelope])
                .unwrap_or_default(),
            Self::TaskCompleted(completed) => vec![&completed.result.envelope],
            Self::Peer { body, .. } | Self::Undeliverable { body, .. } => body.envelopes(),
            _ => Vec::new(),
        }
    }

    /// Whether every envelope it carries is within
    /// [`crate::MAX_ENVELOPE_BYTES`]
    pub fn envelopes_within_limit(&self) -> bool {
        self.envelopes().iter().all(|e| e.is_within_limit())
    }

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
    use crate::directory::{Availability, MatchedAgent, ServedCapability};
    use crate::fixtures::{card, envelope, gateway_card, report, sealed_task};
    use crate::keys::{SealPurpose, MAX_ENVELOPE_BYTES};
    use crate::routing::{
        NoRoute, RequestKind, RouteCandidate, RouteReason, RouteStrategy, RouteTarget, RoutingHints,
    };

    fn every_message() -> Vec<RelayMessage> {
        vec![
            RelayMessage::RegisterInstance {
                instance_id: "i1".into(),
                agent_cards: vec![card()],
                signature: "sig".into(),
                capabilities: vec![],
                tools: vec![],
                protocol_version: crate::PROTOCOL_VERSION,
                public_keys: Some(PublicKeys {
                    signing: "s".repeat(43),
                    sealing: "x".repeat(43),
                }),
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
                result: report(),
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
                requester_card: card(),
                deadline: None,
                signature: "sig".into(),
                envelope: envelope(SealPurpose::Request),
                continues: None,
            },
            RelayMessage::DelegateTask {
                task_id: "t2".into(),
                target_agent: "i3/writer".into(),
                capability: "write".into(),
                requester_card: card(),
                deadline: Some(9.0),
                signature: "sig".into(),
                envelope: envelope(SealPurpose::Request),
                continues: Some("t1".into()),
            },
            RelayMessage::PublicKeysQuery {
                instance_ids: vec!["i2".into()],
            },
            RelayMessage::PublicKeysResult {
                keys: vec![
                    InstanceKeys {
                        instance_id: "i2".into(),
                        public_keys: Some(PublicKeys {
                            signing: "s".repeat(43),
                            sealing: "x".repeat(43),
                        }),
                    },
                    InstanceKeys {
                        instance_id: "i9".into(),
                        public_keys: None,
                    },
                ],
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
                task: Some(sealed_task()),
            }),
            RelayMessage::TaskAssigned(TaskAssigned {
                task_id: "t1".into(),
                provider_instance: "i2".into(),
                task: None,
            }),
            RelayMessage::TaskCompleted(TaskCompleted {
                task_id: "t1".into(),
                result: report(),
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
                    public_keys: None,
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
            RelayMessage::RouteQuery {
                query_id: "q1".into(),
                query: RoutingQuery {
                    kind: RequestKind::Assign,
                    capability: crate::directory::ASK_ME.into(),
                    target: RouteTarget::Person {
                        instance_id: "i2".into(),
                    },
                    hints: RoutingHints {
                        preferred: vec!["i2/papers".into()],
                        excluded: vec!["i3".into()],
                        strategy: Some(RouteStrategy::RoundRobin),
                        required_tools: vec!["builtin.shell".into()],
                        max_candidates: Some(2),
                    },
                },
            },
            RelayMessage::RoutePlan {
                query_id: "q1".into(),
                plan: RoutePlan {
                    plan_id: "p1".into(),
                    expires_at: 9.0,
                    candidates: vec![RouteCandidate {
                        target: "i2/papers".into(),
                        instance_id: "i2".into(),
                        name: "Bob".into(),
                        handle: Some("bob".into()),
                        agent_name: "Papers".into(),
                        provider_kind: crate::ProviderKind::Agent,
                        online: true,
                        load: 3,
                        max_concurrent: 3,
                        wait_seconds: Some(1200),
                        redirected_from: Some("i4".into()),
                        reasons: vec![
                            RouteReason::Covers {
                                name: "Bob".into(),
                                away_name: "Alice".into(),
                                until: Some(7.0),
                            },
                            RouteReason::Busy {
                                name: "Bob".into(),
                                load: 3,
                                max: 3,
                                wait_minutes: Some(20),
                            },
                        ],
                        public_keys: None,
                    }],
                    no_route: None,
                },
            },
            RelayMessage::RoutePlan {
                query_id: "q2".into(),
                plan: RoutePlan {
                    plan_id: "p2".into(),
                    expires_at: 9.0,
                    candidates: vec![],
                    no_route: Some(NoRoute::NotTakingRequests),
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
                envelope: None,
            },
            PeerMessage::RejectTask {
                task_id: "t1".into(),
                reason_kind: Some(ReasonKind::NotTrusted),
                envelope: Some(envelope(SealPurpose::Note)),
            },
            PeerMessage::ProgressUpdate {
                task_id: "t1".into(),
                percent: Some(50),
                reason_kind: None,
                queue_position: None,
                envelope: Some(envelope(SealPurpose::Note)),
            },
            PeerMessage::ProgressUpdate {
                task_id: "t1".into(),
                percent: None,
                reason_kind: Some(ReasonKind::Queued),
                queue_position: Some(2),
                envelope: None,
            },
            PeerMessage::StatusQuery {
                task_id: "t1".into(),
            },
            PeerMessage::StatusResponse {
                task_id: "t1".into(),
                state: TaskState::Working,
                eta_seconds: Some(10),
                elapsed_seconds: 20,
                envelope: None,
            },
            PeerMessage::StatusResponse {
                task_id: "t1".into(),
                state: TaskState::Completed,
                eta_seconds: None,
                elapsed_seconds: 90,
                envelope: Some(envelope(SealPurpose::Result)),
            },
            PeerMessage::CancelTask {
                task_id: "t1".into(),
                envelope: None,
            },
            PeerMessage::TaskDelivered {
                task_id: "t1".into(),
            },
            PeerMessage::TaskTimedOut {
                task_id: "t1".into(),
                envelope: Some(envelope(SealPurpose::Note)),
            },
            PeerMessage::ClarificationRequest {
                task_id: "t1".into(),
                round: 1,
                envelope: envelope(SealPurpose::Clarification),
            },
            PeerMessage::ClarificationResponse {
                task_id: "t1".into(),
                round: 1,
                envelope: envelope(SealPurpose::Answer),
            },
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
            protocol_version: 3,
            public_keys: None,
        })
        .unwrap();
        assert_eq!(
            value,
            json!({"type": "register_instance", "instanceId": "i1", "agentCards": [],
                   "signature": "s", "protocolVersion": 3})
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
    fn envelopes_are_found_wherever_a_message_carries_one() {
        let counts: Vec<usize> = every_message()
            .iter()
            .map(|message| message.envelopes().len())
            .collect();
        assert_eq!(counts.iter().sum::<usize>(), 11, "{counts:?}");
        for message in every_message() {
            assert!(message.envelopes_within_limit());
        }
        let mut big = envelope(SealPurpose::Request);
        big.ciphertext = "c".repeat(MAX_ENVELOPE_BYTES);
        let message = RelayMessage::Peer {
            from_instance: "i1".into(),
            target_instance: "i2".into(),
            body: PeerMessage::ReassignTask {
                task_id: "t1".into(),
                new_provider_instance: "i3".into(),
                new_provider_card: "i3/writer".into(),
                new_provider_name: None,
                by: "Ada".into(),
                envelope: Some(envelope(SealPurpose::Note)),
                request: Some(Box::new(big)),
            },
        };
        assert_eq!(message.envelopes().len(), 2);
        assert!(!message.envelopes_within_limit());
    }

    #[test]
    fn no_content_field_crosses_in_the_clear() {
        // The content fields of protocol 3 are gone. (`TaskFailed` and
        // `Error` carry the relay's own words, not a person's.)
        for message in every_message() {
            if matches!(
                message,
                RelayMessage::TaskFailed(_) | RelayMessage::Error { .. }
            ) {
                continue;
            }
            let text = message.to_json().unwrap();
            for field in [
                "\"input\":",
                "\"output\":",
                "\"note\":",
                "\"reason\":",
                "\"questions\":",
                "\"answers\":",
                "\"error\":\"",
                "\"result\":{\"done",
            ] {
                assert!(!text.contains(field), "{field} in {text}");
            }
        }
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
                envelope: None,
                request: Some(Box::new(envelope(SealPurpose::Request))),
            },
            PeerMessage::DutyQuery {
                duty_id: "standup/2026-11-01/09:00".into(),
            },
            PeerMessage::DutyStatus {
                duty_id: "standup/2026-11-01/09:00".into(),
                state: DutyState::Running { since: 7 },
                envelope: None,
            },
            PeerMessage::DutyStatus {
                duty_id: "standup/2026-11-01/09:00".into(),
                state: DutyState::Failed,
                envelope: Some(envelope(SealPurpose::Note)),
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
