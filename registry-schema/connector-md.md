# connector.md

The authoring format of a connector. A connector is one file, `connector.md`, in a public repo at `connectors/<id>/connector.md`. The folder name is the connector's ID. The publisher writes this file; the registry entry (`connectors/<id>.json`, see [connector.schema.json](connector.schema.json)) is derived from it plus the release.

## The format

YAML frontmatter, then plain instructions for agents.

<!-- prettier-ignore -->
```markdown
---
name: Web page fetch                 # required
description: Fetch a web page and read it as Markdown.   # required, <= 200 chars
version: 1.0.0                       # required, semver; the release
run: uvx mcp-server-fetch==2025.4.7  # stdio: `npx [-y] <package>@<exact version> [args...]` or `uvx <package>==<exact version> [args...]`
# url: https://mcp.example.com/mcp   # OR an HTTP (Streamable HTTP) MCP server; exactly one of run/url
# image: ghcr.io/acme/browser-mcp:1.2.0   # optional, only when the server needs something unusual (a browser, system libraries); the install dialog says so
# placement: host                    # optional, default container; host is refused by the public registry
auth: none                           # none | api_key | oauth | device_code, or a mapping {type, env: [VAR], label, scopes: [...]}
scopes: []                           # oauth/device_code scopes (shorthand for auth.scopes)
hosts: [example.com, "*.example.org"]   # every host it reaches; a stdio server's container reaches only these through the egress proxy
tools:                               # every tool, with read or write access
  - { name: fetch, description: Fetch a URL and return its text, access: read }
reads: [Web pages you ask it to open]    # plain words for the install dialog
writes: []
tags: [web]                          # optional, at least one in the registry entry
---
Plain instructions for agents: when to use it and what to be careful with. They go into an agent's context, with the tools, once the agent is granted the connector.
```

## Rules

- Versions are exact. No ranges (`^1.2.0`, `~1.2`, `1.2.x`), no tags, no `latest`. An `npx` version has all three parts (npm reads `1.2` as a range).
- Exactly one of `run` and `url`.
- `image` only with `run`. An HTTP server has no container of its own.
- `hosts` lists every host the server reaches. A stdio server's container reaches only these, through the egress proxy. An HTTP server's own host goes here too.
- `tools` lists every tool the server offers, each with `read` or `write` access.

## How it maps to the registry entry

| `connector.md`                | Registry entry                                                                     |
| ----------------------------- | ---------------------------------------------------------------------------------- |
| folder name                   | `id`                                                                               |
| `name`, `description`         | `name`, `description`                                                              |
| `version`                     | `latest_version`                                                                   |
| `run: uvx pkg==1.0 a b`       | `kind: mcp-stdio`, `run: {command: uvx, package: pkg, version: 1.0, args: [a, b]}` |
| `run: npx -y pkg@1.0.0 a`     | `kind: mcp-stdio`, `run: {command: npx, package: pkg, version: 1.0.0, args: [a]}`  |
| `url: https://...`            | `kind: mcp-http`, `run: {url: https://...}`                                        |
| `image`                       | `image`                                                                            |
| `placement`                   | `placement`, default `container`                                                   |
| `auth: oauth`                 | `auth: {type: oauth}`                                                              |
| `auth: {type, env, label}`    | `auth: {type, env, label}`                                                         |
| `scopes`                      | `auth.scopes`                                                                      |
| `hosts`                       | `hosts`                                                                            |
| `tools`                       | `tools`                                                                            |
| `reads`, `writes`             | `data: {read, write}`                                                              |
| `tags`                        | `tags`                                                                             |
| the publisher's GitHub handle | `publisher`                                                                        |
| the repo and folder           | `repo`, `path`                                                                     |
| the release tag and commit    | `ref`, `commit`, `signature`                                                       |

`-y` is not part of the entry. Arguments after the package go into `run.args`.

✅ `run: npx -y @acme/mcp-server@1.4.2 --read-only`

❌ `run: npx @acme/mcp-server@^1.4` (a range)

❌ `run: uvx mcp-server-fetch` (no version)

## Tool access and grants

| Access  | Meaning                                               |
| ------- | ----------------------------------------------------- |
| `read`  | Reads only                                            |
| `write` | Changes something: sends, creates, updates or deletes |

A tool the server offers that isn't listed in `tools` counts as `write`.

Grants are per agent and connector. Read and write are granted separately. An agent with a read grant gets only the `read` tools. A write grant adds the `write` tools, and each call of a write tool also asks the user for approval.

## The body

The text after the frontmatter is for agents. Say when to use the connector and what to be careful with. It goes into an agent's context, with the tools, once the agent is granted the connector.
