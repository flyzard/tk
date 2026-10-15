+++
title = "Phase 5: tk merge-driver (field-wise frontmatter, union of log lines)"
kind = "feature"
priority = "p2"
labels = ["phase-5"]
depends_on = []
created = 2026-10-15T08:55:59Z
status = { state = "done", at = 2026-10-15T09:48:22Z, resolution = "completed" }
+++

## Log
- 2026-10-15T09:20:45Z agent:claude: pure merge: fields 3-way, labels/depends_on as sets, log lines unioned by timestamp; conflicts mark one line
- 2026-10-15T09:20:45Z agent:claude: tk init writes .gitattributes; driver registration (git config) is printed, not run
- 2026-10-15T09:20:45Z agent:claude: open: run the ignored real-git test (cargo test -p tk -- --ignored) and register the driver in this clone
