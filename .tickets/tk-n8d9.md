+++
title = "Handoffs 3: tk handoff (NextSteps, AfterHandoff, one transaction; MCP handoff)"
kind = "feature"
priority = "p2"
labels = ["phase-7"]
depends_on = ["tk-p8ep"]
created = 2026-10-15T15:22:05Z
status = { state = "done", at = 2026-10-15T15:49:01Z, resolution = "completed" }
+++

## Context
See plan.md, "Handoffs between sessions".

- tk-core: `NextSteps` (nutype over `Vec<NonEmpty>`, non-empty, with `Explain`), `Handoff { next, ruled_out, decided }`, `AfterHandoff { Keep, Release, Block { reason } }`.
- tk-app: `Command::Handoff { id, if_version, handoff, then }` through `change`: append every entry with one `now`, then the move, in one closure. `keep_lease` already handles renew/release.
- CLI: `tk handoff <id> --next ... [--ruled-out ...] [--decided ...] [--release | --block <reason>]`. MCP `handoff` tool.

## Done when
- [ ] A refused move (e.g. `--release` on todo) writes nothing
- [ ] A handoff without `--next` is rejected while parsing
- [ ] A second handoff replaces the first one's Next in `tk brief`
- [ ] `cargo lint && cargo lint-core` pass

## Log
- 2026-10-15T15:49:01Z agent:claude: tk-core: NextSteps (nutype over Vec<NonEmpty>, Explain, schema), handoff.rs with Handoff::entries and AfterHandoff::into_move; Command::Handoff; MCP handoff tool; tk handoff with HandoffArgs
- 2026-10-15T15:49:01Z agent:claude decided: Log and Handoff share one execute helper, logged(): entries with one now, then an optional Move, through change(), so leases and --if-version come for free; it also kept execute under clippy's line limit
- 2026-10-15T15:49:01Z agent:claude decided: two handoffs in the same second read as one group (timestamps are whole seconds); noted in plan.md rather than adding a marker
- 2026-10-15T15:49:01Z agent:claude: checked in a scratch repo: supersede, refused --release writes nothing, missing --next refused by clap and by JSON, release+block conflict, lease kept/released, stale --if-version, MCP handoff+brief; lint, lint-core, all tests pass
