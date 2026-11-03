# Nandi Registry Schema

JSON Schemas for the files in a Nandi registry: the git repos that list the agents and packs people can install.

**This schema is authoritative for both the in-app marketplace and the marketplace page on vaultdb.ai.** Both read the same registry files; neither may require a field the schema doesn't declare, or reinterpret one it does.

| File                    | Describes                    | Lives in the registry at          |
| ----------------------- | ---------------------------- | --------------------------------- |
| `agent.schema.json`     | One installable agent        | `agents/<id>.json`                |
| `pack.schema.json`      | A curated bundle of agents   | `packs/<id>.json`                 |
| `publisher.schema.json` | Someone who publishes agents | `publishers/<github_handle>.json` |

A registry also has an `index.json` that lists its agents and packs.

## Where the canonical registry lives

[github.com/vaultdbai/nandi-registry](https://github.com/vaultdbai/nandi-registry). Nandi adds it by default (`DEFAULT_REGISTRY` in `src-tauri/src/services/domain/settings.rs` of [vaultdbai/nandi](https://github.com/vaultdbai/nandi)). Users can add more registries, including a private community registry, from **Settings › Sources**.

## Publishing an agent

1. Push the agent to a public GitHub repo (its folder holds `skill.md`) and tag the release `v<version>`.
2. Fork the registry repo.
3. Add `agents/<id>.json` matching `agent.schema.json`. If you're new, add `publishers/<your-handle>.json` too.
4. Add the agent to the `agents` list in `index.json`.
5. Open a pull request. Maintainers review the capabilities the agent requests before merging.

**Publish** in the app does steps 2–5 for you.

## Fields

### Required (agent)

`id`, `name`, `description`, `publisher`, `repo`, `path`, `latest_version`, `tags` (at least one), `min_nandi_version`, `capabilities` (`tools[]`, `network[]`, `pilots[]`), `icon_url`, `readme_url`.

Packs require the same fields, except `path`, plus `agents`: the agents in the pack, each as an `{id, name, description}` object, so a pack file describes itself:

```json
"agents": [
  { "id": "arxiv-researcher", "name": "ArXiv Researcher", "description": "Searches arXiv..." },
  { "id": "paper-summarizer", "name": "Paper Summarizer", "description": "..." }
]
```

The app also accepts a bare ID string in place of an object (named after its ID) so older pack files keep working, but new pack files use objects.

### Reserved (optional)

`install_count`, `rating`, `verified`, `signature`, `deprecated`, `license`. Neither consumer may give these another meaning.

### Versions

`latest_version` and `min_nandi_version` are semver (`1.2.3`, optionally `-pre` or `+build`). The app refuses to install an agent whose `min_nandi_version` is newer than the app itself.

### Signatures

A publisher who signs releases lists an ed25519 `public_key` (`ed25519:<base64>`) in their publisher file. Their agent entries then carry a `signature` over the release commit, which the app verifies before installing.

## How the consumers read a registry

Both consumers read `index.json` at the registry's root (on GitHub, from `raw.githubusercontent.com` at `HEAD`); the per-entry files under `agents/` and `packs/` are for review and history. An index is either `{ "schema_version": 1, "agents": [...], "packs": [...] }` or, for older registries, a bare array of agent entries. A `schema_version` newer than the consumer knows is an error.

**Parsing is lenient, validation is strict.** The schema says what a publisher must write; the consumers accept the same entries as each other, so an entry looks the same in the app and on the website:

- Fields the app needs to install are needed by both: `id`, `name`, `publisher`, `repo`, `latest_version`, and `path` for agents. Other missing fields take defaults: no description, no tags, `min_nandi_version` `0.0.0`, no capabilities, no icon or README.
- Every entry is checked against the app's domain rules: IDs, publisher handles (lowercased), relative `path`s (no `..`), git refs, semver versions, capability names, and icon and README locations (`https://` or a path relative to the registry). An entry that fails is skipped with a warning. The rest of the index still loads.
- Fields beyond the schema that the app reads are read by the website too: `ref` (the git ref to install, default `v<latest_version>`), `commit` (the release's full SHA, which a `signature` names), `capabilities.requires_approval` and `capabilities.aws`, `estimated_cost_usd` (`{min, max}`), and for packs `workflows` and `install_size_kb`.
- A publisher file needs only `github_handle`. Without `display_name`, the handle is shown.
- When two entries share an ID, the highest `latest_version` wins and a tie goes to the earlier one.

### The vaultdb.ai marketplace page

`marketplace.html` on vaultdb.ai (repo `vaultdbai/vaultdb-web`) lists the catalog. Its `scripts/build-catalog.mjs` fetches the canonical registry, applies the rules above (including the publisher signature check), and writes a committed snapshot (`js/catalog-data.js`) that the page renders. The site's deploy workflow refreshes the snapshot daily and keeps the previous one if the registry can't be fetched. **Verified** means the same as in the app: the publisher's `public_key` signed exactly this `kind`, `id`, `latest_version`, and `commit`.
