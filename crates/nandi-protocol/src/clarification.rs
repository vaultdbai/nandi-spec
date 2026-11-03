//! Questions a provider asks the requester about a task, and the answers.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub id: String,
    pub question: String,
    pub topic: String,
    /// The provider says only a person can answer this
    pub requires_human: bool,
}

/// Provider → requester (`PeerMessage::ClarificationRequest`)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ClarificationRequest {
    pub task_id: String,
    /// 1-based
    pub round: u32,
    pub questions: Vec<Question>,
}

/// Requester → provider; `answers` is question ID → answer
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct ClarificationResponse {
    pub task_id: String,
    pub round: u32,
    pub answers: HashMap<String, String>,
}
