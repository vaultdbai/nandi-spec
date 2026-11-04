//! Per-instance keys and sealed envelopes: the shapes on the wire. The
//! crypto that makes and opens them is the `nandi-seal` crate; a relay
//! carries these without linking it.
//!
//! - Every instance (device) has its own [`PublicKeys`]: an Ed25519 key
//!   that signs content and an X25519 key that envelopes are sealed to.
//!   They're sent at enrollment and registration, and the relay pins them:
//!   another key for the same instance ID is refused with the `Error` code
//!   [`KEY_MISMATCH`] until the operator rekeys it.
//! - An [`Envelope`] is content sealed for one recipient. Its
//!   [`SealedFor`] is in the clear (the relay routes by it) and bound into
//!   the seal, so moving a blob to another task, person or purpose breaks
//!   it. What each envelope holds is in [`crate::content`].
//! - An envelope is at most [`MAX_ENVELOPE_BYTES`]. Files travel as links.
//!   A relay refuses a bigger one with the `Error` code
//!   [`ENVELOPE_TOO_LARGE`], and a receiver drops it.
//! - `PublicKeysQuery` asks the relay for the keys it pinned for some
//!   instances (`PublicKeysResult`). A client pins them itself on first
//!   sight, and refuses other keys later.
//!
//! Offline: keys and envelopes are data; nothing here talks to the relay.

use serde::{Deserialize, Serialize};

/// The seal construction an [`Envelope`] uses (see `nandi-seal`)
pub const SEAL_SUITE: &str = "nandi-seal-v1";

/// The `Error` code (and HTTP refusal) for an instance that shows up with
/// other keys than the ones the relay pinned for it
pub const KEY_MISMATCH: &str = "key_mismatch";

/// The `Error` code of a frame carrying an envelope over
/// [`MAX_ENVELOPE_BYTES`]
pub const ENVELOPE_TOO_LARGE: &str = "envelope_too_large";

/// The most an envelope's encoded `enc` and `ciphertext` may hold together
/// (1 MiB of base64url, about 768 KiB of content). Files go as links.
pub const MAX_ENVELOPE_BYTES: usize = 1 << 20;

/// Length of a 32-byte key in unpadded base64url
const KEY_CHARS: usize = 43;

/// An instance's public keys, each 32 bytes in unpadded base64url.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PublicKeys {
    /// Ed25519: verifies what this instance signs
    pub signing: String,
    /// X25519: what envelopes for this instance are sealed to
    pub sealing: String,
}

impl PublicKeys {
    /// Whether both keys look like 32 bytes of unpadded base64url. The
    /// relay checks this before pinning; `nandi-seal` checks the points.
    pub fn is_well_formed(&self) -> bool {
        [&self.signing, &self.sealing].into_iter().all(|key| {
            key.len() == KEY_CHARS
                && key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
    }
}

/// What a sealed envelope carries, as the sender meant it
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum SealPurpose {
    /// The task's input, requester → provider
    Request,
    /// The task's output or error, provider → requester
    Result,
    /// Questions, provider → requester
    Clarification,
    /// Answers, requester → provider
    Answer,
    /// Free text on a lifecycle message (a note, a reason)
    Note,
}

impl SealPurpose {
    /// The serde name; also what the seal binds
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Request => "request",
            Self::Result => "result",
            Self::Clarification => "clarification",
            Self::Answer => "answer",
            Self::Note => "note",
        }
    }
}

/// Who and what an envelope is for. In the clear, and bound into the seal
/// as its associated data.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SealedFor {
    pub task_id: String,
    /// Unique among the sender's envelopes for this task, and increasing.
    /// The receiver drops a `(task_id, from, seq)` it has already opened.
    pub seq: u64,
    /// The sender's instance ID
    pub from: String,
    /// The recipient's instance ID
    pub to: String,
    pub purpose: SealPurpose,
    pub capability: String,
}

/// Content sealed for one recipient ([`SEAL_SUITE`]). Bytes are unpadded
/// base64url.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    /// [`SEAL_SUITE`]
    pub suite: String,
    pub sealed_for: SealedFor,
    /// The sender's key ID (`nandi-seal`'s `key_id` of its public keys)
    pub sender_key_id: String,
    /// The recipient's key ID, so a recipient with new keys knows at once
    /// that an envelope wasn't sealed for them
    pub recipient_key_id: String,
    /// The sender's encapsulated ephemeral key
    pub enc: String,
    /// The sealed, signed content and its tag
    pub ciphertext: String,
}

impl Envelope {
    /// Its encoded size, as [`MAX_ENVELOPE_BYTES`] counts it
    pub fn size(&self) -> usize {
        self.enc.len() + self.ciphertext.len()
    }

    /// Whether it's within [`MAX_ENVELOPE_BYTES`]
    pub fn is_within_limit(&self) -> bool {
        self.size() <= MAX_ENVELOPE_BYTES
    }
}

/// The keys a relay pinned for one instance (`PublicKeysResult`); `None`:
/// it has none for that ID
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceKeys {
    pub instance_id: String,
    #[serde(default)]
    pub public_keys: Option<PublicKeys>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn keys() -> PublicKeys {
        PublicKeys {
            signing: "A".repeat(43),
            sealing: "b-_9".repeat(10) + "xyz",
        }
    }

    #[test]
    fn keys_are_well_formed_only_as_43_base64url_characters() {
        assert!(keys().is_well_formed());
        for bad in [
            "A".repeat(42),
            "A".repeat(44),
            "A".repeat(42) + "=",
            "A".repeat(42) + "+",
        ] {
            let mut wrong = keys();
            wrong.sealing = bad;
            assert!(!wrong.is_well_formed());
        }
    }

    #[test]
    fn envelopes_are_camel_case() {
        let envelope = Envelope {
            suite: SEAL_SUITE.into(),
            sealed_for: SealedFor {
                task_id: "t1".into(),
                seq: 2,
                from: "i1".into(),
                to: "i2".into(),
                purpose: SealPurpose::Request,
                capability: "write".into(),
            },
            sender_key_id: "aa".into(),
            recipient_key_id: "bb".into(),
            enc: "e".into(),
            ciphertext: "c".into(),
        };
        let value = serde_json::to_value(&envelope).unwrap();
        assert_eq!(
            value,
            json!({
                "suite": "nandi-seal-v1",
                "sealedFor": {"taskId": "t1", "seq": 2, "from": "i1", "to": "i2",
                              "purpose": "request", "capability": "write"},
                "senderKeyId": "aa", "recipientKeyId": "bb", "enc": "e", "ciphertext": "c"
            })
        );
        assert_eq!(serde_json::from_value::<Envelope>(value).unwrap(), envelope);
        assert_eq!(
            serde_json::to_value(keys()).unwrap(),
            json!({"signing": keys().signing, "sealing": keys().sealing})
        );
    }

    #[test]
    fn envelopes_over_the_limit_are_flagged() {
        let mut envelope = Envelope {
            suite: SEAL_SUITE.into(),
            sealed_for: SealedFor {
                task_id: "t1".into(),
                seq: 0,
                from: "i1".into(),
                to: "i2".into(),
                purpose: SealPurpose::Note,
                capability: String::new(),
            },
            sender_key_id: "aa".into(),
            recipient_key_id: "bb".into(),
            enc: "e".repeat(43),
            ciphertext: "c".repeat(MAX_ENVELOPE_BYTES - 43),
        };
        assert_eq!(envelope.size(), MAX_ENVELOPE_BYTES);
        assert!(envelope.is_within_limit());
        envelope.ciphertext.push('c');
        assert!(!envelope.is_within_limit());
    }

    #[test]
    fn purposes_round_trip_through_their_names() {
        for purpose in [
            SealPurpose::Request,
            SealPurpose::Result,
            SealPurpose::Clarification,
            SealPurpose::Answer,
            SealPurpose::Note,
        ] {
            assert_eq!(serde_json::to_value(purpose).unwrap(), purpose.as_str());
        }
    }
}
