+++
title = "Fix the 10 code review findings (worktrees, leases, merge, robustness)"
kind = "bug"
priority = "p1"
labels = ["review"]
depends_on = []
created = 2026-10-15T09:37:32Z
status = { state = "done", at = 2026-10-15T09:43:09Z, resolution = "completed" }
+++

## Log
- 2026-10-15T09:43:09Z agent:claude: fixed: worktrees of one repo share a key (git common dir), stale workspace entries warn; checked with two checkouts in one git dir
- 2026-10-15T09:43:09Z agent:claude: fixed: failed start/claim releases its lease; lease upkeep after a save only warns; all lease changes under the shared lock, takeover never overwrites
- 2026-10-15T09:43:09Z agent:claude: fixed: ValidGraph::build_usable keeps unrelated work going past broken tickets; merge ignores trailing whitespace in prose and never drops theirs on a missing key
- 2026-10-15T09:43:09Z agent:claude: fixed: eol=lf route + CRLF parsing, duplicate-of validated, TUI reads each project once, leases cached per store. 5 new tests
