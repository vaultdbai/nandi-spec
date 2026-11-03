//! Sample wire values for the tests

use serde_json::json;

use crate::{
    AgentCard, Availability, DelegationDirection, GatewayCard, OwnerRef, ServesCapability, Task,
    TaskResult, TaskStatus,
};

pub(crate) fn card() -> AgentCard {
    AgentCard {
        id: "i1/papers".into(),
        name: "Papers".into(),
        description: "Finds papers".into(),
        instance_id: "i1".into(),
        endpoint: "ws://relay/a/i1/papers".into(),
        capabilities: vec![ServesCapability {
            name: "find-papers".into(),
            description: String::new(),
            input_schema: Some(json!({"type": "object"})),
            output_schema: None,
        }],
        accepts_delegation: true,
        max_concurrent: 2,
        current_load: 1,
        owner: OwnerRef {
            github_handle: "ada".into(),
            display_name: "Ada".into(),
        },
        signature: None,
        issued_at: 1.0,
    }
}

pub(crate) fn task() -> Task {
    Task {
        id: "t1".into(),
        requester: card(),
        target: "i2/writer".into(),
        capability: "write".into(),
        input: json!({"topic": "rust"}),
        created_at: 5.0,
        deadline: Some(10.0),
    }
}

pub(crate) fn result() -> TaskResult {
    TaskResult {
        task_id: "t1".into(),
        provider_agent: "i2/writer".into(),
        output: json!({"text": "hi"}),
        status: TaskStatus::Completed,
        duration_ms: 12,
        tokens_in: 3,
        tokens_out: 4,
        error: None,
        direction: DelegationDirection::Inbound,
        capability: "write".into(),
        requester_agent: "i1/papers".into(),
        provider_owner: None,
        requester_owner: Some("ada".into()),
        input: json!({"topic": "rust"}),
        started_at: 5.0,
        ended_at: Some(17.0),
    }
}

pub(crate) fn gateway_card() -> GatewayCard {
    GatewayCard {
        instance_id: "i1".into(),
        endpoint: "ws://relay/a/i1/gateway".into(),
        display_name: "Ada".into(),
        description: "Ada's Nandi".into(),
        capabilities: vec!["find-papers".into()],
        accepts_delegation: true,
        max_concurrent: 3,
        current_load: 0,
        owner: card().owner,
        federation_enabled: false,
        gateway_version: 1,
        agent_count: 1,
        body: None,
        direct_delegation: None,
        responsibilities: Vec::new(),
        availability: Availability::available(),
    }
}
