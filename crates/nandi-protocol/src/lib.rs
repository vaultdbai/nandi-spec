//! The Nandi relay wire protocol: everything a relay server and its
//! clients exchange, and nothing else.
//!
//! - [`message`]: the WebSocket frames (`RelayMessage`, `PeerMessage`)
//! - [`http`]: the relay's HTTP endpoints (`/health`, `/instances`,
//!   `/relay/enroll`) and connection constants
//! - [`card`], [`gateway`], [`presence`]: what instances publish about
//!   themselves
//! - [`directory`]: the member directory, availability and people search
//! - [`group`]: groups, invites, group policies and visibility by group
//! - [`group_rules`]: the `requests:` rules a relay reads from a group's
//!   policy
//! - [`routing`]: who gets a request (`RouteQuery` → `RoutePlan`)
//! - [`keys`]: per-instance public keys and sealed envelopes
//! - [`content`]: what each sealed envelope holds
//! - [`task`], [`clarification`]: delegated work and its questions
//! - [`mail`]: store-and-forward, each instance's mailbox and task events
//! - [`duty`]: calendar handovers, watched at the relay
//! - [`time`]: timestamps on the wire
//! - [`version`]: the protocol version and the range a relay accepts
//!
//! Serde shapes are the wire format: a field is never renamed or retyped
//! without a [`PROTOCOL_VERSION`] bump, and new fields come with
//! `#[serde(default)]` and no bump. A relay accepts the client versions
//! in its [`ProtocolRange`] (see [`version`] for when to change which
//! number). With the `specta` feature every type also derives
//! `specta::Type`, for apps that export them to TypeScript.

pub mod card;
pub mod clarification;
pub mod content;
pub mod directory;
pub mod duty;
pub mod gateway;
pub mod group;
pub mod group_rules;
pub mod http;
pub mod keys;
pub mod mail;
pub mod message;
pub mod presence;
pub mod routing;
pub mod task;
pub mod time;
pub mod version;

pub use card::{AgentCard, CardTimestamp, OwnerRef, ServesCapability};
pub use clarification::{ClarificationRequest, ClarificationResponse, Question};
pub use content::{
    AnswerContent, ClarificationContent, NoteContent, RequestContent, ResultContent, SealedContent,
    LIFECYCLE_CAPABILITY,
};
pub use directory::{
    Availability, AvailabilityStatus, MatchedAgent, MemberRecord, PeopleMatch, ServedCapability,
    ASK_ME,
};
pub use duty::{DutyDecision, DutyState, HandoverReason};
pub use gateway::{GatewayCard, GATEWAY_AGENT_ID, GATEWAY_VERSION};
pub use group::{
    is_group_id, GroupApproval, GroupChange, GroupDetail, GroupHold, GroupInvite, GroupMember,
    GroupOp, GroupOutcome, GroupPolicy, GroupRole, GroupSummary, Visibility,
    GROUP_CAPABILITY_NOT_ALLOWED, GROUP_DAILY_LIMIT, HOLD_ALWAYS_ASK, HOLD_APPROVAL_ALWAYS,
    HOLD_OUTSIDE_HOURS, NEEDS_FIRST_TIME_APPROVAL, NO_SHARED_GROUP,
};
pub use group_rules::{CapabilityRules, GroupHours, GroupRequestRules};
pub use http::{
    split_token, EnrollRequest, EnrollResponse, RelayHealth, RelayInstanceInfo, DEFAULT_RELAY_PORT,
    JOIN_CODE_ENV, PROBE_PAYLOAD, RELAY_PATH, SECRET_ENV,
};
pub use keys::{
    Envelope, InstanceKeys, PublicKeys, SealPurpose, SealedFor, ENVELOPE_TOO_LARGE, KEY_MISMATCH,
    MAX_ENVELOPE_BYTES, SEAL_SUITE,
};
pub use mail::{Delivery, FailoverCause, TaskEvent};
pub use message::{
    MessageKind, PeerMessage, RelayMessage, ALL_CAPABILITIES, NOT_A_PARTY, TASK_ENDED,
    UNKNOWN_TASK, VERSION_MISMATCH,
};
pub use presence::{CapabilityPresence, InstancePresence, ProviderKind, ProviderPresence};
pub use routing::{
    NoRoute, RequestKind, RouteCandidate, RoutePlan, RouteReason, RouteStrategy, RouteTarget,
    RoutingHints, RoutingQuery, DEFAULT_MAX_CANDIDATES, MAX_CANDIDATES,
};
pub use task::{
    DelegationDirection, ReasonKind, ResultReport, SealedTask, Task, TaskAttempt, TaskResult,
    TaskState, TaskStatus,
};
pub use time::{now_ms, Timestamp};
pub use version::{
    oldest_supported, ProtocolRange, VersionGap, MIN_SUPPORTED_PROTOCOL, PROTOCOL_VERSION,
};

#[cfg(test)]
pub(crate) mod fixtures;
