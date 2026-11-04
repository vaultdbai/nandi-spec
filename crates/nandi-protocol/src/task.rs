//! Delegated work: a [`Task`] one agent sends another, its [`TaskResult`],
//! and the states and reasons both sides report while it runs.
//!
//! [`Task`] and [`TaskResult`] are the opened forms, with their content.
//! They never cross the relay: on the wire a task is a [`SealedTask`] and
//! a result a [`ResultReport`], their content sealed in an envelope (see
//! [`crate::content`]).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::card::AgentCard;
use crate::keys::Envelope;
use crate::time::Timestamp;

/// A unit of work one agent asks another to do. Opened: the wire carries
/// a [`SealedTask`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    /// The requesting agent's card
    pub requester: AgentCard,
    /// Target card ID (`<instance_id>/<agent_id>`)
    pub target: String,
    pub capability: String,
    pub input: Value,
    pub created_at: Timestamp,
    pub deadline: Option<Timestamp>,
}

/// A [`Task`] on the wire: its metadata in the clear, its input sealed for
/// the provider ([`crate::content::RequestContent`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SealedTask {
    pub id: String,
    /// The requesting agent's card
    pub requester: AgentCard,
    /// Target card ID (`<instance_id>/<agent_id>`)
    pub target: String,
    pub capability: String,
    pub created_at: Timestamp,
    pub deadline: Option<Timestamp>,
    /// The input, sealed for the provider by the requester, or resealed
    /// by the provider it was reassigned from
    pub envelope: Envelope,
    /// The earlier attempt this one carries on, when `envelope` is a
    /// reseal of that attempt's request (its `sealedFor.taskId`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continues: Option<String>,
}

impl SealedTask {
    /// The task ID `envelope` must be sealed under
    pub fn sealed_task_id(&self) -> &str {
        self.continues.as_deref().unwrap_or(&self.id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum TaskStatus {
    /// Sent, no result yet
    Running,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

impl TaskStatus {
    /// As stored in `delegations.status` (the serde name)
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timedOut",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "running" => Self::Running,
            "completed" => Self::Completed,
            "cancelled" => Self::Cancelled,
            "timedOut" => Self::TimedOut,
            _ => Self::Failed,
        }
    }
}

/// Whether we asked (`Outbound`) or served (`Inbound`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum DelegationDirection {
    Outbound,
    Inbound,
}

impl DelegationDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Outbound => "outbound",
            Self::Inbound => "inbound",
        }
    }

    pub fn parse(value: &str) -> Self {
        if value == "inbound" {
            Self::Inbound
        } else {
            Self::Outbound
        }
    }
}

/// A delegated task's outcome; also a `delegations` history row. Opened:
/// the wire carries a [`ResultReport`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TaskResult {
    pub task_id: String,
    /// Provider card ID
    pub provider_agent: String,
    pub output: Value,
    pub status: TaskStatus,
    pub duration_ms: u32,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub error: Option<String>,
    // History fields
    pub direction: DelegationDirection,
    pub capability: String,
    /// Requester card ID
    pub requester_agent: String,
    /// GitHub handles of the two sides, when known
    pub provider_owner: Option<String>,
    pub requester_owner: Option<String>,
    pub input: Value,
    pub started_at: Timestamp,
    pub ended_at: Option<Timestamp>,
}

/// A [`TaskResult`] on the wire (provider → requester): the metadata in
/// the clear, the output and error sealed for the requester
/// ([`crate::content::ResultContent`]). The requester has the input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultReport {
    pub task_id: String,
    /// Provider card ID
    pub provider_agent: String,
    /// Requester card ID
    pub requester_agent: String,
    pub capability: String,
    pub status: TaskStatus,
    pub duration_ms: u32,
    pub tokens_in: u32,
    pub tokens_out: u32,
    /// GitHub handles of the two sides, when known
    pub provider_owner: Option<String>,
    pub requester_owner: Option<String>,
    pub started_at: Timestamp,
    pub ended_at: Option<Timestamp>,
    pub envelope: Envelope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum TaskState {
    Submitted,
    Delivered,
    Acknowledged,
    Working,
    InputRequired,
    Completed,
    Failed,
    Rejected,
    Cancelled,
    TimedOut,
    Abandoned,
    /// Handed to another provider (wave EF); the next attempt carries on
    Reassigned,
}

impl TaskState {
    pub const ALL: [TaskState; 12] = [
        Self::Submitted,
        Self::Delivered,
        Self::Acknowledged,
        Self::Working,
        Self::InputRequired,
        Self::Completed,
        Self::Failed,
        Self::Rejected,
        Self::Cancelled,
        Self::TimedOut,
        Self::Abandoned,
        Self::Reassigned,
    ];

    /// As stored in `tasks.state` (the serde name)
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Submitted => "submitted",
            Self::Delivered => "delivered",
            Self::Acknowledged => "acknowledged",
            Self::Working => "working",
            Self::InputRequired => "inputRequired",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timedOut",
            Self::Abandoned => "abandoned",
            Self::Reassigned => "reassigned",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == value)
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed
                | Self::Failed
                | Self::Rejected
                | Self::Cancelled
                | Self::TimedOut
                | Self::Abandoned
                | Self::Reassigned
        )
    }
}

/// Why a task waits or was refused: the Blocker kinds of work-model §2.3,
/// plus `declined` (the owner said no to a held request). Set where the
/// decision is made, so nothing downstream parses reason text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ReasonKind {
    NeedsMyApproval,
    NeedsTheirApproval,
    NeedsMyAnswer,
    NeedsTheirAnswer,
    NotTrusted,
    NotAccepting,
    PausedOrBlocked,
    Queued,
    Offline,
    NoModel,
    RuntimeAuth,
    TimedOut,
    Failed,
    Declined,
}

impl ReasonKind {
    pub const ALL: [ReasonKind; 14] = [
        Self::NeedsMyApproval,
        Self::NeedsTheirApproval,
        Self::NeedsMyAnswer,
        Self::NeedsTheirAnswer,
        Self::NotTrusted,
        Self::NotAccepting,
        Self::PausedOrBlocked,
        Self::Queued,
        Self::Offline,
        Self::NoModel,
        Self::RuntimeAuth,
        Self::TimedOut,
        Self::Failed,
        Self::Declined,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::NeedsMyApproval => "needsMyApproval",
            Self::NeedsTheirApproval => "needsTheirApproval",
            Self::NeedsMyAnswer => "needsMyAnswer",
            Self::NeedsTheirAnswer => "needsTheirAnswer",
            Self::NotTrusted => "notTrusted",
            Self::NotAccepting => "notAccepting",
            Self::PausedOrBlocked => "pausedOrBlocked",
            Self::Queued => "queued",
            Self::Offline => "offline",
            Self::NoModel => "noModel",
            Self::RuntimeAuth => "runtimeAuth",
            Self::TimedOut => "timedOut",
            Self::Failed => "failed",
            Self::Declined => "declined",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }

    /// The same reason seen from the other side of the task: a hold for my
    /// approval is, for the requester, waiting on theirs
    pub fn for_requester(self) -> Self {
        match self {
            Self::NeedsMyApproval => Self::NeedsTheirApproval,
            Self::NeedsTheirAnswer => Self::NeedsMyAnswer,
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_round_trip_through_their_names() {
        for state in TaskState::ALL {
            assert_eq!(TaskState::parse(state.as_str()), Some(state));
            let json = serde_json::to_value(state).unwrap();
            assert_eq!(json, state.as_str());
        }
        assert_eq!(TaskState::parse("bogus"), None);
    }

    #[test]
    fn reasons_round_trip_through_their_names() {
        for kind in ReasonKind::ALL {
            assert_eq!(ReasonKind::parse(kind.as_str()), Some(kind));
            assert_eq!(serde_json::to_value(kind).unwrap(), kind.as_str());
        }
        assert_eq!(ReasonKind::parse("bogus"), None);
    }

    #[test]
    fn requesters_see_the_other_side_of_a_wait() {
        assert_eq!(
            ReasonKind::NeedsMyApproval.for_requester(),
            ReasonKind::NeedsTheirApproval
        );
        assert_eq!(
            ReasonKind::NeedsTheirAnswer.for_requester(),
            ReasonKind::NeedsMyAnswer
        );
        assert_eq!(ReasonKind::Queued.for_requester(), ReasonKind::Queued);
    }

    #[test]
    fn statuses_and_directions_round_trip_through_their_names() {
        for status in [
            TaskStatus::Running,
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Cancelled,
            TaskStatus::TimedOut,
        ] {
            assert_eq!(TaskStatus::parse(status.as_str()), status);
            assert_eq!(serde_json::to_value(status).unwrap(), status.as_str());
        }
        assert_eq!(TaskStatus::parse("bogus"), TaskStatus::Failed);
        for direction in [DelegationDirection::Outbound, DelegationDirection::Inbound] {
            assert_eq!(DelegationDirection::parse(direction.as_str()), direction);
            assert_eq!(serde_json::to_value(direction).unwrap(), direction.as_str());
        }
    }
}
