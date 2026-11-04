//! What sealed envelopes hold: the content of every person-to-person
//! message, as JSON, sealed for the recipient (see [`crate::keys`] and the
//! `nandi-seal` crate). The relay sees none of it.
//!
//! | Wire field | Purpose | Content | Capability bound |
//! | --- | --- | --- | --- |
//! | `DelegateTask.envelope`, `ReassignTask.request` | `request` | [`RequestContent`] | The task's |
//! | `ResultReport.envelope` | `result` | [`ResultContent`] | The task's |
//! | `StatusResponse.envelope` | `result` | [`ResultContent`] | [`LIFECYCLE_CAPABILITY`] |
//! | `ClarificationRequest.envelope` | `clarification` | [`ClarificationContent`] | [`LIFECYCLE_CAPABILITY`] |
//! | `ClarificationResponse.envelope` | `answer` | [`AnswerContent`] | [`LIFECYCLE_CAPABILITY`] |
//! | Every other `envelope` (a note, a reason, an error) | `note` | [`NoteContent`] | [`LIFECYCLE_CAPABILITY`] |
//!
//! A request and its result bind the task's capability, which both sides
//! read in the clear. Lifecycle messages don't carry the capability, so
//! their envelopes bind [`LIFECYCLE_CAPABILITY`]; the task ID binds them to
//! their task. A receiver checks the purpose, the task, the recipient and
//! the sender of every envelope before it opens one.

use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::clarification::Question;
use crate::keys::SealPurpose;

/// The capability lifecycle envelopes bind (notes, questions, answers,
/// status results)
pub const LIFECYCLE_CAPABILITY: &str = "";

/// Content sealed as JSON for one [`SealPurpose`]
pub trait SealedContent: Serialize + DeserializeOwned {
    const PURPOSE: SealPurpose;

    /// The bytes that get sealed
    fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("sealed content is plain JSON")
    }

    /// Reads opened bytes
    fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| format!("Unreadable sealed content: {e}"))
    }
}

/// A task's input (requester → provider)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestContent {
    pub input: Value,
}

impl SealedContent for RequestContent {
    const PURPOSE: SealPurpose = SealPurpose::Request;
}

/// A task's output and error (provider → requester)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultContent {
    #[serde(default)]
    pub output: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl SealedContent for ResultContent {
    const PURPOSE: SealPurpose = SealPurpose::Result;
}

/// A clarification round's questions (provider → requester)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClarificationContent {
    pub questions: Vec<Question>,
}

impl SealedContent for ClarificationContent {
    const PURPOSE: SealPurpose = SealPurpose::Clarification;
}

/// A clarification round's answers, question ID → answer (requester →
/// provider)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerContent {
    pub answers: HashMap<String, String>,
}

impl SealedContent for AnswerContent {
    const PURPOSE: SealPurpose = SealPurpose::Answer;
}

/// Free text on a lifecycle message: a note, a reason, an error
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteContent {
    pub text: String,
}

impl SealedContent for NoteContent {
    const PURPOSE: SealPurpose = SealPurpose::Note;
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn content_round_trips_as_camel_case_json() {
        let result = ResultContent {
            output: json!({"text": "hi"}),
            error: None,
        };
        assert_eq!(
            serde_json::from_slice::<Value>(&result.to_bytes()).unwrap(),
            json!({"output": {"text": "hi"}})
        );
        assert_eq!(
            ResultContent::from_bytes(&result.to_bytes()).unwrap(),
            result
        );
        let questions = ClarificationContent {
            questions: vec![Question {
                id: "q1".into(),
                question: "Which year?".into(),
                topic: "scope".into(),
                requires_human: true,
            }],
        };
        assert_eq!(
            ClarificationContent::from_bytes(&questions.to_bytes()).unwrap(),
            questions
        );
        let note = NoteContent {
            text: "soon".into(),
        };
        assert_eq!(NoteContent::from_bytes(&note.to_bytes()).unwrap(), note);
        assert!(NoteContent::from_bytes(b"not json").is_err());
    }

    #[test]
    fn each_content_has_its_purpose() {
        assert_eq!(RequestContent::PURPOSE, SealPurpose::Request);
        assert_eq!(ResultContent::PURPOSE, SealPurpose::Result);
        assert_eq!(ClarificationContent::PURPOSE, SealPurpose::Clarification);
        assert_eq!(AnswerContent::PURPOSE, SealPurpose::Answer);
        assert_eq!(NoteContent::PURPOSE, SealPurpose::Note);
    }
}
