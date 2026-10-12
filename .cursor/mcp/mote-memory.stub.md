---
tags: [mcp, stub]
status: planned
updated: 2026-10-07
---

# mote-memory MCP (stub)

Future custom MCP server for Mote long-term memory.

## Planned tools

- `memory_list` — list memory notes
- `memory_get` — read one memory by id
- `memory_upsert` — create/update a fact
- `memory_delete` — remove a fact

## MVP stand-in

Until the server exists, store memory as markdown under `vault/memory/` and use the `mote-vault` filesystem MCP.
