# Changelog

What changed in each release of the Nandi spec crates (`nandi-protocol`, `nandi-seal`, `registry-schema`) and the registry schemas.

## 0.2.0 (2026-11-16)

`PROTOCOL_VERSION` goes from 2 to 7. `MIN_SUPPORTED_PROTOCOL` is 7.

### New

- **`nandi-seal`**, a new crate (`nandi-seal-v1`): per-device Ed25519 signing and X25519 sealing keys, key IDs and fingerprints, and sealing with HPKE (X25519, HKDF-SHA256, ChaCha20-Poly1305). A seal binds the task, sequence, sender, recipient, purpose, capability and both key IDs.
- **Sealed payloads (protocol 4).** Every person-to-person content field travels in an `Envelope` sealed for its recipient. `MAX_ENVELOPE_BYTES` is 1 MiB.
- **Routing (protocol 5).** `RouteQuery` and `RoutePlan`: the relay says who gets a request, with typed reasons a client words in its own language.
- **The team brain (protocol 6).** `SubmitTask`, mailbox deliveries (`Deliver`, `Ack`, `InboxSync`), `TaskEvent`, `Reassign`, and `DutyWatch` and `DutyReport` for calendar handovers.
- **Groups (protocol 7).** `GroupRequest`, `GroupReply` and `GroupChanged`, signed `GroupPolicy`, and `Visibility` (company or groups) on `Registered` and `/health`.
- **Group request rules.** `GroupRequestRules::read` reads a policy's `requests:` block: capabilities (allow and deny globs), hours with an IANA time zone, `always_ask`, approval and daily limit. `SealedTask` gains `firstTimeGroups` and `groupHolds`, and refusals carry a code.
- **Version window.** `ProtocolRange` accepts a range of versions. `MessageKind::since` gives the version each message came with, so a relay never sends a peer a message it can't read.
- **Connector schema.** `registry-schema` gains `connector.schema.json` and the `connector.md` authoring format. A public connector can't use `placement: host`.

### Changed

- `RelayMessage::Error` carries `taskId` when the error is about a task. The final task codes are constants: `TASK_ENDED`, `NOT_A_PARTY`, `UNKNOWN_TASK`.
- `TaskStatus`, `DelegationDirection`, `TaskState` and `ReasonKind` parse with strum and return an error for unknown text, instead of guessing.
- Registry schema version checks refuse wildcards like `1.2.x`, and `npx` versions need all three parts.

### Removed

- `DelegateTask`, `TaskAssigned`, `TaskCompleted`, `TaskFailed`, `Undeliverable`, `ReassignTask`, `TaskTimedOut`, and the `DutyQuery` and `DutyStatus` peer messages. The protocol 6 messages replace them.

### Upgrading from 0.1.0

- Protocol 7 is not wire compatible with protocol 2. Update the relay and every client to 0.2.0 together.
