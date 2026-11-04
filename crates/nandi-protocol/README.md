# nandi-protocol

The Nandi relay wire protocol: every WebSocket message and HTTP body a relay server and its clients exchange, plus the data they carry (agent and gateway cards, tasks and results, capability presence, clarification questions).

Content is sealed end to end. A task's input, its output and error, questions, answers, notes and reasons travel only inside an `Envelope` sealed for the recipient (`nandi-seal`). The relay reads the metadata beside it: IDs, capability, kind, state, typed reason, ETA, progress, place in line, deadline and display names.

A relay depends on this crate and never on the Nandi app; the app depends on it too.

| Module          | What                                                                                     |
| --------------- | ---------------------------------------------------------------------------------------- |
| `message`       | `RelayMessage` (tagged by `type`) and `PeerMessage` (tagged by `kind`), JSON text frames |
| `http`          | `/health`, `/instances`, `/relay/enroll` bodies; `RELAY_PATH`, env names, `split_token`  |
| `card`          | `AgentCard`, `OwnerRef`, `ServesCapability`                                              |
| `gateway`       | `GatewayCard`                                                                            |
| `directory`     | `MemberRecord`, `Availability`, `PeopleMatch`: the member directory and people search    |
| `keys`          | `PublicKeys`, `Envelope`, `SealedFor`, `SealPurpose`, `InstanceKeys`, `MAX_ENVELOPE_BYTES`: per-instance keys and sealed envelopes (the crypto is `nandi-seal`) |
| `content`       | What each envelope holds: `RequestContent`, `ResultContent`, `ClarificationContent`, `AnswerContent`, `NoteContent` |
| `presence`      | Capability presence (`CapabilityPresence`, `InstancePresence`, `ProviderKind`)           |
| `routing`       | `RoutingQuery`, `RoutingHints`, `RoutePlan`, `RouteCandidate`, `RouteReason`: who gets a request (`RouteQuery` → `RoutePlan`) |
| `task`          | `SealedTask`, `ResultReport` (on the wire); `Task`, `TaskResult` (opened); `TaskState`, `ReasonKind` |
| `clarification` | `ClarificationRequest`, `ClarificationResponse`, `Question` (opened forms; on the wire they're sealed) |
| `time`          | `Timestamp` (epoch ms as `f64`), `now_ms`                                                |

## Features

- `specta`: derives `specta::Type` on the types the Nandi app exports to TypeScript.

## Compatibility

Serde shapes are the wire format. A field is never renamed or retyped without a `PROTOCOL_VERSION` bump; new fields come with `#[serde(default)]`. `RegisterInstance` carries the client's version and `/health` the relay's; a relay refuses another version with the `Error` code `version_mismatch`. Version 2: the member directory (`QueryDirectory`/`Directory`) and people search (`FindPeople`/`PeopleMatches`). Version 3: `EnrollRequest` and `RegisterInstance` carry the instance's `publicKeys`; the relay pins them and refuses other keys for the same ID with the code `key_mismatch` until its operator rekeys the ID; `MemberRecord` carries the pinned keys. Version 4: every person-to-person payload is sealed. `envelope` replaces `input` (`DelegateTask`), the result's `output`, `error` and `input` (`ResultReport`), notes, reasons, questions and answers (`PeerMessage`), and a failed duty's error; `PublicKeysQuery`/`PublicKeysResult` give the keys the relay pinned; an envelope is at most `MAX_ENVELOPE_BYTES` (1 MiB), and a relay refuses a bigger one with `envelope_too_large`. Version 5: routing at the relay. `RouteQuery { queryId, query }` (a `RoutingQuery`: the `kind` (`delegate`, `ask`, `assign`), the capability, a `target` (`anyone`, a `person` or an `agent`) and `hints`: `preferred`, `excluded`, `strategy`, `requiredTools`, `maxCandidates`) is answered by `RoutePlan { queryId, plan }`: the best candidates (3 by default), each with its pinned keys and its `reasons`, typed `RouteReason` codes with their parameters so a client renders them in its own language. Reasons never carry another member's success rate. With the relay down there is no plan: a client says "not connected" for network requests and keeps running local ones.

## Sealing

| Message | Envelope | Purpose | Content |
| --- | --- | --- | --- |
| `DelegateTask`, `TaskAssigned.task` | `envelope` | `request` | `RequestContent` |
| `TaskResult`, `TaskCompleted` | `result.envelope` | `result` | `ResultContent` |
| `StatusResponse` | `envelope` | `result` | `ResultContent` |
| `ClarificationRequest` / `ClarificationResponse` | `envelope` | `clarification` / `answer` | `ClarificationContent` / `AnswerContent` |
| `ReassignTask` | `request` | `request` | The request, resealed for the new provider |
| `AcknowledgeTask`, `RejectTask`, `ProgressUpdate`, `CancelTask`, `TaskTimedOut`, `ReassignTask`, `DutyStatus` | `envelope` | `note` | `NoteContent` |

- A request and a result bind the task's capability; lifecycle envelopes bind `LIFECYCLE_CAPABILITY` (empty).
- `seq` is unique among one sender's envelopes for a task and increases. A receiver drops a `(taskId, from, seq)` it already opened.
- A reassignment keeps the requester's signature: the provider reseals the request for the new provider (`ReassignTask.request`), and the requester sends it on as the next attempt's `envelope` with `continues` naming the old attempt, whose ID the envelope is sealed under.

## Use

```toml
nandi-protocol = { git = "https://github.com/vaultdbai/nandi-spec", branch = "main" }
```
