+++
title = "Refs: a ticket's map of where to look, kept by every session"
kind = "feature"
priority = "p1"
labels = ["agents"]
depends_on = []
created = 2026-10-17T10:32:54Z
status = { state = "done", at = 2026-10-17T10:42:56Z, resolution = "completed" }
+++

Sessions pay to orient (find the files, the plan section, the earlier decision), then lose the result. A ref is a pointer plus a why, logged as a typed entry: `ref: <target> | <why>`, dropped with `unref:`. Targets: a repo-relative path with an optional #anchor, a ticket ID, or a URL. The brief folds them into Read first and derives each anchor's current lines (the section for a Markdown heading), flagging missing ones. Done dependencies' decided entries go inline in the brief. tk lint checks refs of open tickets. plan.md:643 said to add file entries back if they were missed; they are.

## Log
- 2026-10-17T10:42:53Z agent:claude decided: LogEntry carries a typed Entry (Ref/Unref hold a Reference): a ref line whose text isn't '<target> | <why>' stays a raw line, like any hand-written one
- 2026-10-17T10:42:53Z agent:claude decided: own-project tickets are read through the caller's Tx (brief, lint): the store lock isn't reentrant, so opening a second transaction on it would block; other projects open their own
- 2026-10-17T10:42:53Z agent:claude ref: plan.md#Refs: where to look | the design and what was left out on purpose
- 2026-10-17T10:42:53Z agent:claude ref: crates/tk-app/src/refs.rs#fn locate_anchor | how an anchor becomes a line or a Markdown section
- 2026-10-17T10:42:53Z agent:claude ref: crates/tk-app/src/brief.rs#fn brief_of | where refs, linked tickets and upstream decisions meet
