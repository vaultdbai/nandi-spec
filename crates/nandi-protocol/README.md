# nandi-protocol

The Nandi relay wire protocol: every WebSocket message and HTTP body a relay server and its clients exchange, plus the data they carry (agent and gateway cards, tasks and results, capability presence, clarification questions).

A relay depends on this crate and never on the Nandi app; the app depends on it too.

| Module          | What                                                                                     |
| --------------- | ---------------------------------------------------------------------------------------- |
| `message`       | `RelayMessage` (tagged by `type`) and `PeerMessage` (tagged by `kind`), JSON text frames |
| `http`          | `/health`, `/instances`, `/relay/enroll` bodies; `RELAY_PATH`, env names, `split_token`  |
| `card`          | `AgentCard`, `OwnerRef`, `ServesCapability`                                              |
| `gateway`       | `GatewayCard`                                                                            |
| `directory`     | `MemberRecord`, `Availability`, `PeopleMatch`: the member directory and people search    |
| `keys`          | `PublicKeys`, `Envelope`, `SealedFor`, `SealPurpose`: per-instance keys and sealed envelopes (the crypto is `nandi-seal`) |
| `presence`      | Capability presence (`CapabilityPresence`, `InstancePresence`, `ProviderKind`)           |
| `task`          | `Task`, `TaskResult`, `TaskState`, `ReasonKind`                                          |
| `clarification` | `ClarificationRequest`, `ClarificationResponse`, `Question`                              |
| `time`          | `Timestamp` (epoch ms as `f64`), `now_ms`                                                |

## Features

- `specta`: derives `specta::Type` on the types the Nandi app exports to TypeScript.

## Compatibility

Serde shapes are the wire format. A field is never renamed or retyped without a `PROTOCOL_VERSION` bump; new fields come with `#[serde(default)]`. `RegisterInstance` carries the client's version and `/health` the relay's; a relay refuses another version with the `Error` code `version_mismatch`. Version 2: the member directory (`QueryDirectory`/`Directory`) and people search (`FindPeople`/`PeopleMatches`). Version 3: `EnrollRequest` and `RegisterInstance` carry the instance's `publicKeys`; the relay pins them and refuses other keys for the same ID with the code `key_mismatch` until its operator rekeys the ID; `MemberRecord` carries the pinned keys.

## Use

```toml
nandi-protocol = { git = "https://github.com/vaultdbai/nandi-spec", branch = "main" }
```
