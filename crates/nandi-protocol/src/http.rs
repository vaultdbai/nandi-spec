//! The relay's HTTP side and how clients reach it: the WebSocket path, the
//! `/health`, `/instances` and `/relay/enroll` bodies, and the instance
//! token carried in the relay URL (`…/relay?token=…`).
//!
//! Offline: a client that can't reach these endpoints reports "not
//! connected" and keeps working on what it last knew.

use serde::{Deserialize, Serialize};

use crate::card::AgentCard;
use crate::gateway::GatewayCard;
use crate::keys::PublicKeys;
use crate::time::Timestamp;

/// Default port of a relay
pub const DEFAULT_RELAY_PORT: u16 = 8787;

/// The WebSocket path
pub const RELAY_PATH: &str = "/relay";

/// Environment variable holding the relay's secret
pub const SECRET_ENV: &str = "NANDI_RELAY_SECRET";

/// Environment variable holding the join code instances enroll with
pub const JOIN_CODE_ENV: &str = "NANDI_RELAY_JOIN_CODE";

/// The ping payload of "Test connection"; the relay's pong echoes it
pub const PROBE_PAYLOAD: &[u8] = b"nandi-relay-probe";

/// `GET /health`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayHealth {
    pub status: String,
    pub instances_connected: u32,
    pub uptime_seconds: u64,
    pub version: String,
    /// `open` or `token`
    pub auth: String,
    /// Whether the relay serves `wss://` itself
    #[serde(default)]
    pub tls: bool,
    /// The relay's [`crate::PROTOCOL_VERSION`]
    #[serde(default)]
    pub protocol_version: u32,
}

/// One entry of `GET /instances`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayInstanceInfo {
    pub instance_id: String,
    /// Remote address of its connection
    pub peer: String,
    pub agents: Vec<AgentCard>,
    /// Its gateway card, if it registered one
    #[serde(default)]
    pub gateway: Option<GatewayCard>,
    /// Milliseconds since the epoch
    pub connected_at: Timestamp,
    pub last_heartbeat: Timestamp,
    pub seconds_since_heartbeat: u64,
}

/// `POST /relay/enroll`: an instance asks for its token with the relay's
/// join code. Missing fields read as empty (the relay refuses them).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollRequest {
    #[serde(default)]
    pub instance_id: String,
    #[serde(default)]
    pub join_code: String,
    /// The instance's keys, which the relay pins for its ID
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_keys: Option<PublicKeys>,
}

/// The relay's answer to an accepted [`EnrollRequest`]: the token this
/// instance connects with from now on. A refusal is `{"error": "…"}` with
/// a 4xx status instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollResponse {
    pub instance_id: String,
    pub token: String,
}

/// Splits the instance token out of a relay URL: `(url without it, token)`.
pub fn split_token(url: &str) -> (String, Option<String>) {
    let Some((base, query)) = url.split_once('?') else {
        return (url.to_string(), None);
    };
    let mut token = None;
    let rest: Vec<&str> = query
        .split('&')
        .filter(|pair| match pair.strip_prefix("token=") {
            Some(value) => {
                token = Some(value.to_string()).filter(|t| !t.is_empty());
                false
            }
            None => !pair.is_empty(),
        })
        .collect();
    if rest.is_empty() {
        (base.to_string(), token)
    } else {
        (format!("{base}?{}", rest.join("&")), token)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn tokens_are_split_out_of_the_url() {
        assert_eq!(
            split_token("wss://relay.example/relay?token=abc"),
            (
                "wss://relay.example/relay".to_string(),
                Some("abc".to_string())
            )
        );
        assert_eq!(
            split_token("ws://h/relay?x=1&token=abc&y=2"),
            ("ws://h/relay?x=1&y=2".to_string(), Some("abc".to_string()))
        );
        assert_eq!(
            split_token("ws://h/relay"),
            ("ws://h/relay".to_string(), None)
        );
        assert_eq!(
            split_token("ws://h/relay?token="),
            ("ws://h/relay".to_string(), None)
        );
    }

    #[test]
    fn enroll_bodies_are_camel_case() {
        let request = EnrollRequest {
            instance_id: "i1".into(),
            join_code: "code".into(),
            public_keys: None,
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({"instanceId": "i1", "joinCode": "code"})
        );
        let empty: EnrollRequest = serde_json::from_value(json!({})).unwrap();
        assert_eq!(empty, EnrollRequest::default());
        let response: EnrollResponse =
            serde_json::from_value(json!({"instanceId": "i1", "token": "t"})).unwrap();
        assert_eq!(
            response,
            EnrollResponse {
                instance_id: "i1".into(),
                token: "t".into()
            }
        );
    }

    #[test]
    fn health_and_instances_are_snake_case() {
        let health: RelayHealth = serde_json::from_value(json!({
            "status": "ok", "instances_connected": 2, "uptime_seconds": 5,
            "version": "1", "auth": "token"
        }))
        .unwrap();
        assert!(!health.tls);
        assert_eq!(health.protocol_version, 0);
        let info = RelayInstanceInfo {
            instance_id: "i1".into(),
            peer: "127.0.0.1:1".into(),
            agents: vec![],
            gateway: None,
            connected_at: 1.0,
            last_heartbeat: 2.0,
            seconds_since_heartbeat: 3,
        };
        let value = serde_json::to_value(&info).unwrap();
        assert_eq!(value["instance_id"], "i1");
        assert_eq!(value["seconds_since_heartbeat"], 3);
    }
}
