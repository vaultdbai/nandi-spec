# Nandi Spec

Open formats for [Nandi](https://github.com/vaultdbai/nandi): capability files, agent and pack manifests, the registry index, and the relay wire protocol (`nandi-protocol`). MIT licensed so anyone can adopt them.

**Status:** `registry-schema/` (registry file schemas) and `crates/nandi-protocol` (the relay wire types, used by the Nandi app and the relay) live here. Capability file and pack manifest specs are still to come.

| Path                     | What                                                            |
| ------------------------ | --------------------------------------------------------------- |
| `registry-schema/`       | JSON Schemas for registry files (agents, packs, publishers)     |
| `crates/nandi-protocol`  | Rust crate: every message and HTTP body a relay and clients use |

## Rule

Every relay call defines what a client does when the relay is unreachable (usually: report "not connected" and keep working).

## Development

```sh
cargo test
cargo test --features specta
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```
