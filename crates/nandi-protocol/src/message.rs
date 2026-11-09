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
//! Tasks are stored and forwarded (see [`crate::mail`]): `SubmitTask`
//! carries the request sealed for each candidate; the relay delivers it
//! (now, or when the provider is back), fails it over, ends it at its due
//! date, and keeps everything for a party in its mailbox (`Deliver`) until
//! it's acked (`Ack`). Reassignment (`Reassign`) and calendar handovers
//! (`DutyWatch`, `DutyReport`, see [`crate::duty`]) are decided at the
//! relay too.
//!
//! Offline: every request here has a reply or a timeout on the client; a
//! client that isn't connected reports "not connected" and keeps working
//! (cards it last saw, its own agents locally). What it sent before waits
//! at the relay.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::card::AgentCard;
use crate::directory::{MemberRecord, PeopleMatch};
use crate::duty::DutyState;
use crate::gateway::GatewayCard;
use crate::group::{GroupChange, GroupOp, GroupOutcome, Visibility};
use crate::keys::{Envelope, InstanceKeys, PublicKeys};
use crate::mail::Delivery;
use crate::presence::{CapabilityPresence, InstancePresence};
use crate::routing::{RequestKind, RoutePlan, RoutingQuery};
use crate::task::{ReasonKind, ResultReport, TaskAttempt, TaskState};
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
    /// The output and error sealed for the requester; kept for it until
    /// it acks the delivery
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
    /// A request for another instance, sealed for each candidate in turn
    /// (best first, at most [`crate::MAX_CANDIDATES`]). The relay answers
    /// with a [`crate::TaskEvent`] (`accepted` or `refused`) in the
    /// requester's mailbox, delivers the first attempt (now, or when its
    /// provider is back) and fails over to the next. An assignment gets
    /// one attempt: it's meant for that person.
    SubmitTask {
        task_id: String,
        kind: RequestKind,
        capability: String,
        requester_card: AgentCard,
        deadline: Option<Timestamp>,
        signature: String,
        attempts: Vec<TaskAttempt>,
    },

    // Relay → both
    /// Registration accepted
    Registered {
        instance_id: String,
        /// Who sees whom on this relay
        #[serde(default)]
        visibility: Visibility,
    },
    CapabilityResult {
        capability: String,
        agents: Vec<AgentCard>,
    },
    Error {
        code: String,
        message: String,
    },

    // Store-and-forward (see `mail`)
    /// Relay → instance: one item from its mailbox
    Deliver {
        mail_id: u64,
        item: Delivery,
    },
    /// Instance → relay: it has these; the relay stops sending them
    Ack {
        mail_ids: Vec<u64>,
    },
    /// Instance → relay, once registered: send everything not acked yet
    InboxSync {},

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

    // Groups (see `group`)
    /// Instance → relay
    GroupRequest {
        request_id: String,
        op: GroupOp,
    },
    /// Relay → instance, for the `GroupRequest` with this `request_id`
    GroupReply {
        request_id: String,
        outcome: GroupOutcome,
    },
    /// Relay → each member online (and someone invited): the group changed
    GroupChanged {
        group_id: String,
        change: GroupChange,
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

    /// Instance → relay: a lifecycle message for the other side of a task.
    /// The relay checks the sender is a party to it, keeps it in the
    /// other's mailbox (live-only kinds go now or not at all, see
    /// [`PeerMessage::is_live_only`]) and delivers it as
    /// [`Delivery::Peer`].
    Peer {
        target_instance: String,
        body: PeerMessage,
    },

    // Reassignment
    /// Instance → relay: hand `task_id` to `target`. From its provider
    /// (its own move, or a manager's [`Delivery::Reseal`]), or from its
    /// requester answering a `Reseal`. `request` is the request sealed
    /// for the new provider (the requester's signature kept); a provider
    /// without it leaves it out, and the relay asks the requester to
    /// seal it. The relay checks the attempt chain, so nobody gets the
    /// task twice.
    Reassign {
        task_id: String,
        /// The new provider's card
        target: String,
        /// Who moved it (a display name)
        by: String,
        /// The reason, sealed for the requester
        #[serde(default, skip_serializing_if = "Option::is_none")]
        envelope: Option<Envelope>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request: Option<Box<Envelope>>,
    },
    /// Relay → the instance that sent `Reassign`: done (`error` empty), or
    /// why not
    ReassignResult {
        task_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    // Calendar handovers (see `duty`)
    /// Backup → relay: watch this duty of `primary_instance`; a watch sent
    /// again replaces the earlier one
    DutyWatch {
        duty_id: String,
        primary_instance: String,
        /// When it's due (epoch ms)
        due_at: i64,
        /// Hand over when the primary's run takes longer than this
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after_minutes: Option<u32>,
    },
    /// Primary → relay: how it's doing with a duty it was asked about. A
    /// failure's error is sealed for the backup (a note bound to the duty
    /// ID).
    DutyReport {
        duty_id: String,
        state: DutyState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        envelope: Option<Envelope>,
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
/// provider, tagged by `kind`. Free text, questions, answers and results
/// go in an `envelope` sealed for the target (see [`crate::content`]); the
/// rest is metadata.
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
    /// reason sealed. The relay gives the task to the next candidate
    /// instead of passing this on, while there is one (never for a
    /// declined assignment).
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
    /// Requester → provider (live only)
    StatusQuery { task_id: String },
    /// Provider → requester (live only)
    StatusResponse {
        task_id: String,
        state: TaskState,
        eta_seconds: Option<u32>,
        elapsed_seconds: u32,
    },
    /// Either side, with the reason sealed
    CancelTask {
        task_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        envelope: Option<Envelope>,
    },
    /// Provider → requester: the task reached the provider
    TaskDelivered { task_id: String },
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
}

impl PeerMessage {
    /// The task the message is about
    pub fn task_id(&self) -> &str {
        match self {
            Self::AcknowledgeTask { task_id, .. }
            | Self::RejectTask { task_id, .. }
            | Self::ProgressUpdate { task_id, .. }
            | Self::StatusQuery { task_id }
            | Self::StatusResponse { task_id, .. }
            | Self::CancelTask { task_id, .. }
            | Self::TaskDelivered { task_id }
            | Self::ClarificationTimeout { task_id }
            | Self::ClarificationRequest { task_id, .. }
            | Self::ClarificationResponse { task_id, .. } => task_id,
        }
    }

    /// Every envelope it carries
    pub fn envelopes(&self) -> Vec<&Envelope> {
        match self {
            Self::AcknowledgeTask { envelope, .. }
            | Self::RejectTask { envelope, .. }
            | Self::ProgressUpdate { envelope, .. }
            | Self::CancelTask { envelope, .. } => envelope.iter().collect(),
            Self::ClarificationRequest { envelope, .. }
            | Self::ClarificationResponse { envelope, .. } => vec![envelope],
            Self::StatusQuery { .. }
            | Self::StatusResponse { .. }
            | Self::TaskDelivered { .. }
            | Self::ClarificationTimeout { .. } => Vec::new(),
        }
    }

    /// Whether it matters only to someone online right now: a status
    /// query and its answer, and progress that is neither a hold nor a
    /// place in line ("still working"). The relay doesn't store these.
    pub fn is_live_only(&self) -> bool {
        match self {
            Self::StatusQuery { .. } | Self::StatusResponse { .. } => true,
            Self::ProgressUpdate {
                reason_kind,
                queue_position,
                ..
            } => reason_kind.is_none() && queue_position.is_none(),
            _ => false,
        }
    }
}

/// The `QueryCapability` capability that matches every registered card
pub const ALL_CAPABILITIES: &str = "*";

/// The `Error` code of a registration refused for its protocol version
pub const VERSION_MISMATCH: &str = "version_mismatch";

impl RelayMessage {
    /// Every envelope it carries (to check [`crate::MAX_ENVELOPE_BYTES`])
    pub fn envelopes(&self) -> Vec<&Envelope> {
        match self {
            Self::SubmitTask { attempts, .. } => attempts.iter().map(|a| &a.envelope).collect(),
            Self::TaskResult { result, .. } => vec![&result.envelope],
            Self::Deliver { item, .. } => item.envelopes(),
            Self::Peer { body, .. } => body.envelopes(),
            Self::Reassign {
                envelope, request, ..
            } => envelope.iter().chain(request.as_deref()).collect(),
            Self::DutyReport { envelope, .. } => envelope.iter().collect(),
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
    use crate::duty::DutyState;
    use crate::fixtures::{card, envelope, gateway_card, report};
    use crate::keys::{SealPurpose, MAX_ENVELOPE_BYTES};
    use crate::mail::tests::every_delivery;
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
            RelayMessage::SubmitTask {
                task_id: "t1".into(),
                kind: RequestKind::Delegate,
                capability: "write".into(),
                requester_card: card(),
                deadline: Some(9.0),
                signature: "sig".into(),
                attempts: vec![
                    TaskAttempt {
                        target: "i2/writer".into(),
                        envelope: envelope(SealPurpose::Request),
                    },
                    TaskAttempt {
                        target: "i3/writer".into(),
                        envelope: envelope(SealPurpose::Request),
                    },
                ],
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
                visibility: crate::group::Visibility::Groups,
            },
            RelayMessage::GroupRequest {
                request_id: "r1".into(),
                op: crate::group::GroupOp::Join { code: "abc".into() },
            },
            RelayMessage::GroupReply {
                request_id: "r1".into(),
                outcome: crate::group::GroupOutcome::Done,
            },
            RelayMessage::GroupChanged {
                group_id: "g1".into(),
                change: crate::group::GroupChange::PolicyChanged { version: 2 },
            },
            RelayMessage::CapabilityResult {
                capability: "find-papers".into(),
                agents: vec![card()],
            },
            RelayMessage::Error {
                code: "unauthorized".into(),
                message: "bad token".into(),
            },
            RelayMessage::Ack {
                mail_ids: vec![1, 2],
            },
            RelayMessage::InboxSync {},
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
            RelayMessage::Reassign {
                task_id: "t1".into(),
                target: "i3/writer".into(),
                by: "Ada".into(),
                envelope: Some(envelope(SealPurpose::Note)),
                request: Some(Box::new(envelope(SealPurpose::Request))),
            },
            RelayMessage::Reassign {
                task_id: "t1".into(),
                target: "i3/writer".into(),
                by: "Ada".into(),
                envelope: None,
                request: None,
            },
            RelayMessage::ReassignResult {
                task_id: "t1".into(),
                error: Some("Carol already had it".into()),
            },
            RelayMessage::DutyWatch {
                duty_id: "standup/2026-11-05/09:00".into(),
                primary_instance: "i2".into(),
                due_at: 7,
                after_minutes: Some(30),
            },
            RelayMessage::DutyReport {
                duty_id: "standup/2026-11-05/09:00".into(),
                state: DutyState::Failed,
                envelope: Some(envelope(SealPurpose::Note)),
            },
            RelayMessage::DutyReport {
                duty_id: "standup/2026-11-05/09:00".into(),
                state: DutyState::Running { since: 7 },
                envelope: None,
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
                    target_instance: "i2".into(),
                    body,
                }),
        )
        .chain(
            every_delivery()
                .into_iter()
                .enumerate()
                .map(|(n, item)| RelayMessage::Deliver {
                    mail_id: n as u64 + 1,
                    item,
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
            },
            PeerMessage::CancelTask {
                task_id: "t1".into(),
                envelope: None,
            },
            PeerMessage::TaskDelivered {
                task_id: "t1".into(),
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
            target_instance: "i2".into(),
            body: PeerMessage::StatusQuery {
                task_id: "t1".into(),
            },
        })
        .unwrap();
        assert_eq!(
            value,
            json!({"type": "peer", "targetInstance": "i2",
                   "body": {"kind": "status_query", "taskId": "t1"}})
        );
    }

    #[test]
    fn only_queries_and_plain_progress_are_live_only() {
        let live: Vec<bool> = every_peer_message()
            .iter()
            .map(PeerMessage::is_live_only)
            .collect();
        assert_eq!(
            live,
            vec![false, false, true, false, true, true, false, false, false, false, false]
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
        assert_eq!(
            serde_json::to_value(RelayMessage::InboxSync {}).unwrap(),
            json!({"type": "inbox_sync"})
        );
        assert_eq!(
            serde_json::to_value(RelayMessage::Ack { mail_ids: vec![4] }).unwrap(),
            json!({"type": "ack", "mailIds": [4]})
        );
        let value = serde_json::to_value(RelayMessage::Deliver {
            mail_id: 9,
            item: crate::Delivery::Reseal {
                task_id: "t1".into(),
                target: "i3/gateway".into(),
                by: "Ada".into(),
            },
        })
        .unwrap();
        assert_eq!(
            value,
            json!({"type": "deliver", "mailId": 9,
                   "item": {"item": "reseal", "taskId": "t1", "target": "i3/gateway", "by": "Ada"}})
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
        // SubmitTask 2, TaskResult 1, Reassign 2, DutyReport 1, peers 4,
        // deliveries 5
        assert_eq!(counts.iter().sum::<usize>(), 15, "{counts:?}");
        for message in every_message() {
            assert!(message.envelopes_within_limit());
        }
        let mut big = envelope(SealPurpose::Request);
        big.ciphertext = "c".repeat(MAX_ENVELOPE_BYTES);
        let message = RelayMessage::Reassign {
            task_id: "t1".into(),
            target: "i3/writer".into(),
            by: "Ada".into(),
            envelope: Some(envelope(SealPurpose::Note)),
            request: Some(Box::new(big.clone())),
        };
        assert_eq!(message.envelopes().len(), 2);
        assert!(!message.envelopes_within_limit());
        let submit = RelayMessage::SubmitTask {
            task_id: "t1".into(),
            kind: RequestKind::Delegate,
            capability: "write".into(),
            requester_card: card(),
            deadline: None,
            signature: "sig".into(),
            attempts: vec![TaskAttempt {
                target: "i2/writer".into(),
                envelope: big,
            }],
        };
        assert!(!submit.envelopes_within_limit());
    }

    #[test]
    fn no_content_field_crosses_in_the_clear() {
        // The content fields of protocol 3 are gone. (`Error` and a refused
        // task's message carry the relay's own words, not a person's.)
        for message in every_message() {
            if matches!(message, RelayMessage::Error { .. }) {
                continue;
            }
            let text = message.to_json().unwrap();
            for field in [
                "\"input\":",
                "\"output\":",
                "\"note\":",
                "\"reason\":\"",
                "\"questions\":",
                "\"answers\":",
                "\"error\":\"",
                "\"result\":{\"done",
            ] {
                if field == "\"error\":\"" && matches!(message, RelayMessage::ReassignResult { .. })
                {
                    continue;
                }
                assert!(!text.contains(field), "{field} in {text}");
            }
        }
    }

    #[test]
    fn unknown_messages_are_rejected() {
        assert!(RelayMessage::from_json(r#"{"type": "nope"}"#).is_err());
        assert!(RelayMessage::from_json("not json").is_err());
        // Gone in protocol 6
        for gone in [
            "delegate_task",
            "task_assigned",
            "task_completed",
            "undeliverable",
        ] {
            assert!(RelayMessage::from_json(&format!(r#"{{"type": "{gone}"}}"#)).is_err());
        }
    }
}
