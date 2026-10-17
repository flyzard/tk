+++
title = "Phase 6: dynamic ticket ID completion once clap_complete stabilizes it"
kind = "feature"
priority = "p3"
labels = ["phase-6"]
depends_on = []
created = 2026-10-15T20:27:08Z
status = { state = "blocked", reason = "upstream: clap_complete 4.6.11 keeps dynamic completion behind unstable-dynamic", since = 2026-10-15T20:27:12Z }
+++

Per plan.md phase 6: complete ticket IDs (with titles) in the shell for `start`, `done`, `show`, `log`, `dep` and the other commands that take an ID.

Blocked upstream: as of clap_complete 4.6.11, dynamic completion (`ArgValueCandidates`, `CompleteEnv`) is still behind the `unstable-dynamic` feature. Don't enable an unstable feature; check whether a newer release stabilizes it first.
