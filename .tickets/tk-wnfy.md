+++
title = "Handoffs 2: tk brief (derived starting context; tk next --claim --brief; MCP brief)"
kind = "feature"
priority = "p2"
labels = ["phase-7"]
depends_on = ["tk-p8ep"]
created = 2026-10-15T15:22:05Z
status = { state = "done", at = 2026-10-15T15:45:56Z, resolution = "completed" }
+++

## Context
`Brief` query in tk-app, never persisted, built by exhaustive destructuring over `EntryKind`. See plan.md, "Handoffs between sessions".

- Header, waiting on (omitted when empty), prose (`without_log`), Next (latest group of `next` entries, with age and "N notes since"), Ruled out / Decided (newest first, cap 10), Notes (last 5), with "N older: tk show".
- `tk brief [<id>] [--json]`: no ID means tickets this checkout has `doing` by the current actor.
- `tk next --claim --brief`; MCP `brief` tool.

## Done when
- [ ] A ticket with a long log briefs to the sections above, capped
- [ ] `tk brief` with no ID finds this worktree's doing ticket
- [ ] `cargo lint && cargo lint-core` pass

## Log
- 2026-10-15T15:45:56Z agent:claude: tk-app/src/brief.rs: Brief, brief(id), my_briefs (doing by actor in the current project); LastHandoff = newest group of next entries, with age_minutes and entries_since; ruled_out/decided newest first capped 10, notes last 5; raw lines count as notes
- 2026-10-15T15:45:56Z agent:claude: CLI: tk brief [<id>], tk next [--claim] --brief; text output in output.rs; MCP brief tool returns {briefs:[...]}; tk-store now exports without_log
- 2026-10-15T15:45:56Z agent:claude decided: named the brief's struct LastHandoff so tk-n8d9 can use Handoff in tk-core
- 2026-10-15T15:45:56Z agent:claude decided: no new tests: brief is derived from log_lines, already covered by the round-trip tests; checked by hand in a scratch repo (supersede, stale count, caps, raw line, waiting on, MCP)
