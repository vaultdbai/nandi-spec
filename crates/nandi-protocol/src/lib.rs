//! The Nandi relay wire protocol: everything a relay server and its
//! clients exchange, and nothing else.
//!
//! - [`message`]: the WebSocket frames (`RelayMessage`, `PeerMessage`)
//! - [`http`]: the relay's HTTP endpoints (`/health`, `/instances`,
//!   `/relay/enroll`) and connection constants
//! - [`card`], [`gateway`], [`presence`]: what instances publish about
//!   themselves
//! - [`directory`]: the member directory, availability and people search
//! - [`task`], [`clarification`]: delegated work and its questions
//! - [`time`]: timestamps on the wire
//!
//! Serde shapes are the wire format: a field is never renamed or retyped
//! without a [`PROTOCOL_VERSION`] bump, and new fields come with
//! `#[serde(default)]`. With the `specta` feature every type also derives
//! `specta::Type`, for apps that export them to TypeScript.

pub mod card;
pub mod clarification;
pub mod directory;
pub mod gateway;
pub mod http;
pub mod message;
pub mod presence;
pub mod task;
pub mod time;

pub use card::{AgentCard, CardTimestamp, OwnerRef, ServesCapability};
pub use clarification::{ClarificationRequest, ClarificationResponse, Question};
pub use directory::{
    Availability, AvailabilityStatus, MatchedAgent, MemberRecord, PeopleMatch, ServedCapability,
    ASK_ME,
};
pub use gateway::{GatewayCard, GATEWAY_AGENT_ID, GATEWAY_VERSION};
pub use http::{
    split_token, EnrollRequest, EnrollResponse, RelayHealth, RelayInstanceInfo, DEFAULT_RELAY_PORT,
    JOIN_CODE_ENV, PROBE_PAYLOAD, RELAY_PATH, SECRET_ENV,
};
pub use message::{
    DutyState, PeerDelivery, PeerMessage, RelayMessage, TaskAssigned, TaskCompleted, TaskFailed,
    ALL_CAPABILITIES, VERSION_MISMATCH,
};
pub use presence::{CapabilityPresence, InstancePresence, ProviderKind, ProviderPresence};
pub use task::{DelegationDirection, ReasonKind, Task, TaskResult, TaskState, TaskStatus};
pub use time::{now_ms, Timestamp};

/// The wire version. A relay refuses a client that speaks another one.
///
/// - 1: versions on registration and `/health`
/// - 2: the member directory (`QueryDirectory`) and people search
///   (`FindPeople`)
pub const PROTOCOL_VERSION: u32 = 2;

#[cfg(test)]
pub(crate) mod fixtures;
