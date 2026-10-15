# tk

Tickets as Markdown files in git: a terminal UI for you, a CLI and an MCP server for your coding agents, and one shared core underneath.

```text
$ tk next --claim
app-k3x9 doing
$ tk log app-k3x9 "reproduced with a credit note dated the 2nd"
$ tk done app-k3x9
```

Each ticket is one Markdown file with TOML frontmatter in a `.tickets/` directory inside your repository. There is no server and no database: tickets are versioned, branched, reviewed and merged with your code. One binary handles every project you work on, together or one at a time.

## Why

Coding agents are good at working through a ticket queue, but most trackers are built for people clicking in a browser. tk is built for both at once:

- **Agents get a strict, scriptable interface.** Every command takes `--json`, and errors come back as `{"code", "message", "hint"}` with a distinct exit status per code. `tk schema` describes every command, and `tk mcp` serves the same commands over MCP.
- **People get a fast terminal UI** that live-reloads as agents work, plus their own editor for writing tickets.
- **Parallel agents don't collide.** Two agents in two git worktrees can't start the same ticket, a stale edit is refused instead of overwriting someone else's, and branches that touched the same ticket merge without hand-editing.

## Install

tk is a single Rust binary. With a Rust toolchain installed ([rustup](https://rustup.rs)):

```sh
cargo install --git https://github.com/flyzard/tk tk --locked
```

Or from a clone:

```sh
cargo install --path crates/tk --locked
```

## Quick start

```sh
cd ~/code/my-app
tk init --key app                          # creates .tickets/ and registers the repo
tk new "Export misses credit notes" --kind bug --priority p1 --label export
tk new "Fix the credit-note query" --priority p0
tk dep add app-k3x9 app-a1b2               # k3x9 waits until a1b2 is done
tk ls                                      # open tickets, most urgent first
tk next                                    # what to work on now
tk                                         # open the TUI
```

Commit `.tickets/` like any other source. tk changes files; you or your agent commit them.

## A ticket file

`.tickets/app-k3x9.md`:

```markdown
+++
title = "Export misses credit notes"
kind = "bug"
priority = "p1"
labels = ["export"]
depends_on = ["app-a1b2"]
created = 2026-10-14T09:12:00Z
status = { state = "doing", by = "agent:claude", since = 2026-10-14T10:00:00Z }
+++

## Context
Credit notes created after the 1st are missing from the export.

## Log
- 2026-10-14T10:05:00Z agent:claude: reproduced with a credit note dated the 2nd
```

- The ID lives only in the filename, so there is nothing to keep in sync.
- Edits are lossless. Changing one field rewrites one line and keeps your comments, spacing and key order. The status is a single line, so a status change is a one-line diff.
- The body is yours. tk only appends to `## Log`, and it creates that heading when it's missing.
- Unknown keys are errors, so a typo like `prority` is reported instead of ignored. `tk lint` points at the line.
- Nothing derived is stored. "Waiting on", "ready" and "last updated" are computed from the files and git.

## Status rules

A ticket is `todo`, `doing`, `blocked` or `done`. These are the only moves:

| Command | Move |
| --- | --- |
| `tk start <id>` | todo → doing; only when every dependency is done |
| `tk block <id> --reason "..."` | todo or doing → blocked |
| `tk unblock <id>` | blocked → todo |
| `tk release <id>` | doing → todo |
| `tk done <id> [--wont-do \| --duplicate-of <id>]` | todo, doing or blocked → done |
| `tk reopen <id>` | done → todo |

Any other move is refused with code `invalid_transition`.

## Commands

| Command | What it does |
| --- | --- |
| `tk` | Opens the TUI when stdout is a terminal |
| `tk init --key <key>` | Creates `.tickets/` here and adds the repository to your workspace |
| `tk new "Title" [--kind] [--priority] [--label] [--dep] [--body <text\|->]` | Creates a ticket |
| `tk ls [--all] [--status] [--label] [--workspace] [--fields id,state,title]` | Lists tickets, hiding done ones by default |
| `tk show <id>` | One ticket, with its version and what it is waiting on |
| `tk next [--claim]` | The top ready ticket; `--claim` also starts it |
| `tk edit <id> [--title] [--kind] [--priority] [--add-label] [--rm-label]` | Changes fields |
| `tk log <id> <text\|->` | Appends a line to `## Log` |
| `tk dep add <id> <dep>` / `tk dep rm <id> <dep>` | Changes dependencies; cycles are refused and named |
| `tk lint [--workspace]` | Validates every ticket file and the dependency graph; non-zero exit on problems |
| `tk apply` | Runs JSON commands from stdin, one per line |
| `tk schema` | Prints the JSON Schema of those commands |
| `tk guide` | Prints a short usage guide for `AGENTS.md`, `CLAUDE.md` or a skill |
| `tk mcp` | Serves the commands over MCP on stdio |
| `tk completions <shell>` | Prints shell completions |

Every command takes `--json`. Set who is acting with `--as agent:<name>` or the `TK_ACTOR` environment variable.

## For agents

Put the output of `tk guide` in your `AGENTS.md` or `CLAUDE.md`. The loop looks like this:

```sh
tk next --claim --json        # take the top ready ticket and start it
tk show <id> --json           # read it; note its "version"
tk log <id> "what you found"  # append to its log as you work
tk done <id>                  # or: tk block <id> --reason "..."
```

- **No silent overwrites.** Pass `--if-version <version>` from `tk show` to any write. If the ticket changed since, tk refuses with code `conflict` and a hint to re-read.
- **Batches.** `tk apply` runs one JSON command per line and stops at the first failure. `tk schema` describes exactly what it accepts.
- **Small output.** `tk ls --fields id,state,title --json` returns only the fields you ask for.

### MCP

`tk mcp` is a stdio MCP server exposing `run` (any command `tk schema` describes), `next`, `list`, `show`, `lint` and `guide`. Start it from inside your project with `TK_ACTOR=agent:<name>` in its environment, and register it with your MCP client as a stdio server.

## Running agents in parallel

**Leases.** `tk start` and `tk next --claim` take a lease in the git directory that all worktrees of a repository share. If another agent holds the ticket, the command is refused with code `claimed`, and `tk next` skips it. Leases last two hours, renew on every write by their holder, and end on release, block or done. They are never committed.

**Merging.** `tk init` routes ticket files to tk's merge driver in `.gitattributes`. Register the driver once per clone:

```sh
git config merge.tk.driver "tk merge-driver %O %A %B %P"
```

Frontmatter then merges field by field, labels and dependencies as sets, and `## Log` lines from both branches are kept in timestamp order. A conflict happens only when both sides change the same field differently. Then that one line gets markers, and either side alone is valid TOML.

## The TUI

Run `tk` with no arguments.

| Key | Action |
| --- | --- |
| `j` / `k` | Move down / up |
| `h` / `l` | Previous / next board column |
| `tab`, `1`–`4` | Board, list, ready, needs fixing |
| `enter` | Ticket detail, with the body rendered as Markdown |
| `n` | New ticket: a title, then your editor for the body |
| `e` | Edit in your editor; saved only if it parses and nobody changed it meanwhile |
| `s` / `b` / `d` / `r` | Start / block / done / reopen |
| `u` / `x` | Unblock / release |
| `p` | Cycle priority |
| `/` | Fuzzy filter |
| `g` | This project / all projects |
| `o` | Cycle the list's sort |
| `?` | Help |
| `q` | Quit |

The cursor stays on its ticket when files change underneath it. A ticket edited in your editor is never left invalid on disk: if your text doesn't parse, tk shows why and offers to reopen it.

## Configuration

`~/.config/tk/workspace.toml` (`$XDG_CONFIG_HOME` is respected):

```toml
projects = ["~/code/my-app", "~/code/ops"]   # tk init adds entries here
editor = ["code", "--wait"]                   # an array, run without a shell
actor = "human:you"                           # agents override with TK_ACTOR
```

Without an `editor`, tk uses `$VISUAL`, then `$EDITOR`, then `vi`. A workspace entry that no longer exists is skipped with a warning.

## How it's built

tk is written so that the compiler enforces as many of its rules as possible:

- **An invalid ticket can't be constructed.** Text is parsed into validated types once, at the edge, and nothing re-checks it inside.
- **An illegal status change can't be expressed.** The data each state needs lives inside that state. A ticket can only be started through a value proving its dependencies are done, and every move goes through one exhaustive `match`.
- **A write can't skip its checks.** Writes only happen inside a locked transaction, from a ticket that was read first. A file is checked to read back identically before it replaces the old one, atomically.

The workspace has four crates whose dependencies only point inward:

| Crate | Role |
| --- | --- |
| `tk-core` | Domain rules as types. `#![no_std]`: it can't touch files, the clock or the environment |
| `tk-store` | File format, lossless edits, transactions, atomic writes, leases, merge driver |
| `tk-app` | One `Command` enum and one `execute` function, used by every interface |
| `tk` | The binary: CLI, TUI and MCP server |

The full design is in [`plan.md`](plan.md).

## Development

```sh
cargo lint          # clippy on the workspace, every lint on, warnings are errors
cargo lint-core     # tk-core built for a bare-metal target, proving it stays no_std
cargo test --workspace
cargo test -p tk -- --ignored   # the merge test that runs real git in a temp directory
bacon               # clippy on every save
```

The toolchain is pinned in `rust-toolchain.toml` (minimum supported Rust: 1.89). Use rustup's `cargo`, not a system or Homebrew one, so the pin and the bare-metal target apply. Contribution rules are in [`CLAUDE.md`](CLAUDE.md). The short version: change types first, never write `_ =>` on a domain enum, no `unwrap`, and never `#[allow]` a lint.

tk tracks its own work in `.tickets/`.

## Status

All six phases of the plan are built: core types, storage, CLI, TUI, leases with the merge driver, and MCP. Still open:

- `tk archive` for old done tickets
- dynamic ID completion in the shell, once clap supports it on stable

## License

TBD
