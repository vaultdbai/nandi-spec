//! Own devices (protocol 8): a person's phone or other browser, paired
//! with their own Nandi, exchanging sealed envelopes with it through the
//! relay. Nothing here reaches another person.
//!
//! # Pairing
//!
//! 1. The person's Nandi (its registered connection, nobody else) sends
//!    `DeviceRequest` with [`DeviceOp::Pair`]. The relay answers
//!    [`DeviceOutcome::Pairing`]: a new device ID ([`is_device_id`]) and a
//!    pairing code, single use and valid for [`PAIRING_TTL_MS`].
//! 2. The Nandi shows a QR code holding a [`PairingInvite`]: the relay,
//!    the Nandi's instance ID and public keys, the device ID and the code.
//! 3. The phone makes its own keys (`nandi-seal`) and claims the code at
//!    [`DEVICE_CLAIM_PATH`] ([`DeviceClaimRequest`] →
//!    [`DeviceClaimResponse`]), which gives it its device token. The relay
//!    pushes `DeviceClaimed` to the Nandi.
//! 4. The Nandi shows the phone's key fingerprint; once the person
//!    confirms it matches the phone's screen, it sends [`DeviceOp::Confirm`]
//!    with those keys. The relay activates the device only when they are
//!    the keys it was claimed with. An unconfirmed claim lapses after
//!    [`PAIRING_TTL_MS`].
//!
//! The relay never hands out keys: the phone has the Nandi's from the QR
//! code, and the Nandi checks the phone's by fingerprint.
//!
//! # Device connections
//!
//! A phone connects to the relay's WebSocket and sends `RegisterDevice`
//! with its token. An active device gets `DeviceRegistered`; a claimed one
//! not yet confirmed gets the `Error` [`DEVICE_PENDING`]. A device
//! connection may send only `OwnEnvelope` (to its home Nandi) and
//! `DeviceHeartbeat`; the relay refuses everything else. It can't act as
//! the Nandi, pair, list or revoke devices, or reach anyone else.
//!
//! # Envelopes
//!
//! `OwnEnvelope { envelope }` (Nandi or device → relay) is delivered live
//! as `OwnDelivery { envelope, receivedAt }`, or refused with
//! [`DEVICE_OFFLINE`] when the other side isn't connected: nothing is
//! stored. The envelope is a sealed and signed [`Envelope`] whose
//! [`crate::SealedFor`] is:
//!
//! | Field        | Value                                                |
//! | ------------ | ---------------------------------------------------- |
//! | `taskId`     | the device ID: the channel between the two           |
//! | `seq`        | increasing per sender; the receiver drops replays    |
//! | `from`, `to` | the sender and the recipient (device or instance ID) |
//! | `purpose`    | [`crate::SealPurpose::OwnDevice`]                    |
//! | `capability` | empty: nothing about the content is in the clear     |
//!
//! The relay sees the sender, the recipient, the time and the size, and
//! checks the route with [`own_envelope_fault`]. The content format is the
//! app's, inside the seal.
//!
//! # Revoking
//!
//! [`DeviceOp::Revoke`] and [`DeviceOp::RevokeAll`] remove a device at
//! once: its token stops working and its connection closes.
//!
//! Offline: device calls answer "not connected"; the Nandi keeps working
//! and may show the devices it last listed. A phone whose relay or Nandi
//! is unreachable says so and does nothing.

use serde::{Deserialize, Serialize};

use crate::keys::{Envelope, PublicKeys, SealPurpose};
use crate::time::Timestamp;

/// How long a pairing code, and a claim waiting for the Nandi's
/// confirmation, stays valid (5 minutes)
pub const PAIRING_TTL_MS: i64 = 5 * 60 * 1000;

/// `POST`: a phone claims a pairing code ([`DeviceClaimRequest`])
pub const DEVICE_CLAIM_PATH: &str = "/relay/devices/claim";

/// What every device ID starts with. Instance IDs never hold a `.`, so a
/// device ID is never an instance ID.
pub const DEVICE_ID_PREFIX: &str = "device.";

/// [`DeviceOutcome::Refused`] and `Error` codes
///
/// No such device of the asker's
pub const DEVICE_NOT_FOUND: &str = "device_not_found";
/// The Nandi has as many devices as the relay allows
pub const DEVICE_LIMIT: &str = "device_limit";
/// `Confirm` named other keys than the device was claimed with
pub const DEVICE_KEYS_MISMATCH: &str = "device_keys_mismatch";
/// The device isn't in the state the op needs (confirming an active one)
pub const DEVICE_STATE: &str = "device_state";
/// A device registered before its Nandi confirmed it
pub const DEVICE_PENDING: &str = "device_pending";
/// The other side of an own-device envelope isn't connected
pub const DEVICE_OFFLINE: &str = "device_offline";
/// A device token that is wrong, revoked or for no device
pub const DEVICE_UNAUTHORIZED: &str = "device_unauthorized";
/// An own-device envelope addressed outside its own channel
pub const NOT_OWN_DEVICE: &str = "not_own_device";
/// A pairing code that is unknown, used or expired
pub const PAIRING_INVALID: &str = "pairing_invalid";

/// Length of a device ID's hex part
const DEVICE_HEX: usize = 32;

/// A device ID: [`DEVICE_ID_PREFIX`] and 32 lowercase hex characters,
/// picked at random by the relay
pub fn is_device_id(id: &str) -> bool {
    id.strip_prefix(DEVICE_ID_PREFIX).is_some_and(|hex| {
        hex.len() == DEVICE_HEX
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Where a device is in its pairing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    /// A pairing code is open; no phone has claimed it
    Pairing,
    /// A phone claimed it; the Nandi hasn't confirmed its keys
    Claimed,
    /// Paired: it may connect and exchange envelopes with its Nandi
    Active,
}

/// One of a Nandi's devices, as the relay knows it. No name: the Nandi
/// keeps those itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct OwnDevice {
    pub device_id: String,
    pub state: DeviceState,
    pub created_at: Timestamp,
    /// When a pairing code or an unconfirmed claim lapses; `None` once
    /// active
    #[serde(default)]
    pub expires_at: Option<Timestamp>,
    /// The phone's keys, from its claim; `None` while pairing
    #[serde(default)]
    pub public_keys: Option<PublicKeys>,
    #[serde(default)]
    pub paired_at: Option<Timestamp>,
    /// When the relay last heard from it
    #[serde(default)]
    pub last_seen_at: Option<Timestamp>,
    /// Connected right now
    #[serde(default)]
    pub online: bool,
}

/// What a Nandi asks of the relay about its own devices
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "op", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum DeviceOp {
    /// Its devices ([`DeviceOutcome::Devices`])
    List,
    /// A new device and its pairing code ([`DeviceOutcome::Pairing`]).
    /// Replaces a pairing still open.
    Pair,
    /// Activates a claimed device: `public_keys` are the ones the person
    /// checked by fingerprint
    Confirm {
        device_id: String,
        public_keys: PublicKeys,
    },
    /// Removes one device: its token stops working and it's disconnected
    Revoke { device_id: String },
    /// Removes every device
    RevokeAll,
}

/// The relay's answer to a [`DeviceOp`]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "outcome",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum DeviceOutcome {
    /// For `List`: oldest first
    Devices { devices: Vec<OwnDevice> },
    /// For `Pair`: the code to show, once
    Pairing {
        device_id: String,
        code: String,
        expires_at: Timestamp,
    },
    /// `Confirm`, `Revoke` or `RevokeAll` done
    Done,
    /// Not done: a code above and a sentence
    Refused { code: String, message: String },
}

/// What the Nandi's pairing QR code holds, for the phone
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PairingInvite {
    /// The relay's WebSocket URL, without a token
    pub relay_url: String,
    /// The Nandi's instance ID: the only recipient the phone will have
    pub home_instance: String,
    /// The Nandi's keys: the phone seals to them and checks its signatures
    pub home_keys: PublicKeys,
    pub device_id: String,
    pub code: String,
    pub expires_at: Timestamp,
}

/// `POST` [`DEVICE_CLAIM_PATH`]: a phone claims a pairing code with its
/// keys. Missing fields read as empty (the relay refuses them).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DeviceClaimRequest {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub public_keys: Option<PublicKeys>,
}

/// The relay's answer to an accepted claim. A refusal is `{"error": "…"}`
/// with a 4xx status instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DeviceClaimResponse {
    pub device_id: String,
    pub home_instance: String,
    /// What the device registers with; shown once, kept on the device
    pub token: String,
}

/// Why `envelope`, sent by `sender` on the own-device channel `device_id`
/// for `recipient`, isn't a well-formed own-device envelope, or `None`
/// when it is (see the module docs). The relay sets `sender`, `recipient`
/// and `device_id` from the connection, never from the envelope.
pub fn own_envelope_fault(
    envelope: &Envelope,
    sender: &str,
    recipient: &str,
    device_id: &str,
) -> Option<&'static str> {
    let sealed = &envelope.sealed_for;
    if sealed.purpose != SealPurpose::OwnDevice {
        Some("An own-device envelope's purpose is ownDevice")
    } else if !sealed.capability.is_empty() {
        Some("An own-device envelope carries no capability")
    } else if sealed.task_id != device_id {
        Some("An own-device envelope's taskId is its device ID")
    } else if sealed.from != sender {
        Some("An own-device envelope comes from its sender")
    } else if sealed.to != recipient {
        Some("An own-device envelope goes only between a Nandi and its own devices")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::fixtures::envelope;
    use crate::keys::SealedFor;

    const DEVICE: &str = "device.0123456789abcdef0123456789abcdef";

    fn keys() -> PublicKeys {
        PublicKeys {
            signing: "s".repeat(43),
            sealing: "x".repeat(43),
        }
    }

    fn own(from: &str, to: &str) -> Envelope {
        let mut envelope = envelope(SealPurpose::OwnDevice);
        envelope.sealed_for = SealedFor {
            task_id: DEVICE.into(),
            seq: 1,
            from: from.into(),
            to: to.into(),
            purpose: SealPurpose::OwnDevice,
            capability: String::new(),
        };
        envelope
    }

    #[test]
    fn device_ids_are_the_prefix_and_32_hex_and_never_an_instance_id() {
        assert!(is_device_id(DEVICE));
        for bad in [
            "device.0123456789ABCDEF0123456789abcdef",
            "device.0123456789abcdef0123456789abcde",
            "device.0123456789abcdef0123456789abcdeff",
            "0123456789abcdef0123456789abcdef",
            "g_0123456789abcdef",
            "",
        ] {
            assert!(!is_device_id(bad), "{bad}");
        }
        assert!(DEVICE.contains('.'));
    }

    #[test]
    fn ops_and_outcomes_are_tagged_snake_case_with_camel_case_fields() {
        assert_eq!(
            serde_json::to_value(DeviceOp::Confirm {
                device_id: DEVICE.into(),
                public_keys: keys(),
            })
            .unwrap(),
            json!({"op": "confirm", "deviceId": DEVICE,
                   "publicKeys": {"signing": keys().signing, "sealing": keys().sealing}})
        );
        assert_eq!(
            serde_json::to_value(DeviceOp::RevokeAll).unwrap(),
            json!({"op": "revoke_all"})
        );
        assert_eq!(
            serde_json::to_value(DeviceOutcome::Pairing {
                device_id: DEVICE.into(),
                code: "c".into(),
                expires_at: 9.0,
            })
            .unwrap(),
            json!({"outcome": "pairing", "deviceId": DEVICE, "code": "c", "expiresAt": 9.0})
        );
        for op in [
            DeviceOp::List,
            DeviceOp::Pair,
            DeviceOp::Revoke {
                device_id: DEVICE.into(),
            },
            DeviceOp::RevokeAll,
        ] {
            let text = serde_json::to_string(&op).unwrap();
            assert_eq!(serde_json::from_str::<DeviceOp>(&text).unwrap(), op);
        }
    }

    #[test]
    fn a_device_reads_with_its_optional_fields_left_out() {
        let device: OwnDevice = serde_json::from_value(json!({
            "deviceId": DEVICE, "state": "pairing", "createdAt": 1.0
        }))
        .unwrap();
        assert_eq!(device.state, DeviceState::Pairing);
        assert!(device.public_keys.is_none() && !device.online);
        let claim: DeviceClaimRequest = serde_json::from_value(json!({})).unwrap();
        assert_eq!(claim, DeviceClaimRequest::default());
        let invite = PairingInvite {
            relay_url: "wss://relay.example/relay".into(),
            home_instance: "ada".into(),
            home_keys: keys(),
            device_id: DEVICE.into(),
            code: "c".into(),
            expires_at: 2.0,
        };
        let value = serde_json::to_value(&invite).unwrap();
        assert_eq!(value["homeInstance"], "ada");
        assert_eq!(
            serde_json::from_value::<PairingInvite>(value).unwrap(),
            invite
        );
    }

    #[test]
    fn own_envelopes_stay_on_their_channel() {
        assert_eq!(
            own_envelope_fault(&own("ada", DEVICE), "ada", DEVICE, DEVICE),
            None
        );
        assert_eq!(
            own_envelope_fault(&own(DEVICE, "ada"), DEVICE, "ada", DEVICE),
            None
        );
        let other = "device.ffffffffffffffffffffffffffffffff";
        let cases: [(Envelope, &str, &str); 6] = [
            (envelope(SealPurpose::Request), "ada", DEVICE),
            (
                {
                    let mut e = own("ada", DEVICE);
                    e.sealed_for.capability = "inbox.approve".into();
                    e
                },
                "ada",
                DEVICE,
            ),
            (
                {
                    let mut e = own("ada", DEVICE);
                    e.sealed_for.task_id = "t1".into();
                    e
                },
                "ada",
                DEVICE,
            ),
            // Forged sender
            (own("bob", DEVICE), "ada", DEVICE),
            // Another person, or another device
            (own(DEVICE, "bob"), DEVICE, "ada"),
            (own("ada", other), "ada", DEVICE),
        ];
        for (envelope, sender, recipient) in cases {
            assert!(
                own_envelope_fault(&envelope, sender, recipient, DEVICE).is_some(),
                "{:?}",
                envelope.sealed_for
            );
        }
    }
}
