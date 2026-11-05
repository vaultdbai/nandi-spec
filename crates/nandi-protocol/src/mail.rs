//! Store-and-forward: what the relay keeps for an instance until it says
//! it has it.
//!
//! - Everything the relay sends an instance about a task or a duty is one
//!   [`Delivery`], posted to that instance's mailbox and numbered (its
//!   `mailId`). The relay pushes it at once when the instance is online
//!   (`Deliver`) and keeps it until the instance acks it (`Ack`). An
//!   instance that connects sends `InboxSync`, and the relay sends every
//!   delivery it hasn't acked, oldest first. So a provider that was
//!   offline gets its work, and a requester that was offline gets its
//!   result.
//! - A client handles a mail ID once per connection: a delivery can come
//!   live and again from `InboxSync`. Envelopes inside are also dropped
//!   by `(taskId, from, seq)` as before.
//! - The relay ends what nobody can finish ([`TaskEvent::Ended`]) and
//!   fails a request over to the next candidate the requester sealed it
//!   for ([`TaskEvent::FailedOver`]), so the requester can be offline.
//! - Live-only traffic isn't stored (see
//!   [`crate::PeerMessage::is_live_only`]).
//! - Retention: acked deliveries go 30 days after their ack; unacked ones
//!   stay while their task is open, then 30 days after it ends.
//!
//! Offline (the relay unreachable): nothing arrives. Work in flight waits
//! at the relay; a client keeps its own rows as they were and catches up
//! with `InboxSync` when it reconnects.

use serde::{Deserialize, Serialize};

use crate::duty::DutyDecision;
use crate::keys::Envelope;
use crate::message::PeerMessage;
use crate::task::{ReasonKind, ResultReport, SealedTask, TaskState};

/// One thing in an instance's mailbox, tagged by `item`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "item",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum Delivery {
    /// Provider: run this task. Its input is sealed for you.
    Task { task: SealedTask },
    /// Requester: your task's result, sealed for you
    Result { result: ResultReport },
    /// A lifecycle message from the other side of a task
    Peer {
        from_instance: String,
        body: PeerMessage,
    },
    /// What the relay did with a task you're a party to
    Event { event: TaskEvent },
    /// Seal the request of `task_id` for `target` and send `Reassign`
    /// with it: a manager moved the task (to its provider), or its
    /// provider no longer holds the request (to its requester, the
    /// author). Nothing moves until it's resealed: there's no team key.
    Reseal {
        task_id: String,
        /// The new provider's card
        target: String,
        /// Who asked for the move (a display name)
        by: String,
    },
    /// Primary: a backup watches this duty; how is it going? Answer
    /// with `DutyReport`.
    DutyQuery {
        duty_id: String,
        backup_instance: String,
    },
    /// Backup: the relay's call on a duty you watch. A failed run's error
    /// is in `envelope`, sealed for you by the primary.
    Duty {
        duty_id: String,
        decision: DutyDecision,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        envelope: Option<Envelope>,
    },
}

impl Delivery {
    /// Every envelope it carries
    pub fn envelopes(&self) -> Vec<&Envelope> {
        match self {
            Self::Task { task } => vec![&task.envelope],
            Self::Result { result } => vec![&result.envelope],
            Self::Peer { body, .. } => body.envelopes(),
            Self::Event { event } => event.envelope().into_iter().collect(),
            Self::Duty { envelope, .. } => envelope.iter().collect(),
            Self::Reseal { .. } | Self::DutyQuery { .. } => Vec::new(),
        }
    }

    /// The task (or duty) it is about
    pub fn task_id(&self) -> &str {
        match self {
            Self::Task { task } => &task.id,
            Self::Result { result } => &result.task_id,
            Self::Peer { body, .. } => body.task_id(),
            Self::Event { event } => event.task_id(),
            Self::Reseal { task_id, .. } => task_id,
            Self::DutyQuery { duty_id, .. } | Self::Duty { duty_id, .. } => duty_id,
        }
    }
}

/// Why the relay gave a request to its next candidate
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum FailoverCause {
    /// The provider refused it
    Rejected,
    /// It took on the request and didn't finish it in time
    TimedOut,
    /// It didn't come online to take the request in time
    NotPickedUp,
}

/// What the relay did with a task, for a party to it; tagged by `event`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "event",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum TaskEvent {
    /// Requester: the relay took your `SubmitTask`. It's with the
    /// provider, or `held` for them until they're online.
    Accepted {
        task_id: String,
        provider_instance: String,
        /// The provider's card
        target: String,
        held: bool,
    },
    /// Requester: the relay wouldn't take it (nobody serves it there, a
    /// bad attempt, a loop); `message` is the relay's own words
    Refused { task_id: String, message: String },
    /// Requester: passed to the next candidate you sealed it for
    FailedOver {
        task_id: String,
        from_instance: String,
        provider_instance: String,
        target: String,
        cause: FailoverCause,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason_kind: Option<ReasonKind>,
    },
    /// Requester: its provider (or a manager, through the provider)
    /// handed it on. The note is sealed for you by `from_instance`.
    Reassigned {
        task_id: String,
        from_instance: String,
        provider_instance: String,
        target: String,
        /// The new provider's display name
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider_name: Option<String>,
        /// Who moved it (a display name)
        by: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        envelope: Option<Box<Envelope>>,
    },
    /// Either side: the relay ended it. Past its due date (`abandoned`
    /// when no provider took it up, else `timedOut`), or, for a provider
    /// it was taken from, given to the next candidate (`timedOut`).
    Ended {
        task_id: String,
        state: TaskState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason_kind: Option<ReasonKind>,
    },
}

impl TaskEvent {
    pub fn task_id(&self) -> &str {
        match self {
            Self::Accepted { task_id, .. }
            | Self::Refused { task_id, .. }
            | Self::FailedOver { task_id, .. }
            | Self::Reassigned { task_id, .. }
            | Self::Ended { task_id, .. } => task_id,
        }
    }

    pub fn envelope(&self) -> Option<&Envelope> {
        match self {
            Self::Reassigned { envelope, .. } => envelope.as_deref(),
            _ => None,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use serde_json::json;

    use super::*;
    use crate::fixtures::{envelope, report, sealed_task};
    use crate::keys::SealPurpose;

    pub(crate) fn every_delivery() -> Vec<Delivery> {
        vec![
            Delivery::Task {
                task: sealed_task(),
            },
            Delivery::Result { result: report() },
            Delivery::Peer {
                from_instance: "i1".into(),
                body: PeerMessage::CancelTask {
                    task_id: "t1".into(),
                    envelope: Some(envelope(SealPurpose::Note)),
                },
            },
            Delivery::Event {
                event: TaskEvent::Accepted {
                    task_id: "t1".into(),
                    provider_instance: "i2".into(),
                    target: "i2/writer".into(),
                    held: true,
                },
            },
            Delivery::Event {
                event: TaskEvent::Refused {
                    task_id: "t1".into(),
                    message: "No agent i2/writer serves write".into(),
                },
            },
            Delivery::Event {
                event: TaskEvent::FailedOver {
                    task_id: "t1".into(),
                    from_instance: "i2".into(),
                    provider_instance: "i3".into(),
                    target: "i3/writer".into(),
                    cause: FailoverCause::Rejected,
                    reason_kind: Some(ReasonKind::NotAccepting),
                },
            },
            Delivery::Event {
                event: TaskEvent::Reassigned {
                    task_id: "t1".into(),
                    from_instance: "i2".into(),
                    provider_instance: "i3".into(),
                    target: "i3/gateway".into(),
                    provider_name: Some("Carol".into()),
                    by: "Bob".into(),
                    envelope: Some(Box::new(envelope(SealPurpose::Note))),
                },
            },
            Delivery::Event {
                event: TaskEvent::Ended {
                    task_id: "t1".into(),
                    state: TaskState::Abandoned,
                    reason_kind: Some(ReasonKind::Offline),
                },
            },
            Delivery::Reseal {
                task_id: "t1".into(),
                target: "i3/gateway".into(),
                by: "Manager".into(),
            },
            Delivery::DutyQuery {
                duty_id: "standup/2026-11-05/09:00".into(),
                backup_instance: "i3".into(),
            },
            Delivery::Duty {
                duty_id: "standup/2026-11-05/09:00".into(),
                decision: DutyDecision::Handover {
                    reason: crate::duty::HandoverReason::Failed,
                },
                envelope: Some(envelope(SealPurpose::Note)),
            },
        ]
    }

    #[test]
    fn deliveries_round_trip_and_name_their_task() {
        for delivery in every_delivery() {
            let text = serde_json::to_string(&delivery).unwrap();
            assert_eq!(
                serde_json::from_str::<Delivery>(&text).unwrap(),
                delivery,
                "{text}"
            );
            assert!(!delivery.task_id().is_empty());
        }
        let counts: Vec<usize> = every_delivery()
            .iter()
            .map(|d| d.envelopes().len())
            .collect();
        assert_eq!(counts, vec![1, 1, 1, 0, 0, 0, 1, 0, 0, 0, 1]);
    }

    #[test]
    fn events_are_tagged_snake_case_with_camel_case_fields() {
        let value = serde_json::to_value(Delivery::Event {
            event: TaskEvent::Ended {
                task_id: "t1".into(),
                state: TaskState::TimedOut,
                reason_kind: None,
            },
        })
        .unwrap();
        assert_eq!(
            value,
            json!({"item": "event", "event": {"event": "ended", "taskId": "t1", "state": "timedOut"}})
        );
        assert_eq!(
            serde_json::to_value(FailoverCause::NotPickedUp).unwrap(),
            "not_picked_up"
        );
    }
}
