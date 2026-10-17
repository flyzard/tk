# tk — Type-Driven Ticket Tracker Plan

Oct 14, 2026 · @Avenida do código

Revised on 14 Oct 2026 after a design and library review; what changed and why is in Review, 14 Oct.

## What tk is

tk is one Rust binary that tracks tickets for all your products as Markdown files in git: a TUI for you, a CLI for agents, one shared core.

- One ticket is one Markdown file with TOML frontmatter, stored in `.tickets/` inside each product repo.
- A workspace config lists every repo, so each view works per product or across all of them.
- `tk` with no arguments opens the TUI. Subcommands are the agent interface, all with `--json`.
- Both interfaces turn input into the same typed `Command`, run by one function, so they can't behave differently.

**The bar for done:** if it compiles and clippy passes, an invalid ticket can't be written, an illegal status change can't be expressed, and an agent can't silently overwrite someone else's edit.

**Not in v1:** a server, sync beyond git, a web board, automatic commits (tk changes files; you or the agent commit), cross-project dependencies, and MCP. MCP comes later as a thin layer over `Command`.

## Compiler-driven principles

Every rule is a type or an exhaustive match, so a change that breaks a rule fails to compile instead of failing a test.

1. **Parse, don't validate.** Text becomes a domain type once, at the edge. Inside the program every `Title` is already valid, and nothing re-checks it.
2. **Make illegal states unrepresentable.** Data that only exists in one state lives inside that enum variant: `Done` carries its resolution, `Todo` carries nothing.
3. **Declarative newtypes.** Every text type is a nutype with its rules declared (`sanitize(trim)`, `len_char_max = 120`). Construction and serde deserialization both run the rules; there is no other way in.
4. **Exhaustive everything.** No `_ =>` arms on domain enums and no `..` when destructuring for rendering or JSON. A new variant or field becomes a compiler-written to-do list.
5. **Proofs wrap what they prove.** A value that proves a check passed contains the thing checked. `ValidGraph::ready` takes a ticket by value and returns a `Ready` ticket, so a readiness check can't be applied to a different ticket.
6. **Ownership as protocol.** Steps that must happen once consume `self`. An `Edited` ticket is moved into `save`, and `save` exists only inside a transaction.
7. **Pure core, enforced by the compiler.** tk-core is `#![no_std]` with `alloc`, so there is no `std::fs`, `std::env` or clock to call. Time and random bytes arrive as arguments.
8. **Store facts, derive views.** Nothing computable is persisted. Readiness, "blocked by dependencies" and last-updated are derived, so they can't go stale.
9. **Every exception is justified and expires.** `#[allow]` is denied. `#[expect(lint, reason = "...")]` is used instead, and it becomes an error once the lint stops firing.

The lints that hold this in place:

```toml
# Cargo.toml (workspace root); each crate adds: [lints] workspace = true
[workspace.lints.rust]
unsafe_code = "forbid"
unused_must_use = "deny"

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
unreachable = "deny"
indexing_slicing = "deny"
string_slice = "deny"                  # byte slicing can panic inside "ção"
wildcard_enum_match_arm = "deny"
match_wildcard_for_single_variants = "deny"
allow_attributes = "deny"              # use #[expect] instead
allow_attributes_without_reason = "deny"
let_underscore_must_use = "deny"
dbg_macro = "deny"
todo = "warn"                          # allowed while sketching signatures; clear before merging
```

```rust
// crates/tk-core/src/lib.rs: the core cannot reach files, environment or clock
#![cfg_attr(not(test), no_std)]
extern crate alloc;
```

```toml
# crates/tk-core/Cargo.toml: every dependency without std
[dependencies]
bon = { workspace = true, default-features = false, features = ["alloc"] }
jiff = { workspace = true, default-features = false, features = ["alloc", "serde"] }
nutype = { workspace = true, default-features = false, features = ["serde"] }
serde = { workspace = true, default-features = false, features = ["alloc", "derive"] }
strum = { workspace = true, default-features = false, features = ["derive"] }
thiserror = { workspace = true, default-features = false }
```

The daily loop: change a type, run `bacon clippy`, fix every error it lists. In VS Code Insiders, set `rust-analyzer.check.command` to `clippy` so the same errors appear inline. The compiler is the to-do list; a test is written only where types can't reach (see the test section below).

## Architecture

tk is a Cargo workspace of four crates whose dependencies only point inward, so the compiler enforces the architecture: tk-core can't import tk-store or touch the disk.

&#91;embedded content: crate dependencies · 4 crates, 1 entry point\]

The TUI, the CLI and later MCP all parse their input into the same `Command`, so the interfaces can't drift apart.

```text
tk/
  Cargo.toml          # [workspace], [workspace.lints], [workspace.dependencies]
  rust-toolchain.toml # channel = "stable"
  crates/
    tk-core/          # domain rules; #![no_std] + alloc
    tk-store/         # file format, transactions, versions, atomic writes, leases, merge driver
    tk-app/           # Command, execute, Outcome, AppError, JSON views
    tk/               # the binary: clap CLI + ratatui TUI
  .tickets/           # tk tracks its own work
```

Rust edition 2024, minimum Rust 1.89 (for `std::fs::File::lock`; frizbee needs it too), with `rust-toolchain.toml` pinned to stable. Release dates and the alternatives checked are in the Review tab.

| Crate | Used for | Where |
| --- | --- | --- |
| clap 4.6, clap\_complete 4.6 | CLI parsing and shell completions | tk |
| serde, serde\_json | JSON in and out | all |
| toml 1.1, toml\_edit 0.25 | Frontmatter parsing and lossless edits | tk-store |
| nutype 0.8 | Validated newtypes | tk-core |
| strum 0.28 | Enum names, parsing and iteration | tk-core |
| bon 3.10 | Compile-checked builders | tk-core |
| jiff 0.2 | Timestamps | tk-core |
| thiserror 2 | Error enums | all |
| blake3 1.8 | File version hashes | tk-store |
| tempfile 3.27 | Temp file, then atomic rename | tk-store |
| camino 1.2, fs-err 3.3 | UTF-8 paths; I/O errors that name the file | tk-store, tk |
| etcetera 0.11 | Locating `~/.config/tk` | tk-store |
| pulldown-cmark 0.13 | Finding `## Log` in the body | tk-store |
| getrandom 0.4 | Random ID bytes | tk-app |
| schemars 1.2 | JSON Schema for `tk schema` | tk-app |
| annotate-snippets 0.12 | `tk lint` errors that point at the line | tk-app |
| ratatui 0.30, crossterm 0.29 | Terminal UI | tk |
| tui-input 0.15, tui-markdown 0.3 | Input boxes; Markdown in Detail view | tk |
| notify 8.2, notify-debouncer-full 0.7 | Live reload | tk |
| frizbee 0.13 | Fuzzy filter | tk |
| proptest 1.11 (dev) | The ticket round-trip test | tk-store |
| rmcp 3.5 (phase 6) | MCP server | tk |

## Ticket file format

Each ticket is `<id>.md`: TOML frontmatter between `+++` lines, then a free Markdown body.

```text
my-product/
  .tickets/
    project.toml      # key = "app", name = "My App", format = 1
    app-k3x9.md
    app-a1b2.md
    .lock             # gitignored
~/.config/tk/workspace.toml
```

```toml
# ~/.config/tk/workspace.toml (found via etcetera, so XDG_CONFIG_HOME is respected)
projects = ["~/code/my-app", "~/code/ops"]   # keys come from each project.toml
editor = ["code-insiders", "--wait"]          # an array, so no shell parsing
actor = "human:muxa"                          # agents override with TK_ACTOR
```

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

## Done when
- [ ] Credit notes appear in the export

## Log
- 2026-10-14T10:05:00Z agent:claude: reproduced with a credit note dated the 2nd
```

- **The ID lives only in the filename.** There is nothing to keep in sync. The prefix must match the key in `project.toml`, or the file is reported invalid.
- **No `updated` field.** Git already knows, and a field people forget to bump would lie.
- **TOML, not YAML.** [toml\_edit](https://docs.rs/toml_edit) changes one key while keeping comments, spacing and order, so a status change is a one-line diff.
- **Status sits on one line** as an inline table. When two branches change it, git shows one conflicting line, and either side is valid TOML.
- **Logs merge cleanly.** `.gitattributes` routes `.tickets/*.md merge=tk` to `tk merge-driver`, which merges frontmatter field by field and unions `## Log` lines by timestamp (phase 5).
- **Unknown keys are errors** (`deny_unknown_fields`), so a typo like `prority` is caught instead of ignored.
- **The body is yours.** tk only appends under `## Log`, creating the heading if it's missing. It finds the heading with pulldown-cmark, so a heading inside a code fence never matches.
- **The format is versioned** with `format = 1` in `project.toml`, checked on every load.

## Domain types (tk-core)

tk-core holds every rule about tickets as a type. It has no I/O, and it's the first crate to build.

```rust
// ids.rs: the only way to get a TicketId is to parse one
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ProjectKey(String);   // 2-8 lowercase ASCII letters

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Suffix([u8; 4]);      // Crockford base32, lowercase

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TicketId { project: ProjectKey, suffix: Suffix }

impl FromStr for TicketId { type Err = IdError; /* "app-k3x9" */ }
impl fmt::Display for TicketId { /* exact inverse of FromStr */ }

impl Suffix {
    /// The shell passes in random bytes, so the core stays deterministic.
    pub fn from_entropy(bytes: [u8; 4]) -> Self { /* map each byte onto the alphabet */ }
}
```

| Type | Guarantees | Shape |
| --- | --- | --- |
| `Title` | Trimmed, 1-120 characters, one line | nutype over `String` |
| `Label` | Kebab-case, 1-32 characters | nutype; `Labels` is a `BTreeSet<Label>`: sorted, no duplicates |
| `Priority` | One of four levels | Enum `P0`-`P3` with strum's `EnumString`, `VariantNames`, `EnumIter` |
| `Kind` | Bug, feature, chore or spike | Enum, same strum derives |
| `Actor` | `human:<name>` or `agent:<name>` | Enum over a nutype `Name` |
| `NonEmpty` | At least one non-blank character | nutype; block reasons and log text |
| `Body` | Any Markdown | Opaque, plus `append_log` |
| `Timestamp` | A UTC instant | `jiff::Timestamp`, always passed in |

Every text type is declared once with nutype. The macro generates the only constructor, a typed error enum, and serde support that runs the same rules:

```rust
#[nutype(
    sanitize(trim),
    validate(not_empty, len_char_max = 120, predicate = |s: &str| !s.contains('\n')),
    derive(Debug, Clone, PartialEq, Eq, AsRef, Display, Serialize, Deserialize),
)]
pub struct Title(String);

// Title::try_new("  Fix export  ") gives "Fix export".
// Failures are a generated enum: TitleError::NotEmptyViolated,
// TitleError::LenCharMaxViolated, TitleError::PredicateViolated.
```

The ticket itself:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ticket {
    id: TicketId,                    // from the filename
    pub title: Title,
    pub kind: Kind,
    pub priority: Priority,
    pub labels: Labels,
    depends_on: BTreeSet<TicketId>,  // changes only through ValidGraph
    created: Timestamp,
    status: Status,                  // changes only through transitions
    pub body: Body,
}

impl Ticket {
    /// New tickets always start as Todo.
    pub fn new(spec: NewTicket, id: TicketId, now: Timestamp) -> Self;
    /// The other way in: Frontmatter can only come from deserializing a file.
    pub fn from_stored(id: TicketId, front: Frontmatter, body: Body) -> Self;
    pub fn transition(self, m: Move, now: Timestamp) -> Result<Self, InvalidTransition>;
    pub fn remove_dep(self, dep: &TicketId) -> Self;
    pub fn status(&self) -> &Status;
    pub fn depends_on(&self) -> &BTreeSet<TicketId>;
    /// Every field, borrowed, for rendering; destructured without `..`.
    pub fn view(&self) -> TicketView<'_>;
}
```

Status and dependencies are private, so code outside tk-core can change them only through transitions and the graph. The other fields are public because each is valid on its own. `NewTicket` derives bon's `Builder`, so leaving out the title is a compile error.

Rules that span tickets live in `ValidGraph`. Its checks take the ticket by value and hand it back wrapped, so a passed check can't be applied to a different ticket:

```rust
pub struct ValidGraph { /* private: statuses and edges of one project */ }

impl ValidGraph {
    /// Fails on missing dependencies, self-dependencies and cycles.
    pub fn build(tickets: &BTreeMap<TicketId, Ticket>) -> Result<Self, GraphError>;
    /// Todo tickets whose dependencies are all Done, by (priority, created).
    pub fn ready_ids(&self) -> impl Iterator<Item = &TicketId>;
    /// Takes the ticket; wraps it only if it is Todo and its dependencies are Done.
    pub fn ready(&self, t: Ticket) -> Result<Ready, (Ticket, NotReady)>;
    /// Adds the dependency only if it creates no cycle.
    pub fn add_dep(&self, t: Ticket, dep: TicketId) -> Result<Ticket, (Ticket, WouldCycle)>;
}

/// Private field: ValidGraph::ready is the only way to make one.
pub struct Ready(Ticket);
```

Dependencies stay within one project in v1, so one project lock covers every check a command makes.

## Status state machine

Status is an enum whose variants carry only the data valid in that state, and every move between states goes through one exhaustive match.

&#91;embedded content: ticket status · 4 states, 7 moves\]

Starting needs a `Ready` ticket, which only `ValidGraph::ready` produces, so a ticket can't start while its dependencies are open. The public API has no other way to say "start": `Ticket::transition` takes a `Move`, and `Move` has no start variant. "Blocked by dependencies" is never stored; it's derived from the graph.

```rust
pub enum Status {
    Todo,
    Doing   { by: Actor, since: Timestamp },
    Blocked { reason: NonEmpty, since: Timestamp },
    Done    { at: Timestamp, resolution: Resolution },
}

pub enum Resolution { Completed, WontDo, Duplicate(TicketId) }

/// What callers can ask for. There is deliberately no Start here.
pub enum Move {
    Block   { reason: NonEmpty },
    Unblock,
    Release,
    Finish  { resolution: Resolution },
    Reopen,
}

/// Every transition, Start included. Private to tk-core.
enum Transition { Start { by: Actor }, Move(Move) }

impl Ticket {
    pub fn transition(self, m: Move, now: Timestamp) -> Result<Self, InvalidTransition> {
        self.apply(Transition::Move(m), now)
    }
}

impl Ready {
    /// The only way to start a ticket.
    pub fn start(self, by: Actor, now: Timestamp) -> Result<Ticket, InvalidTransition> {
        self.0.apply(Transition::Start { by }, now)
    }
}

/// The one place transitions are defined.
fn next(current: &Status, t: Transition, now: Timestamp) -> Result<Status, InvalidTransition> {
    use Move as M;
    use Status as S;
    use Transition as T;
    match (current, t) {
        (S::Todo, T::Start { by }) => Ok(S::Doing { by, since: now }),
        (S::Todo | S::Doing { .. }, T::Move(M::Block { reason })) => {
            Ok(S::Blocked { reason, since: now })
        }
        (S::Blocked { .. }, T::Move(M::Unblock)) | (S::Doing { .. }, T::Move(M::Release)) => Ok(S::Todo),
        (S::Todo | S::Doing { .. } | S::Blocked { .. }, T::Move(M::Finish { resolution })) => {
            Ok(S::Done { at: now, resolution })
        }
        (S::Done { .. }, T::Move(M::Reopen)) => Ok(S::Todo),
        // Every invalid pair is named, never `_`: a new state, move or transition
        // fails to compile until each pairing is decided.
        (S::Doing { .. } | S::Blocked { .. } | S::Done { .. }, t @ T::Start { .. })
        | (S::Todo, t @ T::Move(M::Unblock | M::Release | M::Reopen))
        | (S::Doing { .. }, t @ T::Move(M::Unblock | M::Reopen))
        | (S::Blocked { .. }, t @ T::Move(M::Block { .. } | M::Release | M::Reopen))
        | (S::Done { .. }, t @ T::Move(M::Block { .. } | M::Unblock | M::Release | M::Finish { .. })) => {
            Err(InvalidTransition { from: current.name(), attempted: t.name() })
        }
    }
}
```

Adding a `Cancelled` state, for example, produces compile errors listing every missing pair until each one is decided.

## Store: reliable writes (tk-store)

The store makes four mistakes impossible to express: writing outside a transaction, writing without reading first, writing unvalidated data, and saving the same edit twice.

```rust
/// Hash of the file's bytes when it was read. No public constructor:
/// the only way to get a Version is to read the file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Version(blake3::Hash);

impl Store {
    /// Holds the project lock for the whole closure. `Tx` can't outlive it,
    /// so every read-check-write sequence runs under one lock.
    pub fn transaction<R, E: From<StoreError>>(
        &self,
        f: impl for<'t> FnOnce(&mut Tx<'t>) -> Result<R, E>,
    ) -> Result<R, E>;
}

impl Tx<'_> {
    pub fn snapshot(&self) -> Result<Snapshot, LoadError>;
    pub fn load(&self, id: &TicketId) -> Result<Loaded, LoadError>;
    pub fn create(&mut self, ticket: Ticket) -> Result<Saved, CreateError>;
    pub fn save(&mut self, edit: Edited) -> Result<Saved, SaveError>;   // consumes the edit
}

pub struct Loaded { ticket: Ticket, doc: TicketDoc, version: Version }
pub struct Edited { before: Ticket, after: Ticket, doc: TicketDoc, base: Version }

#[must_use]
pub struct Saved { pub id: TicketId, pub version: Version }

impl Loaded {
    pub fn ticket(&self) -> &Ticket;
    pub fn check_version(&self, expected: Option<&VersionHex>) -> Result<(), Conflict>;
    /// The only way to get an Edited. The closure owns the ticket, so core's
    /// consuming methods (transition, ready + start, add_dep) fit directly.
    pub fn update<E>(self, f: impl FnOnce(Ticket) -> Result<Ticket, E>) -> Result<Edited, UpdateError<E>>;
}

// Starting a ticket, end to end (AppError implements From for each error type):
store.transaction(|tx| -> Result<Saved, AppError> {
    let graph = ValidGraph::build(tx.snapshot()?.tickets())?;
    let loaded = tx.load(&id)?;
    loaded.check_version(if_version.as_ref())?;
    let edited = loaded.update(|t| -> Result<Ticket, AppError> {
        let ready = graph.ready(t).map_err(|(_, not_ready)| not_ready)?;
        Ok(ready.start(actor, now)?)
    })?;
    Ok(tx.save(edited)?)
})
```

`TicketDoc` wraps the `toml_edit` document and the body. `save` compares `before` and `after` field by field, destructured without `..`, and writes only the changed keys into the document. `update` rejects a closure that hands back a different ticket's ID.

A transaction and its `save` run these steps:

1. `Store::transaction` takes an exclusive lock on `.tickets/.lock` with [`std::fs::File::lock`](https://doc.rust-lang.org/stable/std/fs/struct.File.html#method.lock), stable since Rust 1.89, and holds it until the closure returns.
2. `save` hashes the file on disk again. If it differs from `base`, it returns `SaveError::Conflict` with both versions.
3. It renders the document and parses the bytes back. If they don't produce the `after` ticket, it refuses to write: a file tk can't read never reaches disk.
4. It writes a temp file in the same directory with tempfile, syncs it, renames it over the original with `persist`, then syncs the directory. A crash leaves the old file or the new one, never a mix.
5. It returns `Saved` with the new `Version`. The lock is released when the transaction ends.

`create` uses tempfile's `persist_noclobber`, which is atomic and fails if the file already exists. It can never overwrite a ticket or leave a half-written one, and a suffix collision just retries with new random bytes.

Agents get the same protection. `tk show --json` returns the `version`, and every write command accepts `--if-version`, checked in step 2.

Loading a project never fails because one file is broken. `Snapshot { tickets: BTreeMap<TicketId, Loaded>, invalid: Vec<InvalidFile> }` keeps both. The TUI lists invalid files in a "Needs fixing" panel, and `tk lint` exits non-zero on them.

## Commands, CLI and errors (tk-app)

Every interface is a parser into one `Command` enum, and one `execute` function runs it. CLI flags, TUI keys, JSON lines from `tk apply` and a later MCP server can't drift apart.

```rust
#[derive(Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    New { project: ProjectKey, title: Title, kind: Kind, priority: Priority,
          labels: Labels, depends_on: BTreeSet<TicketId>, body: Body },
    Edit { id: TicketId, if_version: Option<VersionHex>, changes: Vec<FieldChange> },
    Start { id: TicketId, if_version: Option<VersionHex> },
    Block { id: TicketId, if_version: Option<VersionHex>, reason: NonEmpty },
    Unblock { id: TicketId, if_version: Option<VersionHex> },
    Release { id: TicketId, if_version: Option<VersionHex> },
    Finish { id: TicketId, if_version: Option<VersionHex>, resolution: Resolution },
    Reopen { id: TicketId, if_version: Option<VersionHex> },
    Log { id: TicketId, text: NonEmpty },
    Next { project: Option<ProjectKey>, claim: bool },
}

/// Ctx carries the store, workspace, actor, current time and an entropy source.
pub fn execute(cmd: Command, ctx: &mut Ctx) -> Result<Outcome, AppError>;
```

The newtypes deserialize through nutype's rules, so a bad `tk apply` line is rejected while it's parsed, naming the field that failed. `tk schema` prints the JSON Schema that schemars generates from these same types. nutype's schemars support targets schemars 0.8 only, so newtype fields in `Command` carry `#[schemars(with = "String")]`.

| Command | What it does |
| --- | --- |
| `tk` | Opens the TUI when stdout is a terminal |
| `tk init --key app` | Creates `.tickets/project.toml` and adds the repo to the workspace |
| `tk new "Title" [--kind] [--priority] [--label] [--dep] [--ref]` | Creates a ticket; the body comes from stdin when piped |
| `tk ls [--all] [--status] [--label] [--fields]` | Lists tickets, hiding done ones by default |
| `tk show <id>` | One ticket with its version and its dependencies' status |
| `tk next [--claim] [--brief]` | The top ready ticket; `--claim` also starts it, `--brief` prints its brief |
| `tk start`, `block`, `unblock`, `release`, `done`, `reopen` | Status transitions on one ticket |
| `tk edit <id> [--title] [--priority] [--add-label]` | Field changes |
| `tk log <id> [text or -] [--ruled-out \| --decided]` | Appends to `## Log`; `-` reads stdin |
| `tk ref <id> <target> --why ... [--drop]` | Points the ticket at a place worth reading, or drops a ref; refused if the target isn't there |
| `tk handoff <id> --next ... [--ruled-out] [--decided] [--ref] [--release \| --block]` | Ends a session: typed log entries and an optional move, in one transaction |
| `tk brief [<id>]` | The compact starting context for a ticket; no ID means your `doing` ticket here |
| `tk dep add` / `tk dep rm` | Dependency changes, cycle-checked |
| `tk lint [--all]` | Validates every file; use it as a pre-commit hook |
| `tk apply` | Runs JSON lines of `Command` from stdin |
| `tk schema` | Prints the JSON Schema of `Command` |
| `tk guide` | Prints the short usage text for AGENTS.md or a skill |
| `tk completions <shell>` | Prints shell completions (clap\_complete) |
| `tk merge-driver` | Called by git to merge ticket files (phase 5) |

Every command takes `--json`, and `--as agent:claude` (or `TK_ACTOR`) sets the actor. Enum flags use clap's `PossibleValuesParser` built from strum's `VariantNames`, and IDs use a `TicketId` value parser, so bad values are rejected before `execute` runs, with suggestions. clap stays out of tk-core.

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("ticket {0} not found")]
    NotFound(TicketId),
    #[error("cannot {attempted} a ticket that is {from}")]
    InvalidTransition { from: StateName, attempted: TransitionName },
    #[error("ticket {id} changed since you read it")]
    Conflict { id: TicketId, expected: Version, found: Version },
    #[error("invalid input")]
    Invalid(Vec<FieldError>),
    #[error("that dependency would create a cycle")]
    WouldCycle { path: Vec<TicketId> },
    #[error("ticket is waiting on other tickets")]
    NotReady { waiting_on: Vec<TicketId> },
    #[error("claimed by {by} until {until}")]
    Claimed { by: Actor, until: Timestamp },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode { NotFound, InvalidTransition, Conflict, Invalid, WouldCycle, NotReady, Claimed, Io }

impl AppError {
    pub fn code(&self) -> ErrorCode { match self { /* one arm per variant, no wildcard */ } }
    pub fn hint(&self) -> Option<String> { match self { /* e.g. "run tk show app-k3x9 and retry" */ } }
}

impl ErrorCode {
    pub const fn exit(self) -> u8 { match self { /* a stable exit code per variant */ } }
}
```

Errors go to stderr as `{"code":"conflict","message":"...","hint":"..."}`. On a terminal, file errors render with annotate-snippets and point at the offending line, the same way `tk lint` reports. A new error variant fails to compile until it has a code, an exit status and a hint.

JSON output goes through view types built by exhaustive destructuring:

```rust
impl From<&Ticket> for TicketView {
    fn from(t: &Ticket) -> Self {
        // No `..`: a new Ticket field breaks this line until the JSON decides about it.
        let Ticket { id, title, kind, priority, labels, depends_on, created, status, body } = t;
        /* build the view */
    }
}
```

## TUI (ratatui)

The TUI follows the Elm architecture: a pure `update` turns each message into a new model plus a list of effects, and only the runtime touches disk.

```rust
struct Model {
    snapshot: Snapshot,
    view: View,                    // Board, List or Ready
    mode: Mode,
    selected: Option<TicketId>,    // by id, never by index
    scope: Scope,                  // this project or all projects
    filter: Filter,
    notice: Option<Notice>,
}

enum Mode {
    Normal,
    Detail(TicketId),
    Filtering(TextInput),          // the input exists only while filtering
    Confirm(Command),              // 'y' runs exactly this command
    Help,
}

enum Msg { Key(KeyEvent), FilesChanged, Finished(Result<Outcome, AppError>), Resize(u16, u16) }
enum Effect { Run(Command), Reload, Edit(TicketId), Quit }

fn update(model: &mut Model, msg: Msg) -> Vec<Effect>;   // pure: no I/O, no clock
fn view(model: &Model, frame: &mut Frame);
```

- **Mode state lives in its variant.** A filter input only exists while filtering, and a confirm prompt always knows exactly what it will run.
- **Selection by `TicketId`.** When an agent changes files and the list reloads, the cursor stays on the same ticket instead of jumping.
- **One path for writes.** `Effect::Run` goes through the same `execute` as the CLI. A conflict shows as a notice and triggers a reload.
- **Live reload.** notify 8.2 with notify-debouncer-full watches each project's `.tickets/`. The full debouncer follows renames, which is how every atomic write lands, so you see agents' changes as they work.
- **Safe editing.** `e` copies the ticket to a temp file, calls `ratatui::restore()`, runs the `editor` array from the workspace config (`code-insiders --wait`), then calls `ratatui::init()` again. tk parses the copy: if it's valid it saves through a transaction with the version check; if not, it shows the errors and offers to reopen. The real file is never left invalid.
- **No async runtime.** `ratatui::run()` sets up and restores the terminal. One thread reads terminal events, notify sends on the same `mpsc` channel, and the main loop drains it.

Views: **Board** (columns Todo, Doing, Blocked, Done), **List** (sortable table, fuzzy filter via frizbee in a tui-input box), **Ready** (what `tk next` would pick, across all projects), **Detail** (fields, dependencies with their status, and the body rendered by tui-markdown) and **Needs fixing** (files that failed to parse, with their errors).

| Key | Action |
| --- | --- |
| `j` / `k` | Move down / up |
| `h` / `l` | Previous / next column |
| `Enter` | Open detail |
| `n` | New ticket: title prompt, then the editor for the body |
| `e` | Edit in your editor, validated on save |
| `s` / `b` / `d` / `r` | Start / block / done / reopen |
| `p` | Cycle priority |
| `/` | Fuzzy filter |
| `g` | Toggle this project / all projects |
| `?` | Help |
| `q` | Quit |

Key handling is one `match` per mode, so a key added to `Normal` can't silently do something in `Filtering`.

## Multi-agent coordination

Leases stop two agents from starting the same ticket, even in different worktrees, because they live in the git directory all worktrees share.

- A lease is a file at `$(git rev-parse --git-common-dir)/tk/leases/<id>.toml` holding the actor and an expiry. It's never committed.
- `tk next --claim` picks the top ready ticket without a live lease and creates its lease with `create_new` (O\_EXCL). If two agents race, the OS lets exactly one win.
- Every `Start` takes a lease, humans included, then writes `doing` with `by` and `since` to the ticket file.
- Leases expire after 2 hours and renew on any tk write by the same actor. Taking over an expired lease happens under a lock file in the leases directory, which every worktree shares.
- `release` and `done` delete the lease.
- `Lease` has no public constructor. Only `Leases::acquire` returns one, and `execute` acquires one before every `Start`.

In a single checkout, the store lock and the state machine already stop double starts: the second agent's `Start` hits a ticket that is already `Doing`. Leases matter once agents work in parallel worktrees, which is why they come in phase 5.

## Handoffs between sessions

An agent's context fills up with old file reads and dead ends, and quality drops. The fix is one ticket per session: each session ends with a typed handoff, and the next one starts from a derived brief instead of the chat history.

**A handoff is typed log entries, and the brief is derived from them.** There is no `## Handoff` section to rewrite: the body stays the author's, tk still only appends, and nothing derived is stored.

```markdown
## Log
- 2026-10-15T09:20:45Z agent:claude: pure merge: fields 3-way, labels as sets
- 2026-10-15T09:48:00Z agent:claude ruled-out: 2-way log merge, loses one side's deletions
- 2026-10-15T09:48:00Z agent:claude decided: driver registration is printed, not run
- 2026-10-15T09:48:00Z agent:claude next: run cargo test -p tk -- --ignored
```

- **The format is `- <ts> <actor> <kind>: <text>`;** a note keeps the old `- <ts> <actor>: <text>`. An actor `Name` can't contain `:`, so the actor token ending in `:` marks a note and the two can't be confused. The kind is never read from the text, so a note like `open: ...` stays a note.
- **Existing logs and the merge driver are unchanged.** The driver only reads the timestamp and dedupes by exact line, so tagged lines merge like notes.
- **Rendering and parsing live together in `tk-store/log.rs`,** so one round-trip test covers the one place that defines the format. A line tk can't parse is kept as raw text, because people write under `## Log` too.

```rust
// tk-core
/// A log line's kind and text, parsed. A ref's text that isn't `<target> | <why>` leaves the line raw.
pub enum Entry { Note(NonEmpty), RuledOut(NonEmpty), Decided(NonEmpty), Next(NonEmpty), Ref(Reference), Unref(Reference) }
/// What `tk log` may write. No `Next`: only a handoff writes next steps. Refs have `tk ref`.
pub enum LogKind { Note, RuledOut, Decided }
/// nutype over Vec<NonEmpty>: a handoff must say what's next. If nothing is, use `tk done`.
pub struct NextSteps(Vec<NonEmpty>);
pub struct Handoff { pub next: NextSteps, pub ruled_out: Vec<NonEmpty>, pub decided: Vec<NonEmpty>, pub refs: Vec<Reference> }
/// What happens to the ticket after the handoff.
pub enum AfterHandoff { Keep, Release, Block { reason: NonEmpty } }

// tk-app
Command::Log     { id, text, #[serde(default)] kind: LogKind }   // old JSON stays valid
Command::Handoff { id, if_version, handoff: Handoff, then: AfterHandoff }
```

`Handoff` runs through the same `change` path as every write. It appends every entry with one `now`, then applies the move in the same closure. If the move is refused, nothing is written, and `keep_lease` renews the lease for `Keep` and releases it for `Release` or `Block`. A handoff on a ticket that isn't `doing` is allowed: notes taken before claiming are legitimate, and `--release` on a `todo` ticket is already refused by the state machine.

**The latest handoff is the newest group of `next` entries.** Only `Handoff` writes `next`, and timestamps are whole seconds from one `now` per command, so a handoff's entries share a timestamp and a later handoff replaces the earlier plan. Two handoffs within the same second read as one, which a session ending is never fast enough to hit. `ruled-out` and `decided` are facts, so they accumulate.

`Brief` is a query in tk-app and is never persisted. It's built by matching `Entry` exhaustively, so a new kind fails to compile until the brief decides where it goes:

| Part | Shown |
| --- | --- |
| Header | ID, title, status, holder and since |
| Waiting on | Open dependencies with their state; omitted when empty |
| Prose | The body without its log (`log::without_log`) |
| Next | The latest handoff's steps, with its age and how many notes were logged since, so a stale plan shows as stale |
| Where to look | The live refs, each located now (see Refs below), capped at 10 |
| Ruled out, Decided | All of them, newest first, capped at 10 with "N older: tk show" |
| Decided in a dependency | For each done dependency, its 3 newest decided entries |
| Notes | The last 5, with "N older: tk show" |

`tk brief` with no ID shows the tickets this checkout has in `doing` by the current actor. Each worktree's files only show its own starts, so this is scoped per worktree even when every agent is `agent:claude`. `tk next --claim --brief` claims and prints the brief in one step. MCP gets `brief` and `handoff` tools; `run` takes the new commands.

**Hooking it into an agent.** In Claude Code, a SessionStart hook matching `startup` and `compact` runs `tk brief`, so a fresh session starts from the brief and a compacted one gets it back after the summary. A hook can't make the agent write a handoff, because compaction gives the model no turn. Handoffs come from habit, so `tk guide` teaches the loop: claim, brief, work, handoff.

## Refs: where to look

Most of a fresh session's cost is orientation: finding the files, the section of this plan and the earlier decision that matter. Orientation is search, and search is what fills the context. Handoffs keep a ticket's story (what was tried, decided, next) but not its map, so every session searched again and threw the result away. **The session that pays for a search leaves the result behind, as a pointer with a reason.** The cheapest moment is planning: a session that reads this plan once can split it into tickets that each point at their two sections, and every session that runs one of them starts from a few lines instead of 700.

An earlier draft left out a `file` entry kind: changed files are in the diff, paths go stale on rename, and a `decided` note can name a file. A ref answers each. The diff shows what changed, not what was worth reading (a spec, a caller, a test). Staleness is handled by storing a stable anchor and deriving the line numbers. A note naming a file can't be checked, located, or dropped.

```markdown
- 2026-10-17T10:02:00Z agent:claude ref: plan.md#Open decisions | "Done tickets": the original archive rule
- 2026-10-17T10:09:00Z agent:claude ref: crates/tk-app/src/query.rs#fn dep_states | Ready must see archived deps
- 2026-10-17T10:15:00Z agent:claude unref: crates/tk-store | too broad: only merge.rs matters
```

- **A ref is a pointer and a why, never a copy.** A copy goes stale and bloats the brief; a bare pointer is noise. The why lets a session decide whether to open it without opening it, so it's required, like a ruled-out entry's reason.
- **Targets** (`RefTarget`): a repository path with an optional `#anchor`, a ticket ID, or an `http(s)` link. Text that parses as a ticket ID is one. Paths are `RepoPath` (no `..`, no leading `/`), so they mean the same in every clone and worktree. The anchor is plain text, so it works for any language with no parser: the first line containing it, except that in Markdown a heading containing it wins and gives its whole section, up to the next heading of the same or a higher level.
- **Store the anchor, derive the lines.** The brief locates every ref on each read, `plan.md:589-643 (55 lines)`, so line numbers are never stale and the session knows the cost before it reads. Nothing derived is stored.
- **Refs are a set, folded from the log.** A ref to a target already referenced replaces its why in place; an `unref` removes it, with a why too ("too broad" is worth knowing). Facts accumulate; the map is kept current. The log keeps the history and merges like every other line.
- **Checked when written, flagged when stale.** `ref`, `new --ref` and `handoff --ref` refuse a missing file, anchor or ticket (`bad_ref`), so a typo is caught by the session that made it. An `unref` must name a live ref. `tk lint` reports refs on open tickets whose target is gone; refs on done tickets are history.
- **Done dependencies' decisions are inline.** `depends_on` already says this ticket builds on another, and what that ticket decided is the contract. The brief shows each done dependency's three newest `decided` entries; its notes and refs stay out. A ref to a ticket shows its state and title only.
- **Bounded.** The brief shows 10 refs; more than that suggests splitting the ticket.

Writing refs is a habit, like handoffs, so `tk guide` gives the rule: if you had to search for it, ref it; if you read it and it didn't matter, don't.

Left out on purpose: inlining file contents (line ranges let a session pull exactly what it needs), finding refs automatically from prose or diffs (noise), suggesting refs from other tickets with the same labels (a recommendation engine; knowledge every ticket needs belongs in the repo's docs), and a separate out-of-scope kind (`decided: X is out of scope` says it).

## Where types can't reach

Types prove shapes and states. They can't prove that text round-trips, that the filesystem behaves or that git merges cleanly, so ten tests cover exactly that.

| Test | Why types can't cover it |
| --- | --- |
| `TicketId`: parse then display gives the same text, for all 1,048,576 suffixes | String formats are outside the type system; checking every value is a proof, not a sample |
| Ticket: render then parse gives the same ticket (proptest) | TOML and Markdown are text |
| `update` then render equals rendering the new ticket from scratch (proptest) | The lossless path and the full render must agree |
| Changing one field leaves every other byte of the file identical | Lossless editing is a property of bytes |
| Load, let another writer save, then save with the old version: `Conflict` | Needs real files |
| Two threads claim one ticket: exactly one gets the lease | Needs the real OS |
| A cycle and a missing dependency are both reported | Built from runtime data |
| Two branches change status and append logs; the merge keeps both log lines and valid TOML | Needs real git |
| A log entry of every kind, refs to every target shape included, renders then parses back the same (proptest); an old untagged line parses as a note, and a hand-written line or a ref without its why as raw text | Log lines are text |
| An anchor finds a Markdown heading's section, skipping fenced code and trailing blanks, before prose that mentions it | Markdown is text |

The proptest generators build values through nutype's constructors, so they only produce valid tickets.

Nothing else is tested. The exhaustive `match` and the declared nutype rules are the specification. The render-and-reparse check in `save` (step 3) also runs in production and catches anything the tests miss before it reaches disk. Now and then, run cargo-mutants on tk-store to confirm these tests catch injected bugs.

## Build phases

Build in phases. After phase 3 agents can use tk, and from then on tk tracks its own tickets.

1. **Skeleton.** Workspace, four crates, lint config, `rust-toolchain.toml`, `bacon`, and rust-analyzer set to run clippy. Done when `cargo clippy` passes on the empty crates with every lint on and tk-core builds as `no_std`.
2. **tk-core.** IDs, nutype newtypes, `Status`, `Move`, transitions, `Ticket`, `ValidGraph`, `Ready`. Write every signature with `todo!()` first, then fill them in until the compiler is quiet. Done when no `todo!()` is left and the exhaustive ID test passes.
3. **tk-store, tk-app and the CLI (the MVP).** File format, transactions, `update` with field-level rendering, atomic writes, `--if-version`, `Command`, `execute`, errors, every CLI command and `tk lint`. Done when tk tracks its own work in `.tickets/` and the round-trip and conflict tests pass.
4. **TUI.** Board, list, detail, filter, the editor flow and live reload. Done when you run a week of work from the TUI alone.
5. **Multi-agent.** Leases, `tk next --claim` and `tk merge-driver`. Done when two agents in two worktrees never start the same ticket, and merging their branches needs no hand-editing.
6. **Optional.** `tk mcp` via [rmcp](https://crates.io/crates/rmcp), the official Rust MCP SDK, over the same `Command`; dynamic ID completion once clap\_complete stabilizes it; `tk archive` for old done tickets.
7. **Handoffs.** Typed log entries and `tk brief` first, since they're useful on their own, then `tk handoff`, then `tk guide` and the hook recipe. Done when a fresh agent session picks up a ticket from `tk brief` alone, without re-exploring what the last session ruled out.
8. **Refs.** `Entry::Ref`/`Unref`, `tk ref`, `--ref` on `new` and `handoff`, Where to look and dependencies' decisions in the brief, stale refs in `tk lint`. Done when a fresh session reaches its first edit by reading the brief's line ranges, without searching the repository.

## CLAUDE.md for the tk repo

Put these rules in the repo so agents building tk keep the guarantees instead of working around them.

```markdown
# Building tk

- Change types first. Add the variant or field, run `cargo clippy`, and fix every error it reports.
- Never write `_ =>` on a domain enum, or `..` when destructuring a domain struct for rendering or JSON.
- No `unwrap`, `expect`, `panic!`, `unreachable!` or slice indexing. Return a typed error.
- Never `#[allow]` a lint. Use `#[expect(lint, reason = "...")]`.
- Every new text field is a nutype with its rules declared. No hand-written validation.
- tk-core stays `no_std`: no I/O, no clock. Take `Timestamp` and random bytes as arguments.
- Never add a public constructor for `Version`, `Ready`, `Tx` or `Lease`, or a public setter for a ticket's status or dependencies.
- Change tickets only inside `Store::transaction`, through `Loaded::update`.
- Don't store anything that can be derived.
- Add a test only at a text, filesystem or git boundary.

## Tickets (tk)

- Your ticket's brief is printed at session start; if not, run `tk brief`, or `tk next --claim --brief` if you have none.
- Trust its Ruled out and Decided entries. Don't re-investigate them.
- Log only what the next session would otherwise have to rediscover, with the reason: `tk log <id> --ruled-out "X: why"`, `--decided "Y: why"`, or a plain note.
- If you had to search for something, ref it so nobody searches again: `tk ref <id> "<path>#<anchor>" --why "what is there"`. Read refs at the lines the brief gives.
- Done: `cargo lint && cargo lint-core` pass, then `tk done <id>`.
- Stopping before done (blocked, context getting long, end of a work chunk): `tk handoff <id> --next "<concrete step: file, command>"`.
- Anything else: `tk guide`.
```

## Open decisions

- **Name.** `tk` is a placeholder; check it doesn't clash with anything on your PATH.
- **Non-code work.** An `ops` repo with its own `.tickets/` is the simplest home for certification, marketing and admin tickets.
- **Done tickets.** Keep them in place, or add `tk archive` to move old ones into `.tickets/archive/`.
- **Lease length.** Two hours is a starting guess; tune it once agents run in parallel.
- **ID length.** Four characters give about a million IDs per project, and a rare clash between two branches shows up as a git conflict on the filename. Use five if that feels tight.
- **Forcing a start.** `Ready` is the only way to start a ticket. If you want a `--force` escape hatch for yourself, give it its own type, such as `Override`, so the exception stays visible in the code.
- **Cross-project dependencies** are out of v1, since they would need two repo locks per command. Revisit if you miss them.

## Sources

- [toml\_edit documentation](https://docs.rs/toml_edit)
- [std::fs::File::lock](https://doc.rust-lang.org/stable/std/fs/struct.File.html#method.lock), stable since Rust 1.89
- [rmcp on crates.io](https://crates.io/crates/rmcp), the official Rust MCP SDK
- Every library version, release date and alternative checked: the Review tab

