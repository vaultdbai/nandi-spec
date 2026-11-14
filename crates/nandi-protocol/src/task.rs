//! Delegated work: a [`Task`] one agent sends another, its [`TaskResult`],
//! and the states and reasons both sides report while it runs.
//!
//! [`Task`] and [`TaskResult`] are the opened forms, with their content.
//! They never cross the relay: on the wire a task is a [`SealedTask`] and
//! a result a [`ResultReport`], their content sealed in an envelope (see
//! [`crate::content`]).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum::{EnumString, IntoStaticStr, ParseError, VariantArray};

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
    /// by the provider it was reassigned from (the requester's signature
    /// kept). Bound to this task's ID either way.
    pub envelope: Envelope,
}

/// One candidate of a `SubmitTask`: who, and the input sealed for them
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskAttempt {
    /// The provider's card (`<instance_id>/<agent_id>`)
    pub target: String,
    pub envelope: Envelope,
}

impl TaskAttempt {
    /// The provider's instance ID
    pub fn instance(&self) -> &str {
        self.target.split('/').next().unwrap_or_default()
    }
}

/// A delegated task's outcome. Its text (strum) is its serde name: both are
/// camelCase.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    EnumString,
    IntoStaticStr,
    VariantArray,
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
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
        self.into()
    }

    /// Reads a stored [`Self::as_str`]. Text no variant has is an error,
    /// never a default.
    pub fn parse(value: &str) -> Result<Self, ParseError> {
        value.parse()
    }
}

/// Whether we asked (`Outbound`) or served (`Inbound`). Its text (strum) is
/// its serde name: both are camelCase.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    EnumString,
    IntoStaticStr,
    VariantArray,
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
pub enum DelegationDirection {
    Outbound,
    Inbound,
}

impl DelegationDirection {
    /// As stored in `delegations.direction` (the serde name)
    pub fn as_str(self) -> &'static str {
        self.into()
    }

    /// Reads a stored [`Self::as_str`]. Text no variant has is an error,
    /// never a default.
    pub fn parse(value: &str) -> Result<Self, ParseError> {
        value.parse()
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

/// Where a task is in its lifecycle. Its text (strum) is its serde name:
/// both are camelCase.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumString,
    IntoStaticStr,
    VariantArray,
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
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
    /// As stored in `tasks.state` (the serde name)
    pub fn as_str(self) -> &'static str {
        self.into()
    }

    /// Reads a stored [`Self::as_str`]. Text no variant has is an error,
    /// never a default.
    pub fn parse(value: &str) -> Result<Self, ParseError> {
        value.parse()
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
/// decision is made, so nothing downstream parses reason text. Its text
/// (strum) is its serde name: both are camelCase.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    EnumString,
    IntoStaticStr,
    VariantArray,
)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
#[strum(serialize_all = "camelCase")]
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
    /// As stored in `reason_kind` columns (the serde name)
    pub fn as_str(self) -> &'static str {
        self.into()
    }

    /// Reads a stored [`Self::as_str`]. Text no variant has is an error,
    /// never a default.
    pub fn parse(value: &str) -> Result<Self, ParseError> {
        value.parse()
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

    /// The wire and stored text of every state, written out by hand so a
    /// change to the derives can't change it unnoticed
    const STATE_NAMES: [(TaskState, &str); 12] = [
        (TaskState::Submitted, "submitted"),
        (TaskState::Delivered, "delivered"),
        (TaskState::Acknowledged, "acknowledged"),
        (TaskState::Working, "working"),
        (TaskState::InputRequired, "inputRequired"),
        (TaskState::Completed, "completed"),
        (TaskState::Failed, "failed"),
        (TaskState::Rejected, "rejected"),
        (TaskState::Cancelled, "cancelled"),
        (TaskState::TimedOut, "timedOut"),
        (TaskState::Abandoned, "abandoned"),
        (TaskState::Reassigned, "reassigned"),
    ];

    const REASON_NAMES: [(ReasonKind, &str); 14] = [
        (ReasonKind::NeedsMyApproval, "needsMyApproval"),
        (ReasonKind::NeedsTheirApproval, "needsTheirApproval"),
        (ReasonKind::NeedsMyAnswer, "needsMyAnswer"),
        (ReasonKind::NeedsTheirAnswer, "needsTheirAnswer"),
        (ReasonKind::NotTrusted, "notTrusted"),
        (ReasonKind::NotAccepting, "notAccepting"),
        (ReasonKind::PausedOrBlocked, "pausedOrBlocked"),
        (ReasonKind::Queued, "queued"),
        (ReasonKind::Offline, "offline"),
        (ReasonKind::NoModel, "noModel"),
        (ReasonKind::RuntimeAuth, "runtimeAuth"),
        (ReasonKind::TimedOut, "timedOut"),
        (ReasonKind::Failed, "failed"),
        (ReasonKind::Declined, "declined"),
    ];

    #[test]
    fn states_keep_their_wire_and_stored_names() {
        assert_eq!(STATE_NAMES.len(), TaskState::VARIANTS.len());
        for (state, name) in STATE_NAMES {
            assert_eq!(
                serde_json::to_string(&state).unwrap(),
                format!("\"{name}\"")
            );
            assert_eq!(
                serde_json::from_str::<TaskState>(&format!("\"{name}\"")).unwrap(),
                state
            );
            assert_eq!(state.as_str(), name);
            assert_eq!(TaskState::parse(name), Ok(state));
        }
    }

    #[test]
    fn reasons_keep_their_wire_and_stored_names() {
        assert_eq!(REASON_NAMES.len(), ReasonKind::VARIANTS.len());
        for (kind, name) in REASON_NAMES {
            assert_eq!(serde_json::to_string(&kind).unwrap(), format!("\"{name}\""));
            assert_eq!(
                serde_json::from_str::<ReasonKind>(&format!("\"{name}\"")).unwrap(),
                kind
            );
            assert_eq!(kind.as_str(), name);
            assert_eq!(ReasonKind::parse(name), Ok(kind));
        }
    }

    #[test]
    fn unknown_state_and_reason_text_is_an_error() {
        for text in ["bogus", "", "Submitted", "input_required", "TimedOut"] {
            assert!(TaskState::parse(text).is_err(), "state {text:?}");
        }
        for text in ["bogus", "", "Queued", "needs_my_answer", "NOMODEL"] {
            assert!(ReasonKind::parse(text).is_err(), "reason {text:?}");
        }
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

    /// The wire and stored text of every status, written out by hand so a
    /// change to the derives can't change it unnoticed
    const STATUS_NAMES: [(TaskStatus, &str); 5] = [
        (TaskStatus::Running, "running"),
        (TaskStatus::Completed, "completed"),
        (TaskStatus::Failed, "failed"),
        (TaskStatus::Cancelled, "cancelled"),
        (TaskStatus::TimedOut, "timedOut"),
    ];

    const DIRECTION_NAMES: [(DelegationDirection, &str); 2] = [
        (DelegationDirection::Outbound, "outbound"),
        (DelegationDirection::Inbound, "inbound"),
    ];

    #[test]
    fn statuses_keep_their_wire_and_stored_names() {
        assert_eq!(STATUS_NAMES.len(), TaskStatus::VARIANTS.len());
        for (status, name) in STATUS_NAMES {
            assert_eq!(
                serde_json::to_string(&status).unwrap(),
                format!("\"{name}\"")
            );
            assert_eq!(
                serde_json::from_str::<TaskStatus>(&format!("\"{name}\"")).unwrap(),
                status
            );
            assert_eq!(status.as_str(), name);
            assert_eq!(TaskStatus::parse(name), Ok(status));
        }
    }

    #[test]
    fn directions_keep_their_wire_and_stored_names() {
        assert_eq!(DIRECTION_NAMES.len(), DelegationDirection::VARIANTS.len());
        for (direction, name) in DIRECTION_NAMES {
            assert_eq!(
                serde_json::to_string(&direction).unwrap(),
                format!("\"{name}\"")
            );
            assert_eq!(
                serde_json::from_str::<DelegationDirection>(&format!("\"{name}\"")).unwrap(),
                direction
            );
            assert_eq!(direction.as_str(), name);
            assert_eq!(DelegationDirection::parse(name), Ok(direction));
        }
    }

    #[test]
    fn unknown_status_and_direction_text_is_an_error() {
        for text in ["bogus", "", "Running", "timed_out", "TimedOut"] {
            assert!(TaskStatus::parse(text).is_err(), "status {text:?}");
        }
        for text in ["bogus", "", "Inbound", "OUTBOUND"] {
            assert!(
                DelegationDirection::parse(text).is_err(),
                "direction {text:?}"
            );
        }
    }
}
