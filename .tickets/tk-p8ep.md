+++
title = "Handoffs 1: typed log entries (EntryKind, LogKind, render+parse in tk-store/log.rs)"
kind = "feature"
priority = "p2"
labels = ["phase-7"]
depends_on = []
created = 2026-10-15T15:22:05Z
status = { state = "done", at = 2026-10-15T15:35:35Z, resolution = "completed" }
+++

## Context
First step of the handoff design (plan.md, "Handoffs between sessions"). Types first.

- tk-core: `EntryKind { Note, RuledOut, Decided, Next }` and `LogKind { Note, RuledOut, Decided }` (no `Next`: only a handoff writes it).
- tk-store/log.rs: render and parse `- <ts> <actor> <kind>: <text>` in one place; a note keeps `- <ts> <actor>: <text>`. Unparseable lines come back as raw text. Move the `format!` out of execute.rs.
- tk-app: `Command::Log { id, text, #[serde(default)] kind: LogKind }`, so old `tk apply`/MCP JSON stays valid.
- CLI: `tk log <id> --ruled-out | --decided`.

## Done when
- [ ] Proptest: every kind renders then parses back the same; an old untagged line parses as a note; a hand-written line as raw
- [ ] Old JSON `{"op":"log","id":..,"text":..}` still runs
- [ ] `cargo lint && cargo lint-core` pass

## Log
- 2026-10-15T15:35:35Z agent:claude: EntryKind/LogKind in tk-core; LogEntry (Display + parse), LogLine and log_lines in tk-store/log.rs; execute.rs now renders through LogEntry
- 2026-10-15T15:35:35Z agent:claude: tk log --ruled-out | --decided (conflicting flags); Command::Log gains #[serde(default)] kind, old JSON verified via tk apply; kind=next refused at parse
- 2026-10-15T15:35:35Z agent:claude decided: a tagged 'note:' line parses as a note and renders untagged; parsing is lenient, rendering canonical
- 2026-10-15T15:35:35Z agent:claude: tests: proptest log_entries_round_trip and old_and_hand_written_log_lines_parse in store/tests.rs; cargo lint, lint-core, workspace tests and the ignored git test pass
