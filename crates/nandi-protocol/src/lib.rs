//! The Nandi relay wire protocol: everything a relay server and its
//! clients exchange, and nothing else.
//!
//! - [`message`]: the WebSocket frames (`RelayMessage`, `PeerMessage`)
//! - [`http`]: the relay's HTTP endpoints (`/health`, `/instances`,
//!   `/relay/enroll`) and connection constants
//! - [`card`], [`gateway`], [`presence`]: what instances publish about
//!   themselves
//! - [`directory`]: the member directory, availability and people search
//! - [`routing`]: who gets a request (`RouteQuery` → `RoutePlan`)
//! - [`keys`]: per-instance public keys and sealed envelopes
//! - [`content`]: what each sealed envelope holds
//! - [`task`], [`clarification`]: delegated work and its questions
//! - [`mail`]: store-and-forward, each instance's mailbox and task events
//! - [`duty`]: calendar handovers, watched at the relay
//! - [`time`]: timestamps on the wire
//!
//! Serde shapes are the wire format: a field is never renamed or retyped
//! without a [`PROTOCOL_VERSION`] bump, and new fields come with
//! `#[serde(default)]`. With the `specta` feature every type also derives
//! `specta::Type`, for apps that export them to TypeScript.

pub mod card;
pub mod clarification;
pub mod content;
pub mod directory;
pub mod duty;
pub mod gateway;
pub mod http;
pub mod keys;
pub mod mail;
pub mod message;
pub mod presence;
pub mod routing;
pub mod task;
pub mod time;

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
pub use http::{
    split_token, EnrollRequest, EnrollResponse, RelayHealth, RelayInstanceInfo, DEFAULT_RELAY_PORT,
    JOIN_CODE_ENV, PROBE_PAYLOAD, RELAY_PATH, SECRET_ENV,
};
pub use keys::{
    Envelope, InstanceKeys, PublicKeys, SealPurpose, SealedFor, ENVELOPE_TOO_LARGE, KEY_MISMATCH,
    MAX_ENVELOPE_BYTES, SEAL_SUITE,
};
pub use mail::{Delivery, FailoverCause, TaskEvent};
pub use message::{PeerMessage, RelayMessage, ALL_CAPABILITIES, VERSION_MISMATCH};
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

/// The wire version. A relay refuses a client that speaks another one.
///
/// - 1: versions on registration and `/health`
/// - 2: the member directory (`QueryDirectory`) and people search
///   (`FindPeople`)
/// - 3: per-instance public keys at enrollment and registration, pinned
///   by the relay (`key_mismatch`) and carried in the directory
/// - 4: every person-to-person payload sealed (`envelope` replaces
///   `input`, `output`, `error`, notes, reasons, questions and answers),
///   `PublicKeysQuery`, the envelope cap (`envelope_too_large`)
/// - 5: routing at the relay (`RouteQuery` → `RoutePlan`, see [`routing`])
/// - 6: the team brain at the relay. Store-and-forward (`SubmitTask` with
///   an envelope per candidate, `Deliver`/`Ack`/`InboxSync`, see [`mail`]),
///   failover, timeouts and abandons at the relay ([`TaskEvent`]),
///   reassignment (`Reassign`, `ReassignResult`, [`Delivery::Reseal`]) and
///   calendar handovers (`DutyWatch`, `DutyReport`, see [`duty`]). Gone:
///   `DelegateTask`, `TaskAssigned`, `TaskCompleted`, `TaskFailed`,
///   `Undeliverable`, the relay's `Peer` to a client, `ReassignTask`,
///   `TaskTimedOut`, `DutyQuery`/`DutyStatus`, `StatusResponse`'s result
///   and `SealedTask.continues`.
pub const PROTOCOL_VERSION: u32 = 6;

#[cfg(test)]
pub(crate) mod fixtures;
