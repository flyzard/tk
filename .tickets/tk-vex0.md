+++
title = "Handoffs 4: tk guide, README and SessionStart hook recipe"
kind = "chore"
priority = "p3"
labels = ["phase-7"]
depends_on = ["tk-n8d9", "tk-wnfy"]
created = 2026-10-15T15:22:05Z
status = { state = "done", at = 2026-10-15T15:50:16Z, resolution = "completed" }
+++

## Context
- `tk guide`: the loop becomes claim -> brief -> work -> handoff.
- README: commands table, the log line format, and "For agents".
- Document a Claude Code SessionStart hook (matcher `startup|compact`) running `tk brief`. Say plainly that a hook can't force a handoff.

## Done when
- [ ] A fresh agent session picks up a ticket from `tk brief` alone (phase 7 done-when in plan.md)

## Log
- 2026-10-15T15:50:16Z agent:claude: guide.md: loop is claim+brief -> log (--ruled-out/--decided) -> done or handoff; SessionStart hook recipe (startup|compact, TK_ACTOR in env), validated as JSON; MCP tool list and server instructions mention brief/handoff
- 2026-10-15T15:50:16Z agent:claude: README: commands table, For agents loop, new 'Handoffs and briefs' section, status says seven phases; plan.md tk next row gains --brief
- 2026-10-15T15:50:16Z agent:claude: not verified yet: the phase-7 done-when (a fresh agent session picks a ticket up from tk brief alone) needs a real new session with the hook installed
- 2026-10-15T15:52:54Z agent:claude: verified with a fresh subagent given only tk guide + a hook-style brief (scratch repo, Decimal rounding bug): followed decided, skipped ruled-out, no re-investigation, logged and closed in 2 tool calls. Synthetic task; it checks the mechanism, not real handoff quality
- 2026-10-15T15:52:54Z agent:claude: open question from that run: tk done leaves 'Done when' checkboxes unticked, and the guide doesn't say whether agents should tick them
