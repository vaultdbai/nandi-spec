//! Calendar handovers: a calendar trigger's backup takes over when its
//! primary doesn't do the work. The relay watches and decides.
//!
//! 1. The backup's Nandi registers a watch: `DutyWatch` (the duty, the
//!    primary's instance, when it's due and how long a run may take).
//!    Watches are kept on the relay's disk, so they survive restarts.
//! 2. A grace period after the duty is due, the relay asks the primary's
//!    Nandi ([`crate::Delivery::DutyQuery`]), which answers with
//!    `DutyReport`: its [`DutyState`] (a failed run's error sealed for the
//!    backup).
//! 3. The relay decides: settled, ask again later, or hand over
//!    ([`crate::Delivery::Duty`] with a [`DutyDecision`]). A primary that
//!    is offline or doesn't answer is handed over.
//!
//! `duty_id` is `<trigger>/<date>/<time>`.
//!
//! Offline (the relay unreachable): the backup's Nandi hands over by
//! itself once the grace period has passed, as it would for a primary
//! that doesn't answer.

use serde::{Deserialize, Serialize};

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
    /// Its error, if any, is in the `DutyReport` envelope
    Failed,
    /// Due, but no run was started (trigger off, rate limited, or missing)
    NotRun,
}

/// Why the backup takes over
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "code",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum HandoverReason {
    /// The primary's run failed (its error, if any, is sealed beside)
    Failed,
    /// The primary's run never started
    NotRun,
    /// Still running after the watch's limit
    Overran { minutes: u32 },
    /// The primary's Nandi didn't answer (offline, or not running)
    NoAnswer,
}

impl HandoverReason {
    /// The English, for logs and the trigger's payload
    pub fn sentence(self) -> String {
        match self {
            Self::Failed => "the primary's run failed".to_string(),
            Self::NotRun => "the primary's run never started".to_string(),
            Self::Overran { minutes } => format!("not finished after {minutes} minutes"),
            Self::NoAnswer => "the primary's Nandi didn't answer".to_string(),
        }
    }
}

/// The relay's call on a watched duty
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "decision",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum DutyDecision {
    /// The primary did it: nothing for the backup to do
    Settled,
    /// The backup takes over
    Handover { reason: HandoverReason },
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn duty_shapes_are_tagged() {
        assert_eq!(
            serde_json::to_value(DutyState::Upcoming { due_at: 5 }).unwrap(),
            json!({"state": "upcoming", "dueAt": 5})
        );
        assert_eq!(
            serde_json::to_value(DutyDecision::Handover {
                reason: HandoverReason::Overran { minutes: 30 }
            })
            .unwrap(),
            json!({"decision": "handover", "reason": {"code": "overran", "minutes": 30}})
        );
        assert_eq!(
            serde_json::to_value(DutyDecision::Settled).unwrap(),
            json!({"decision": "settled"})
        );
        for reason in [
            HandoverReason::Failed,
            HandoverReason::NotRun,
            HandoverReason::Overran { minutes: 2 },
            HandoverReason::NoAnswer,
        ] {
            assert!(!reason.sentence().is_empty());
            let text = serde_json::to_string(&reason).unwrap();
            assert_eq!(
                serde_json::from_str::<HandoverReason>(&text).unwrap(),
                reason
            );
        }
    }
}
