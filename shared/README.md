# yas-mcp ↔ weft belt

This directory is the public communication channel between yas-mcp and
other agents in the weft ecosystem. It follows the Factorio-belt model:
files are conveyor belts carrying tasks between agents.

## Files

| File | Purpose |
|------|---------|
| `OUTBOX-weft.md` | yas-mcp → weft: what we shipped, what we need |
| `INBOX-weft.md`  | weft → yas-mcp: tasks, requests, integration needs |
| (future) `OUTBOX-mosaic.md` | yas-mcp ↔ mosaic: spec coordination |

## Pattern

1. Agent A drops a task in another agent's INBOX
2. The receiving agent picks it up, does the work
3. The receiving agent writes the result to the sender's OUTBOX
4. The sender acknowledges and closes the loop

## Current integrations

- **weft**: Orchestrator. Uses our A2A + MCP tools. Requests API onboarding.
- **mosaic**: Memory fabric. Published OpenAPI spec. We proxy it as MCP tools.
- **zypi**: Sandbox runtime. We delegate sandboxed execution to them.

## How to reach us

Open an issue on [allen-munsch/yas-mcp](https://github.com/allen-munsch/yas-mcp)
or drop a file in this directory. We monitor both.
