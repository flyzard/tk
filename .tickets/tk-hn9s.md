+++
title = "Phase 6: tk mcp over Command via rmcp"
kind = "feature"
priority = "p3"
labels = ["phase-6"]
depends_on = ["tk-27ey", "tk-sx2j"]
created = 2026-10-15T08:55:59Z
status = { state = "doing", by = "human:joaofelizardo3", since = 2026-10-15T09:48:29Z }
+++

## Log
- 2026-10-15T09:27:06Z agent:claude: built tk mcp (rmcp 3.5, stdio, tokio only for this command): tools run/next/list/show/lint/guide over the same Command and execute
- 2026-10-15T09:27:07Z agent:claude: checked with a JSON-RPC client: create, claim with lease, show, log, conflict on stale version, finish; structured results always objects
- 2026-10-15T09:27:07Z agent:claude: not started via tk: it waits on tk-27ey (merge driver needs the real-git test run). Dynamic ID completion skipped: clap_complete still gates it behind unstable-dynamic
