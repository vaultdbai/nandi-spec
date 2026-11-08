# nandi-protocol

The Nandi relay wire protocol: every WebSocket message and HTTP body a relay server and its clients exchange, plus the data they carry (agent and gateway cards, tasks and results, capability presence, clarification questions).

Content is sealed end to end. A task's input, its output and error, questions, answers, notes and reasons travel only inside an `Envelope` sealed for the recipient (`nandi-seal`). The relay reads the metadata beside it: IDs, capability, kind, state, typed reason, ETA, progress, place in line, deadline and display names.

A relay depends on this crate and never on the Nandi app; the app depends on it too.

| Module          | What                                                                                     |
| --------------- | ---------------------------------------------------------------------------------------- |
| `message`       | `RelayMessage` (tagged by `type`) and `PeerMessage` (tagged by `kind`), JSON text frames |
| `mail`          | `Delivery`, `TaskEvent`, `FailoverCause`: store-and-forward, each instance's mailbox at the relay |
| `duty`          | `DutyState`, `DutyDecision`, `HandoverReason`: calendar handovers, watched at the relay |
| `http`          | `/health`, `/instances`, `/relay/enroll` bodies; `RELAY_PATH`, env names, `split_token`  |
| `card`          | `AgentCard`, `OwnerRef`, `ServesCapability`                                              |
| `gateway`       | `GatewayCard`                                                                            |
| `directory`     | `MemberRecord`, `Availability`, `PeopleMatch`: the member directory and people search    |
| `keys`          | `PublicKeys`, `Envelope`, `SealedFor`, `SealPurpose`, `InstanceKeys`, `MAX_ENVELOPE_BYTES`: per-instance keys and sealed envelopes (the crypto is `nandi-seal`) |
| `content`       | What each envelope holds: `RequestContent`, `ResultContent`, `ClarificationContent`, `AnswerContent`, `NoteContent` |
| `presence`      | Capability presence (`CapabilityPresence`, `InstancePresence`, `ProviderKind`)           |
| `routing`       | `RoutingQuery`, `RoutingHints`, `RoutePlan`, `RouteCandidate`, `RouteReason`: who gets a request (`RouteQuery` → `RoutePlan`) |
| `task`          | `SealedTask`, `TaskAttempt`, `ResultReport` (on the wire); `Task`, `TaskResult` (opened); `TaskState`, `ReasonKind` |
| `clarification` | `ClarificationRequest`, `ClarificationResponse`, `Question` (opened forms; on the wire they're sealed) |
| `time`          | `Timestamp` (epoch ms as `f64`), `now_ms`                                                |

## Features

- `specta`: derives `specta::Type` on the types the Nandi app exports to TypeScript.

## Compatibility

Serde shapes are the wire format. A field is never renamed or retyped without a `PROTOCOL_VERSION` bump; new fields come with `#[serde(default)]`. `RegisterInstance` carries the client's version and `/health` the relay's; a relay refuses another version with the `Error` code `version_mismatch`. Version 2: the member directory (`QueryDirectory`/`Directory`) and people search (`FindPeople`/`PeopleMatches`). Version 3: `EnrollRequest` and `RegisterInstance` carry the instance's `publicKeys`; the relay pins them and refuses other keys for the same ID with the code `key_mismatch` until its operator rekeys the ID; `MemberRecord` carries the pinned keys. Version 4: every person-to-person payload is sealed. `envelope` replaces `input` (`DelegateTask`), the result's `output`, `error` and `input` (`ResultReport`), notes, reasons, questions and answers (`PeerMessage`), and a failed duty's error; `PublicKeysQuery`/`PublicKeysResult` give the keys the relay pinned; an envelope is at most `MAX_ENVELOPE_BYTES` (1 MiB), and a relay refuses a bigger one with `envelope_too_large`. Version 5: routing at the relay. `RouteQuery { queryId, query }` (a `RoutingQuery`: the `kind` (`delegate`, `ask`, `assign`), the capability, a `target` (`anyone`, a `person` or an `agent`) and `hints`: `preferred`, `excluded`, `strategy`, `requiredTools`, `maxCandidates`) is answered by `RoutePlan { queryId, plan }`: the best candidates (3 by default), each with its pinned keys and its `reasons`, typed `RouteReason` codes with their parameters so a client renders them in its own language. Reasons never carry another member's success rate. With the relay down there is no plan: a client says "not connected" for network requests and keeps running local ones. Version 6: the relay owns person-to-person work. `SubmitTask` (replacing `DelegateTask`) carries the request sealed for each candidate (`attempts`); everything the relay sends an instance about a task or a duty is a `Delivery` in its mailbox, pushed as `Deliver { mailId, item }` and kept until `Ack { mailIds }`; `InboxSync` sends what wasn't acked. The relay tells a party what it did with a task (`TaskEvent`: `accepted`, `refused`, `failed_over`, `reassigned`, `ended`), so failover, timeouts and abandons happen with the requester offline. `Reassign` (with `ReassignResult`) moves a task at the relay; a manager's move asks the provider to reseal (`Delivery::Reseal`). `DutyWatch` and `DutyReport` let the relay decide calendar handovers (`Delivery::DutyQuery`, `Delivery::Duty`). Live-only peer messages (`PeerMessage::is_live_only`) aren't stored. Gone: `DelegateTask`, `TaskAssigned`, `TaskCompleted`, `TaskFailed`, `Undeliverable`, the relay's `Peer` to a client, `ReassignTask`, `TaskTimedOut`, `DutyQuery`/`DutyStatus` peer messages, `StatusResponse`'s result and `SealedTask.continues`. Version 7: groups. `GroupRequest { requestId, op }` (a `GroupOp`: `list`, `get`, `create`, `update`, `set_policy`, `invite`, `revoke_invite`, `join`, `leave`, `remove`, `set_role`) is answered by `GroupReply { requestId, outcome }` (`list`, `group`, `invite`, `done` or `refused` with a code), and `GroupChanged { groupId, change }` goes to a group's members and to someone invited. Nobody is in a group by default; everyone can list groups with their names and descriptions, and only members see a group's members, invites and policy. Each group has a policy file (`GroupPolicy`: Markdown with YAML frontmatter, versioned, signed by the admin who wrote it over `GroupPolicy::signed_bytes`); members check the signature and enforce it, and the relay checks only that the author is an admin and the version is the next. `Registered` and `/health` carry the relay's `Visibility`: `company` (everyone sees everyone) or `groups` (a public relay: the directory, people search, capabilities, presence, routing, keys and requests are limited to people who share a group, and a request to anyone else is refused with `no_shared_group`).

## Sealing

| Message | Envelope | Purpose | Content |
| --- | --- | --- | --- |
| `SubmitTask` (each of `attempts`), `Deliver` `task` | `envelope` | `request` | `RequestContent` |
| `TaskResult`, `Deliver` `result` | `result.envelope` | `result` | `ResultContent` |
| `ClarificationRequest` / `ClarificationResponse` | `envelope` | `clarification` / `answer` | `ClarificationContent` / `AnswerContent` |
| `Reassign` | `request` | `request` | The request, resealed for the new provider |
| `AcknowledgeTask`, `RejectTask`, `ProgressUpdate`, `CancelTask`, `Reassign`, `TaskEvent::Reassigned`, `DutyReport`, `Deliver` `duty` | `envelope` | `note` | `NoteContent` |

- A request and a result bind the task's capability; lifecycle envelopes bind `LIFECYCLE_CAPABILITY` (empty).
- `seq` is unique among one sender's envelopes for a task and increases. A receiver drops a `(taskId, from, seq)` it already opened.
- A task keeps its ID through failover and reassignment, so every envelope for it is bound to that ID. A reassignment keeps the requester's signature: the provider reseals the request for the new provider (`Reassign.request`); a provider without it leaves it out and the relay asks the requester, the author, to seal it again.

## Use

```toml
nandi-protocol = { git = "https://github.com/vaultdbai/nandi-spec", branch = "main" }
```
