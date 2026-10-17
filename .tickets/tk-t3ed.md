+++
title = "Phase 6: tk archive moves old done tickets into .tickets/archive/"
kind = "feature"
priority = "p3"
labels = ["phase-6"]
depends_on = []
created = 2026-10-15T20:27:08Z
status = { state = "doing", by = "human:joaofelizardo3", since = 2026-10-15T20:27:21Z }
+++

Per plan.md (phase 6 and Open decisions: "Done tickets"): `tk archive` moves old done tickets into `.tickets/archive/`.

To settle when starting:
- Which tickets count as "old" (done before a cutoff, e.g. `--before <date>`, or all done).
- Dependencies on an archived ticket must still count as done, so `Ready`, `show` and `lint` read the archive too, or archiving refuses tickets that open tickets still depend on.
- `ls --all`, `show <id>` and `brief` should still find archived tickets.
- The move goes through `Store::transaction`, like every other change.
- Merge driver: a ticket edited on one branch and archived on another becomes a rename/modify conflict. Note how git handles this.
