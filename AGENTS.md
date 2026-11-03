# AI Agent Instructions

## Overview

Open formats for Nandi: `registry-schema/` (JSON Schemas for registry files) and `crates/nandi-protocol` (the relay wire protocol, shared by the Nandi app and the relay). MIT licensed.

Inside the Nandi workspace, the workspace `AGENTS.md` adds the general standards. This file is the stand-alone minimum for contributors.

## Core Rules

- Before committing, run all of these. CI runs clippy with and without `specta`:

  ```bash
  cargo fmt --check
  cargo clippy --all-targets -- -D warnings
  cargo clippy --all-targets --features specta -- -D warnings
  cargo test
  cargo test --features specta
  ```

- **Cross-repo changes go spec first.** Change the wire types here, run the gates and push, before nandi-relay or the Nandi app uses them. Then the relay, then the app.
- No legacy code: nothing is released, so no serde aliases, fallbacks or shims.
- No unsolicited commits, and never force-push. Use Conventional Commits (`feat`, `fix`, `docs`, `refactor`, `test`, `chore`, with a scope where it helps).
- Use modern Rust formatting: `format!("{variable}")`.
- `nandi-protocol` holds data shapes and pure functions only: no I/O, no async, no app state.
- `specta` stays behind the optional `specta` feature (`#[cfg_attr(feature = "specta", derive(specta::Type))]`), pinned to the version the Nandi app uses.

### Offline Semantics (CRITICAL)

Every relay call in the protocol defines what the client does when the
relay is unreachable (usually: report "not connected" and keep working).

### Wire Compatibility (CRITICAL)

Serde shapes are the wire format. Never rename or retype a field without
a protocol version bump; add fields with `#[serde(default)]`.

## Licence

MIT (see `LICENSE`), copyright VaultDB.ai. Never copy code from nandi-relay (BUSL-1.1) into this repo.
