+++
title = "Friendlier CLI messages for nutype errors (e.g. 'Title failed the predicate test')"
kind = "chore"
priority = "p3"
labels = ["cli"]
depends_on = []
created = 2026-10-15T08:55:59Z
status = { state = "done", at = 2026-10-15T09:29:24Z, resolution = "completed" }
+++

## Log
- 2026-10-15T09:20:20Z agent:tester: lease check
- 2026-10-15T09:29:24Z agent:claude: Explain trait in tk-core: one sentence per nutype rule, exhaustive over the generated error enums; used by CLI flags, log text and TUI prompts
- 2026-10-15T09:29:24Z agent:claude: still nutype's wording on serde paths (tk apply, MCP, ticket files): serde calls nutype's own Display
