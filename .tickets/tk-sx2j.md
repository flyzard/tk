+++
title = "Phase 5: leases for tk next --claim and every start"
kind = "feature"
priority = "p2"
labels = ["phase-5"]
depends_on = []
created = 2026-10-15T08:55:59Z
status = { state = "done", at = 2026-10-15T09:20:45Z, resolution = "completed" }
+++

## Log
- 2026-10-15T09:20:45Z agent:claude: leases in $(git rev-parse --git-common-dir)/tk/leases, published by rename-noclobber; takeover of expired ones under a shared lock
- 2026-10-15T09:20:45Z agent:claude: start and next --claim acquire; writes by the holder renew; leaving doing releases; race test: exactly one winner in 50/50 rounds
- 2026-10-15T09:20:45Z agent:claude: checked in this repo: claimed ticket refused (exit 15), skipped by next, expired lease ignored
