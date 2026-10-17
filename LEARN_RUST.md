# Learning Rust through tk

A guide for a developer who already knows **Go** and **Laravel/PHP**, using the code in this repository.

Every code sample here is real code from tk, with its path, unless it is marked *(simplified)* or *(Go)* / *(PHP)*. Every compiler error is real output from rustc 1.99 (the toolchain pinned in `rust-toolchain.toml`), produced by breaking a scratch copy of the repo on purpose. Your files were not touched.

> **Snapshot note.** This guide was written against commit `a293c7c`. While it was being written, another session was refactoring the `## Log` code (a new `crates/tk-core/src/entry.rs`; `EntryKind`/`LogKind` moving out of `fields.rs`). The guide avoids that area, apart from one example marked as in progress.

---

## Contents

0. [How to use this guide](#0-how-to-use-this-guide)
1. [The toolchain, in Go and Composer terms](#1-the-toolchain-in-go-and-composer-terms)
2. [The map: workspace, crates, modules, visibility](#2-the-map-workspace-crates-modules-visibility)
3. [Syntax you need on day one](#3-syntax-you-need-on-day-one)
4. [Ownership and borrowing: the big one](#4-ownership-and-borrowing-the-big-one)
5. [Structs, and privacy as a guarantee](#5-structs-and-privacy-as-a-guarantee)
6. [Enums and pattern matching: the best part](#6-enums-and-pattern-matching-the-best-part)
7. [Error handling without exceptions or `if err != nil`](#7-error-handling-without-exceptions-or-if-err--nil)
8. [Traits and generics](#8-traits-and-generics)
9. [Closures and lifetimes](#9-closures-and-lifetimes)
10. [Iterators and collections (Laravel Collections, compiled)](#10-iterators-and-collections-laravel-collections-compiled)
11. [Serde: data in and out](#11-serde-data-in-and-out)
12. [Newtypes: "parse, don't validate"](#12-newtypes-parse-dont-validate)
13. [Typestate and proof values: tk's signature move](#13-typestate-and-proof-values-tks-signature-move)
14. [`no_std` and a pure core](#14-no_std-and-a-pure-core)
15. [Macros](#15-macros)
16. [Concurrency and async, as tk uses them](#16-concurrency-and-async-as-tk-uses-them)
17. [Architecture patterns](#17-architecture-patterns)
18. [Testing](#18-testing)
19. [Lints as guardrails](#19-lints-as-guardrails)
20. [Reading order through the codebase](#20-reading-order-through-the-codebase)
21. [Exercises](#21-exercises)
22. [Compiler error field guide](#22-compiler-error-field-guide)
23. [Cheat sheet: Go / PHP → Rust](#23-cheat-sheet-go--php--rust)

---

## 0. How to use this guide

Rust feels different from Go and PHP for one main reason: **the compiler checks things you're used to checking with discipline, code review and tests.** tk takes this further than most Rust projects. Its whole design (`plan.md`) is "if it compiles and clippy passes, an invalid ticket can't be written". So it's a very good codebase to learn from: almost every line exists to make the compiler enforce something.

Suggested approach:

1. Read sections 1–7 in order. They're the foundation.
2. Keep the code open next to the guide. Every reference like `crates/tk-core/src/status.rs:138` is clickable in VS Code.
3. Do the "Try it" boxes. They take a minute each and you learn more from them than from reading.
4. Sections 8–19 can be read in any order once the basics are in place.
5. Finish with the exercises in section 21. They're real changes to tk, guided by compiler errors.

The habit to build: **when the compiler complains, read the entire message.** Rust's error messages are the best of any mainstream language. They often contain the fix, and they always tell you *why*.

---

## 1. The toolchain, in Go and Composer terms

| You know | Rust | Notes |
| --- | --- | --- |
| `go` / `php` + `composer` | `cargo` | Build tool, package manager, test runner and task runner in one |
| `go.mod` / `composer.json` | `Cargo.toml` | |
| `go.sum` / `composer.lock` | `Cargo.lock` | Committed for binaries, as here |
| `go.work` | `[workspace]` in the root `Cargo.toml` | tk has 4 crates in one workspace |
| `go build` | `cargo build` | `--release` for optimized builds |
| `go vet` + `golangci-lint` | `cargo clippy` | tk uses it as the gate: `cargo lint` |
| `gofmt` / Pint | `cargo fmt` | |
| `go test ./...` / `phpunit` | `cargo test` | |
| `go doc` | `cargo doc --open` | Builds HTML docs **for your own code** too. Try it here. |
| pkg.go.dev / Packagist | crates.io + docs.rs | |
| GOTOOLCHAIN / `.php-version` | `rust-toolchain.toml` | Pins rustc 1.99.0 here |

The commands this repo uses (from `CLAUDE.md` and `.cargo/config.toml`):

```sh
bacon                       # re-runs clippy on every save; the "compiler as to-do list" loop
cargo lint                  # alias: clippy on the whole workspace, warnings = errors
cargo lint-core             # alias: clippy on tk-core for a bare-metal target (proves no_std)
cargo test                  # unit + property tests
cargo test -p tk -- --ignored   # the real-git merge test
rustc --explain E0382       # a long explanation of any error code
```

`cargo check` is the fastest command: it type-checks without generating code. Use it (or bacon) while you write. The IDE does this too: rust-analyzer shows the same errors inline.

> **Try it.** Run `cargo doc --open -p tk-core --no-deps`. You get a documentation site for tk's domain types. The `///` comments in the code become its text, and `//!` comments become each module's intro.

---

## 2. The map: workspace, crates, modules, visibility

### 2.1 Four crates, dependencies pointing inward

```text
tk (binary: CLI, TUI, MCP)  ──►  tk-app (Command, execute, errors, JSON)  ──►  tk-store (files, transactions)  ──►  tk-core (domain rules)
```

A **crate** is Rust's compilation unit, roughly a Go module with one package tree, or a Composer package. Each crate lists its dependencies in its own `Cargo.toml`. `tk-core/Cargo.toml` doesn't list `tk-store`, so tk-core *cannot* import it. The architecture is enforced by the build, not by convention.

In Laravel nothing stops a model from calling `Http::get()` or a controller from writing raw SQL. In Go you can approximate this with `internal/` and package boundaries, but a domain package can still import `os` and `net/http`. In tk, `tk-core` can't even reach the standard library's file system (see section 14).

### 2.2 Shared settings in the workspace

The root `Cargo.toml` declares versions once and lints once:

```toml
# Cargo.toml
[workspace.dependencies]
tk-core = { path = "crates/tk-core" }
serde = { version = "1", default-features = false }

[workspace.lints.clippy]
unwrap_used = "deny"
wildcard_enum_match_arm = "deny"
```

Each crate opts in:

```toml
# crates/tk-store/Cargo.toml
[lints]
workspace = true

[dependencies]
tk-core = { workspace = true }
serde = { workspace = true, features = ["std", "derive"] }
```

**Features** are compile-time switches that a crate offers, a bit like Go build tags (`//go:build`). tk-core has an optional `schema` feature that pulls in `schemars`:

```toml
# crates/tk-core/Cargo.toml
[features]
schema = ["dep:schemars", "schemars/derive", "schemars/jiff02"]
```

The code then says "derive `JsonSchema` only when the feature is on":

```rust
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Priority { /* ... */ }
```

`#[cfg(...)]` is conditional compilation. `#[cfg_attr(condition, attribute)]` applies an attribute only when the condition is true.

### 2.3 Modules: one file per module, declared explicitly

In Go, every `.go` file in a directory belongs to the package automatically. In Rust, **a file is only compiled if a parent declares it** with `mod`:

```rust
// crates/tk-core/src/lib.rs
mod fields;      // compiles src/fields.rs as module `fields`
mod graph;
mod handoff;
mod ids;
#[cfg(feature = "schema")]
mod schema;      // only compiled with the feature
mod status;
mod text;
mod ticket;

pub use fields::{Actor, ActorError, EntryKind, Kind, LogKind, Priority};
pub use graph::{DepError, GraphProblem, NotReady, Ready, ValidGraph};
pub use status::{InvalidTransition, Move, Resolution, StateName, Status, TransitionName};
pub use ticket::{Frontmatter, NewTicket, Ticket, TicketView};
```

The modules are private (`mod`, not `pub mod`). The crate then **re-exports** the chosen types with `pub use`. Other crates write `tk_core::Ticket`, not `tk_core::ticket::Ticket`. This is the crate's public API, curated in one place, like a PHP package that exposes only some classes from its `src/`.

Paths inside a crate:

| Path | Meaning | Like |
| --- | --- | --- |
| `crate::ids::TicketId` | from the crate root | `App\Models\...` absolute namespace |
| `super::*` | the parent module | `..` |
| `self::model::Model` | this module's child | `./model` |

You'll see `use std::io::{self, Write as _};` in several files. `self` imports `io` itself. `Write as _` imports the trait only to make its methods (like `write_all`) callable, without binding the name. More on traits in section 8.

### 2.4 Visibility: closer to PHP's `private` than to Go's package scope

| Rust | Visible to | Go | PHP |
| --- | --- | --- | --- |
| (nothing) | this module and its children | lowercase (but Go = whole package) | `private` |
| `pub(crate)` | the whole crate | lowercase, or `internal/` | — |
| `pub(super)` | the parent module | — | `protected`, roughly |
| `pub` | everyone | Uppercase | `public` |

**The key difference from Go:** in Go, an unexported field is visible to *every file in the package*. In Rust, a private field is visible only *inside the module that defines it* (plus its children). `Ticket.status` is private to `ticket.rs`, so even `graph.rs`, in the same crate, has to call `t.status()`. This is what makes the guarantees in section 5 work.

`pub(crate)` is used a lot in tk for "only my crate may do this":

```rust
// crates/tk-core/src/ticket.rs:140
/// Only `ValidGraph::add_dep` calls this, after the cycle check.
pub(crate) fn with_dep(mut self, dep: TicketId) -> Self {
```

tk-store and tk-app can't add a dependency without going through the cycle check, because they can't see `with_dep`.

---

## 3. Syntax you need on day one

### 3.1 Immutable by default

```rust
let paths = Vec::new();        // can't be changed
let mut paths = Vec::new();    // can be changed
```

Go and PHP variables are always mutable. In Rust, `mut` is a signal to the reader: *this one changes.* The compiler warns if you write `mut` and never change the variable.

### 3.2 Everything is an expression

`if`, `match` and `{ blocks }` produce values. That's why Rust has no ternary operator:

```rust
// crates/tk-store/src/format.rs:125
let gap = if body.is_empty() { "" } else { "\n" };
```

The **last expression of a block, without a semicolon, is its value.** That includes function bodies, so you rarely write `return`:

```rust
// crates/tk-core/src/ticket.rs:146
pub fn id(&self) -> &TicketId {
    &self.id          // no semicolon: this is the return value
}
```

Adding a semicolon (`&self.id;`) turns it into a statement whose value is `()` (the empty tuple, Rust's "void"). The compiler then reports "expected `&TicketId`, found `()`". Early returns still use `return`, as in Go.

### 3.3 Shadowing

You can declare a new variable with the same name. It's used to transform a value step by step without inventing names:

```rust
// crates/tk/src/main.rs:209
let text = text_or_stdin(&text)?;                         // String
let text = NonEmpty::try_new(text.trim_end().to_owned())  // now a NonEmpty
    .map_err(|e| AppError::Invalid(format!("log text: {}", e.explain())))?;
```

Go doesn't allow `:=` to redeclare a variable in the same scope unless another new variable is on the left. In Rust shadowing is normal and idiomatic.

### 3.4 Numbers: no implicit conversions (like Go)

```rust
// crates/tk-core/src/ids.rs:33
fn symbol(byte: u8) -> u8 {
    ALPHABET
        .get(usize::from(byte & 0x1f))   // u8 -> usize: lossless, so `from`
        .copied()
        .unwrap_or(b'0')
}
```

- Lossless conversion: `usize::from(x)` or `x.into()`.
- Conversion that can fail: `u8::try_from(c)` returns a `Result`.
- `x as u8` exists but truncates silently. tk avoids it.
- `b'0'` is a byte literal (`u8`), `b"abc"` is a byte string (`&[u8; 3]`), `'a'` is a `char` (a Unicode scalar value, 4 bytes).

Overflow: in debug builds `u8` arithmetic that overflows **panics**. In release builds it wraps. When it matters, use `checked_add` and friends. tk does this for time:

```rust
// crates/tk-app/src/execute.rs:205
fn lease_until(now: Timestamp) -> Result<Timestamp, AppError> {
    now.checked_add(LEASE_TTL)
        .map_err(|e| AppError::Internal(e.to_string()))
}
```

### 3.5 Strings: `String` and `&str`

This trips up everyone coming from Go and PHP.

| Type | What it is | Go analogy |
| --- | --- | --- |
| `String` | an owned, growable UTF-8 buffer on the heap | a `[]byte` you own, guaranteed UTF-8 |
| `&str` | a borrowed view into UTF-8 bytes someone else owns | a `string` (read-only view) |
| `&'static str` | a `&str` that lives for the whole program, like a literal | a string constant |

Rule of thumb: **take `&str` as a parameter, return or store `String`.**

```rust
// crates/tk/src/cli.rs:287
fn title(s: &str) -> Result<Title, &'static str> {
    Title::try_new(s.to_owned()).map_err(|e| e.explain())
}
```

`s.to_owned()`, `s.to_string()` and `String::from(s)` all turn a `&str` into a `String` by copying. `&my_string` or `my_string.as_str()` goes the other way, for free.

**Strings are UTF-8 bytes, and you can't index them** (`s[0]` doesn't compile). Slicing `&s[0..3]` compiles but **panics** if the cut lands inside a multi-byte character. That's why tk denies it:

```toml
# Cargo.toml
string_slice = "deny"                  # byte slicing can panic inside "ção"
```

You know this problem from PHP (`substr` vs `mb_substr`) and from Go (`s[i]` gives a byte, not a rune). tk uses methods that can't panic, like `split_once`, `strip_prefix` and `.get(range)`, which returns an `Option`:

```rust
// crates/tk-core/src/ids.rs:87
let (project, suffix) = s.split_once('-').ok_or(IdError::MissingDash)?;
```

### 3.6 Formatting

```rust
format!("{id}.md")                       // variables can go inside the braces
format!("{} isn't a UTF-8 path", p.display())
write!(f, "{}-{}", self.project, self.suffix)   // into a formatter or buffer
eprintln!("warning: {warning}");          // to stderr
```

`{}` uses the `Display` trait (meant for users), `{:?}` uses `Debug` (for developers, like Go's `%+v`, or `dd()` in Laravel). Section 8 covers both.

The `!` means these are **macros**, not functions. They expand at compile time, which is how `format!` checks your placeholders against the arguments at compile time.

---

## 4. Ownership and borrowing: the big one

Go has a garbage collector. PHP has reference counting with copy-on-write arrays and objects passed by handle. **Rust has neither.** It frees memory at a point the compiler knows statically, using three rules:

1. Every value has exactly one **owner** (a variable, a field, a collection slot).
2. When the owner goes out of scope, the value is **dropped** (freed, closed, released).
3. You can lend a value out by reference: **either** any number of shared `&T` references **or** exactly one exclusive `&mut T`, never both at once.

Think of rule 3 as a read-write lock that the compiler checks at compile time, with no runtime cost.

### 4.1 Moves

Assigning or passing a non-`Copy` value **moves** it. The old variable can't be used any more.

tk is built on this. Look at how a status changes:

```rust
// crates/tk-core/src/ticket.rs:123
pub fn transition(self, m: Move, now: Timestamp) -> Result<Self, InvalidTransition> {
    self.apply(Transition::Move(m), now)
}
```

`self`, not `&self`: the method **takes ownership** of the ticket and gives back a new one (or an error). Here's what happens if you try to use the old ticket afterwards (real output):

```text
error[E0382]: use of moved value: `t`
   --> crates/tk-core/examples/demo.rs:5:13
    |
  3 | fn moved_twice(t: Ticket, now: Timestamp) {
    |                - move occurs because `t` has type `Ticket`, which does not implement the `Copy` trait
  4 |     let a = t.transition(Move::Release, now);
    |               ------------------------------ `t` moved due to this method call
  5 |     let b = t.transition(Move::Reopen, now);
    |             ^ value used here after move
    |
note: `tk_core::Ticket::transition` takes ownership of the receiver `self`, which moves `t`
   --> crates/tk-core/src/ticket.rs:123:23
    |
123 |     pub fn transition(self, m: Move, now: Timestamp) -> Result<Self, InvalidTransition> {
    |                       ^^^^
help: you can `clone` the value and consume it, but this might not be your desired behavior
```

Read the `help:` line carefully: "this might not be your desired behavior". The compiler offers `.clone()` but warns that it might be wrong. In tk it *is* wrong: you'd end up with two diverging versions of the same ticket. The move is the design. tk calls it **"ownership as protocol"**: if a step must happen only once, the function consumes its input.

In Go, `t2 := t.Transition(...)` leaves `t` usable and stale. Nothing tells you not to touch it again.

### 4.2 Copy vs Clone

Small, plain values are `Copy`: assignment copies the bits and the original stays usable. Numbers, `bool`, `char`, and any type you mark `#[derive(Clone, Copy)]` whose fields are all `Copy`:

```rust
// crates/tk-core/src/fields.rs:13
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, /* ... */)]
pub enum Priority { P0, P1, #[default] P2, P3 }
```

`Ticket` is `Clone` but not `Copy`. It holds `String`s and sets, so copying it costs allocations, and Rust makes you write `.clone()` explicitly so the cost is visible. Go copies structs silently (a shallow copy: slices and maps inside still point to the same data). PHP copies arrays silently and shares objects silently.

You'll see `.clone()` throughout tk where a value really needs two owners, e.g. `t.id().clone()` to put an ID in an error while the ticket keeps its own.

### 4.3 The four kinds of `self`

| Receiver | Means | Go analogy | Example in tk |
| --- | --- | --- | --- |
| `&self` | read-only borrow | value receiver (but no copy) | `fn status(&self) -> &Status` |
| `&mut self` | exclusive, mutable borrow | pointer receiver | `fn add_project(&mut self, ...)` in `workspace.rs` |
| `self` | takes ownership; caller loses it | — | `fn transition(self, ...)` |
| `mut self` | takes ownership and mutates the local copy | — | `fn remove_dep(mut self, ...) -> Self` |

`remove_dep` is a common Rust idiom: take the value, change it, hand it back. Calls chain, and nobody else can hold a reference to the half-changed ticket:

```rust
// crates/tk-core/src/ticket.rs:134
pub fn remove_dep(mut self, dep: &TicketId) -> Self {
    self.depends_on.remove(dep);
    self
}
```

### 4.4 Struct update syntax moves fields

```rust
// crates/tk-core/src/ticket.rs:127
pub(crate) fn apply(self, t: Transition, now: Timestamp) -> Result<Self, InvalidTransition> {
    let status = next(&self.status, t, now)?;
    Ok(Self { status, ..self })
}
```

`Self { status, ..self }` builds a new `Ticket` with the new `status` and **moves** every other field out of the old `self`. No clones. It's like a PHP spread or `$model->fill()`, but nothing is copied.

`next(&self.status, ...)` lends the status to `next` only for the duration of the call. Once that borrow ends, `self` can be moved again.

### 4.5 Borrowing rules in action: splitting a borrow

`Ctx` holds the projects (`stores`) and a random-byte source (`entropy`, which needs `&mut` to call). Creating a ticket needs both at once:

```rust
// crates/tk-app/src/execute.rs:331
fn create(ctx: &mut Ctx, key: &ProjectKey, spec: impl Fn() -> NewTicket, depends_on: &BTreeSet<TicketId>)
    -> Result<Outcome, AppError>
{
    let Ctx { stores, now, entropy, .. } = ctx;
    let store = stores
        .get(key)
        .ok_or_else(|| AppError::UnknownProject(key.clone()))?;
    store.transaction(|tx| {
        // ... calls entropy()? inside, up to 16 times
```

Why destructure instead of calling the handy `ctx.store(key)`? Here's what happens with the "obvious" version (real output, after editing a scratch copy):

```rust
let store = ctx.store(key)?;          // borrows *all of* ctx, shared
let now = &ctx.now;
let entropy = &mut ctx.entropy;       // wants ctx.entropy, exclusive
```

```text
error[E0502]: cannot borrow `ctx.entropy` as mutable because it is also borrowed as immutable
   --> crates/tk-app/src/execute.rs:339:19
    |
337 |     let store = ctx.store(key)?;
    |                 --- immutable borrow occurs here
338 |     let now = &ctx.now;
339 |     let entropy = &mut ctx.entropy;
    |                   ^^^^^^^^^^^^^^^^ mutable borrow occurs here
340 |     store.transaction(|tx| {
    |     ----- immutable borrow later used here
```

`ctx.store()` is a method on `&self`, so for as long as `store` lives, the compiler counts **all of `ctx`** as borrowed. Destructuring borrows **each field separately**: `stores` (shared) and `entropy` (exclusive) don't overlap, so both are allowed. The compiler tracks borrows per field inside one function, but a method call borrows the whole receiver. That's the lesson.

In Go this is a data race waiting to happen if anything runs concurrently. Rust rejects it even in single-threaded code, because the same rules are what make multi-threaded Rust safe.

### 4.6 Moving out of a `&mut`: `mem::replace` and `mem::take`

The TUI keeps its current mode in `model.mode`, and some modes own data (an input box, a draft). On a key press it needs to *take* that data out by value. But `model` is only a `&mut Model`: borrowed, not owned. You can't move a field out of something you've only borrowed, because the owner would be left holding a hole.

The fix is to put something else in its place and take the old value:

```rust
// crates/tk/src/tui/model.rs:361
match std::mem::replace(&mut model.mode, Mode::Normal) {
    Mode::Normal => normal_key(model, key),
    Mode::Detail { id, scroll } => detail_key(model, key, id, scroll),
    Mode::Filtering(input) => filter_key(model, key, input),
    // ...
}
```

Here's the naive `match model.mode {` (real output, trimmed):

```text
error[E0507]: cannot move out of `model.mode.id` as enum variant `Detail` which is behind a mutable reference
   --> crates/tk/src/tui/model.rs:361:11
    |
361 |     match model.mode {
    |           ^^^^^^^^^^
...
364 |         Mode::Detail { id, scroll } => detail_key(model, key, id, scroll),
    |                        -- data moved here
365 |         Mode::Filtering(input) => filter_key(model, key, input),
    |                         ----- ...and here
help: consider borrowing the pattern binding
    |
364 |         Mode::Detail { ref id, scroll } => detail_key(model, key, id, scroll),
    |                        +++
```

**The suggested fix is a trap.** `ref id` would borrow from `model.mode` while `detail_key(model, ...)` also needs `&mut model`, which is an E0502. `mem::replace` is the right fix: the model is left in a valid state (`Normal`), and each handler puts back whatever mode comes next. The compiler's hints are excellent but local. Sometimes the right fix is to restructure the code.

`std::mem::take(&mut x)` is `replace(&mut x, Default::default())`, used for a one-shot flag:

```rust
// crates/tk/src/tui/model.rs:307
if std::mem::take(&mut model.edit_created) {   // read the bool and reset it to false
```

---

## 5. Structs, and privacy as a guarantee

### 5.1 A struct with some fields private

```rust
// crates/tk-core/src/ticket.rs:13
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Ticket {
    id: TicketId,
    pub title: Title,
    pub kind: Kind,
    pub priority: Priority,
    pub labels: Labels,
    depends_on: BTreeSet<TicketId>,
    created: Timestamp,
    status: Status,
    pub body: Body,
}
```

Look at which fields are `pub`. Title, kind, priority, labels and body can be set freely by anyone, because every value of their *types* is already valid (section 12). `id`, `depends_on`, `created` and `status` are private, because changing them has rules: status has a state machine, dependencies need a cycle check.

Compare Eloquent: `$ticket->status = 'done'; $ticket->save();` works from anywhere, including a Blade view. `$fillable`/`$guarded` protect only mass assignment. In Rust, outside `ticket.rs` (real output):

```text
error[E0616]: field `status` of struct `Ticket` is private
 --> crates/tk-core/examples/demo.rs:9:7
  |
9 |     t.status = Status::Todo;
  |       ^^^^^^ private field
```

Reading goes through getters that return **references** (no copy, read-only):

```rust
// crates/tk-core/src/ticket.rs:155
pub fn status(&self) -> &Status {
    &self.status
}
```

### 5.2 There are no constructors, only functions

Rust has no `__construct` and no `New...` convention enforced by the language. A struct literal `Ticket { ... }` is the only primitive way to build one, and **it's only available where every field is visible.** Since `Ticket` has private fields, code outside `ticket.rs` must use the functions it offers:

```rust
// crates/tk-core/src/ticket.rs:74
/// New tickets always start as `Todo`, with no dependencies.
#[must_use]
pub fn new(spec: NewTicket, id: TicketId, now: Timestamp) -> Self { /* ... status: Status::Todo ... */ }

/// The way in for a ticket read from disk. The ID comes from the filename.
#[must_use]
pub fn from_stored(id: TicketId, front: Frontmatter, body: Body) -> Self { /* ... */ }
```

`new` is just a naming convention. `#[must_use]` makes ignoring the return value a warning (and the workspace's `unused_must_use = "deny"` makes it an error).

`Frontmatter` goes further: all of its fields are private and it has *no* constructor function. The only way to get one is to deserialize a file. A ticket loaded from disk has necessarily gone through serde and all the validation rules.

### 5.3 Builders that check required fields at compile time

```rust
// crates/tk-core/src/ticket.rs:28
/// What a new ticket needs. Leaving out the title is a compile error.
#[derive(bon::Builder)]
pub struct NewTicket {
    title: Title,
    #[builder(default)]
    kind: Kind,
    // ...
}
```

Used in `execute.rs:61`:

```rust
NewTicket::builder()
    .title(title.clone())
    .kind(kind)
    .priority(priority)
    .labels(labels.clone())
    .body(body.clone())
    .build()
```

The `bon` crate generates a builder whose *type* changes as you set fields (a "typestate" builder). `.build()` only exists on the type where `title` has been set. Laravel factories and Go functional options check this at runtime, if at all.

### 5.4 Borrowed views and the "no `..`" rule

```rust
// crates/tk-core/src/ticket.rs:59
#[derive(Clone, Copy, Debug)]
pub struct TicketView<'a> {
    pub id: &'a TicketId,
    pub title: &'a Title,
    pub kind: Kind,
    pub priority: Priority,
    // ...
}
```

`TicketView<'a>` is a struct of *references* into a `Ticket`, a read-only snapshot of every field, private ones included. `'a` is a lifetime (section 9): "this view can't outlive the ticket it borrows from". Because it holds only references and `Copy` values, it's `Copy` itself.

Every renderer destructures it **without `..`**:

```rust
// crates/tk-store/src/format.rs:53
let TicketView {
    id: _,
    title,
    kind,
    priority,
    labels,
    depends_on,
    created,
    status,
    body,
} = ticket.view();
```

`id: _` explicitly says "I've seen this field and I'm ignoring it". If someone adds a field to `Ticket` (and therefore to `TicketView`), this line fails to compile in every renderer, JSON view and file writer, and each one has to decide what to do with the new field. With `..` it would be silently skipped. This is a project rule (`CLAUDE.md`: "Never write ... `..` when destructuring a domain struct for rendering or JSON").

---

## 6. Enums and pattern matching: the best part

### 6.1 Enums that carry data

Go has no enums, only `const ( Todo = iota ... )` plus discipline. PHP 8.1 enums are names with an optional scalar value. **Rust enums are "sum types": each variant can carry its own fields.**

```rust
// crates/tk-core/src/status.rs:12
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Status {
    Todo,
    Doing {
        by: Actor,
        since: Timestamp,
    },
    Blocked {
        reason: NonEmpty,
        since: Timestamp,
    },
    Done {
        at: Timestamp,
        resolution: Resolution,
    },
}
```

This is "make illegal states unrepresentable". Compare the Go version you'd probably write *(Go)*:

```go
type Status struct {
    State      string      // "todo" | "doing" | "blocked" | "done"
    By         *Actor      // only when doing
    Reason     *string     // only when blocked
    Since      *time.Time  // doing or blocked
    At         *time.Time  // only when done
    Resolution *Resolution // only when done
}
```

Or the Laravel migration: `status` plus five nullable columns. Either way, nothing prevents `State: "todo"` with a `Reason`, or `"done"` with no `At`. In Rust a `Done` without `at` can't be constructed, and a `Todo` *has* no reason field to set.

A variant can also wrap a single value, tuple-style:

```rust
// crates/tk-core/src/status.rs:34
pub enum Resolution {
    Completed,
    WontDo,
    Duplicate(TicketId),
}
```

### 6.2 `match` must be exhaustive

Here's tk's entire state machine. It's the heart of the project, so read it slowly:

```rust
// crates/tk-core/src/status.rs:138
pub(crate) fn next(
    current: &Status,
    t: Transition,
    now: Timestamp,
) -> Result<Status, InvalidTransition> {
    use Move as M;
    use Status as S;
    use Transition as T;
    match (current, t) {
        (S::Todo, T::Start { by }) => Ok(S::Doing { by, since: now }),
        (S::Todo | S::Doing { .. }, T::Move(M::Block { reason })) => {
            Ok(S::Blocked { reason, since: now })
        }
        (S::Blocked { .. }, T::Move(M::Unblock))
        | (S::Doing { .. }, T::Move(M::Release))
        | (S::Done { .. }, T::Move(M::Reopen)) => Ok(S::Todo),
        (S::Todo | S::Doing { .. } | S::Blocked { .. }, T::Move(M::Finish { resolution })) => {
            Ok(S::Done {
                at: now,
                resolution,
            })
        }
        // Every invalid pair is named, never `_`: a new state, move or transition
        // fails to compile until each pairing is decided.
        (S::Doing { .. } | S::Blocked { .. } | S::Done { .. }, t @ T::Start { .. })
        | (S::Todo, t @ T::Move(M::Unblock | M::Release | M::Reopen))
        | (S::Doing { .. }, t @ T::Move(M::Unblock | M::Reopen))
        | (S::Blocked { .. }, t @ T::Move(M::Block { .. } | M::Release | M::Reopen))
        | (
            S::Done { .. },
            t @ T::Move(M::Block { .. } | M::Unblock | M::Release | M::Finish { .. }),
        ) => Err(InvalidTransition {
            from: current.name(),
            attempted: t.name(),
        }),
    }
}
```

The pattern features used here:

| Pattern | Meaning |
| --- | --- |
| `(a, b)` | match two values at once, as a tuple |
| `S::Todo \| S::Doing { .. }` | either of these (an "or-pattern") |
| `S::Doing { .. }` | the `Doing` variant, ignoring its fields |
| `T::Start { by }` | the `Start` variant, binding its `by` field to a variable |
| `t @ T::Start { .. }` | match this shape, and bind the whole matched value to `t` |
| `use Status as S;` | a local alias, scoped to the function, to keep the table readable |

`match` returns a value, so each arm is an expression. Notice also that `by` and `reason` are *moved* out of `t` (which `next` owns) into the new status. No clones.

Now the payoff. Here's what happens when you add a state. I added `Archived` to `Status` in a scratch copy and ran `cargo check` (real output):

```text
error[E0004]: non-exhaustive patterns: `&Status::Archived` not covered
   --> crates/tk-core/src/status.rs:108:15
    |
108 |         match self {
    |               ^^^^ pattern `&Status::Archived` not covered

error[E0004]: non-exhaustive patterns: `(&Status::Archived, _)` not covered
   --> crates/tk-core/src/status.rs:148:11
    |
148 |     match (current, t) {
    |           ^^^^^^^^^^^^ pattern `(&Status::Archived, _)` not covered

error[E0004]: non-exhaustive patterns: `StateName::Archived` not covered
   --> crates/tk-core/src/graph.rs:192:15
    |
192 |         match t.status().name() {
    |               ^^^^^^^^^^^^^^^^^ pattern `StateName::Archived` not covered
```

**That's the to-do list for the feature, written by the compiler**: the state's name, every transition into and out of it, and whether an archived ticket can be started. `CLAUDE.md` puts it as "Change types first. Add the variant or field, run `cargo clippy`, and fix every error it reports."

Errors appear crate by crate, innermost first. Once you fix tk-core, tk-store's errors show up, then tk-app's, then the binary's.

### 6.3 Why `_ =>` is banned

A wildcard arm would make the example above compile silently, and `Archived` tickets would fall into whatever `_` did. tk's lints forbid it. Real clippy output:

```text
error: wildcard match will also match any future added variants
  --> crates/tk-core/examples/demo.rs:10:9
   |
10 |         _ => "whenever",
   |         ^ help: try: `Priority::P1 | Priority::P2 | Priority::P3`
```

The project allows a wildcard only on enums it doesn't own, with a written reason:

```rust
// crates/tk/src/tui/model.rs:352
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "KeyCode is crossterm's input enum, not a domain enum: unbound keys do nothing"
)]
fn key_press(model: &mut Model, key: KeyEvent) -> Vec<Effect> {
```

### 6.4 `Option` and `Result` are ordinary enums

There is no `nil` or `null` in safe Rust. "Maybe absent" is a type:

```rust
enum Option<T> { Some(T), None }
enum Result<T, E> { Ok(T), Err(E) }
```

You can't use an `Option<Actor>` as an `Actor` without handling `None`. A whole class of Go nil-pointer panics and PHP "Attempt to read property on null" errors goes away.

### 6.5 The pattern-matching toolbox

**`if let`**: match one pattern and ignore the rest.

```rust
// crates/tk-app/src/execute.rs (Command::Finish arm)
if let Resolution::Duplicate(of) = &resolution {
    check_duplicate(ctx, &id, of)?;
}
```

**`let ... else`**: bind or bail. This is Rust's guard clause:

```rust
// crates/tk/src/main.rs:45
let Some(cmd) = cmd else {
    // no subcommand: open the TUI
    // ...
    return Ok(ExitCode::SUCCESS);
};
// from here on, `cmd` is the unwrapped value
```

**Match guards**: `if` conditions on an arm. tk uses these everywhere for "not found is fine":

```rust
// crates/tk-store/src/store.rs:58
let text = match fs_err::read_to_string(&path) {
    Ok(text) => text,
    Err(e) if e.kind() == io::ErrorKind::NotFound => {
        return Err(StoreError::NoProject(repo.to_owned()));
    }
    Err(e) => return Err(e.into()),
};
```

**Let chains** (new in edition 2024): combine conditions and pattern matches with `&&`. This one also uses a **slice pattern**, `[store]`, meaning "a slice with exactly one element":

```rust
// crates/tk-app/src/execute.rs:392
if claim && let [store] = stores.as_slice() {
    return claim_in(store, actor, now);
}
```

**Ranges and `@` on characters:**

```rust
// crates/tk/src/tui/model.rs (normal_key, body abbreviated)
KeyCode::Char(c @ '1'..='4') => { /* c is the digit pressed */ }
```

**`matches!`**: a pattern test that returns a `bool`:

```rust
// crates/tk-app/src/execute.rs (keep_lease)
let renew = matches!(after, Status::Doing { by, .. } if by == actor);
```

**Array destructuring** (from a test):

```rust
// crates/tk-core/src/graph.rs:380
let [_, b, c] = tickets;    // tickets is [Ticket; 3]; moves out the 2nd and 3rd
```

### 6.6 Capability through visibility: `Move` vs `Transition`

```rust
// crates/tk-core/src/status.rs:40
/// What callers can ask for. There is deliberately no `Start` here:
/// only [`Ready::start`](crate::Ready::start) can start a ticket.
pub enum Move {
    Block { reason: NonEmpty },
    Unblock,
    Release,
    Finish { resolution: Resolution },
    Reopen,
}

/// Every transition, `Start` included. Private to tk-core.
pub(crate) enum Transition {
    Start { by: Actor },
    Move(Move),
}
```

Public callers can express every move except "start". To start a ticket you need a `Ready` value, which only the dependency graph hands out (section 13). The set of things you *can say* is limited by what types you can *name*.

### 6.7 Enums for UI state, too

The TUI uses the same idea for its modes:

```rust
// crates/tk/src/tui/model.rs
/// State that exists only in one mode lives in that variant: a filter input
/// exists only while filtering, and a confirm prompt knows exactly what it runs.
pub enum Mode {
    Normal,
    Detail { id: TicketId, scroll: u16 },
    Filtering(Input),
    Prompt(Prompt, Input),
    /// `y` runs exactly this command.
    Confirm { question: String, command: Command },
    Reedit { id: TicketId, draft: Draft, error: String },
    Help,
}
```

In a Livewire or Vue component this would be `$isFiltering`, `$filterInput`, `$confirmQuestion`, `$pendingCommand`... and bugs where two flags are true at once.

---

## 7. Error handling without exceptions or `if err != nil`

### 7.1 `Result` and the `?` operator

Errors are values, as in Go, but carried in one `Result<T, E>` instead of a `(T, error)` pair. The `?` operator replaces `if err != nil { return nil, err }`:

*(Go)*
```go
data, err := os.ReadFile(path)
if err != nil {
    return nil, err
}
```

*(Rust, `crates/tk-store/src/store.rs:440`)*
```rust
let bytes = fs_err::read(path)?;
```

`expr?` means: if `Ok(v)`, evaluate to `v`. If `Err(e)`, **convert `e` with `From`** and return it from the current function. The conversion is the part Go doesn't have, and it's what makes layered errors cheap (7.3).

`?` also works on `Option` in a function that returns `Option`:

```rust
// crates/tk-store/src/format.rs:259
/// The 0-based line containing byte `at`.
fn line_of(text: &str, at: usize) -> Option<usize> {
    Some(text.get(..at)?.matches('\n').count())
}
```

Compared to Laravel's exceptions, errors are part of the function signature. Every function that can fail says so in its return type, and the caller can't forget to handle it: an unused `Result` is a warning (here, an error).

### 7.2 Defining errors: enums + `thiserror`

```rust
// crates/tk-store/src/error.rs:7
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("ticket {0} not found")]
    NotFound(TicketId),
    #[error("ticket {0} already exists")]
    Exists(TicketId),
    #[error(transparent)]
    Conflict(Conflict),
    #[error(transparent)]
    Invalid(Box<InvalidFile>),
    #[error("no tk project at {0}; run `tk init --key <key>` there")]
    NoProject(Utf8PathBuf),
    #[error("{path} has format {found}; this tk reads format 1")]
    Format { path: Utf8PathBuf, found: u32 },
    // ...
    #[error(transparent)]
    Io(#[from] io::Error),
}
```

- `#[derive(thiserror::Error)]` generates the `std::error::Error` and `Display` implementations.
- `#[error("...")]` is the message. `{0}` is the first tuple field, `{path}` a named field.
- `#[error(transparent)]` uses the inner error's message.
- `#[from]` generates `impl From<io::Error> for StoreError`, so `?` on an I/O error inside a function returning `StoreError` just works.

Compared to Go's `errors.Is` / `errors.As` on wrapped errors, you `match` on the variant, and the compiler tells you when you missed one.

`Invalid(Box<InvalidFile>)`: `InvalidFile` is large (it holds the whole file text). Since a `Result` is as big as its largest variant, boxing keeps every `Result<_, StoreError>` small. `Box<T>` is a pointer to a heap value with a single owner.

### 7.3 Layered errors with exhaustive conversion

tk-app has its own error type, and converts tk-store's errors into it **with an exhaustive match**:

```rust
// crates/tk-app/src/error.rs:151
impl From<StoreError> for AppError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::NotFound(id) => Self::NotFound(id),
            StoreError::Conflict(c) => Self::Conflict(c),
            StoreError::Invalid(file) => Self::InvalidFile(file),
            StoreError::Io(e) => Self::Io(e),
            e @ (StoreError::NoProject(_)
            | StoreError::ProjectExists(_)
            | StoreError::Format { .. }
            | StoreError::Config { .. }
            | StoreError::WrongProject { .. }) => Self::Config(e.to_string()),
            e @ (StoreError::Exists(_) | StoreError::Unreadable(_)) => {
                Self::Internal(e.to_string())
            }
        }
    }
}
```

Because of this `From` impl, `tx.load(id)?` (which returns `StoreError`) works inside any function returning `AppError`. Each `AppError` then maps to a stable code, an exit status and a hint, each in its own exhaustive `match` (`error.rs:64-148`). Add an error variant and the compiler makes you give it a code, an exit status and a hint:

```rust
// crates/tk-app/src/error.rs:132
pub const fn exit(self) -> u8 {
    match self {
        Self::Invalid => 2,
        Self::NotFound => 3,
        Self::InvalidTransition => 4,
        Self::Conflict => 5,
        // ...
    }
}
```

In Laravel terms this is `App\Exceptions\Handler::render()`, except it can't miss an exception class.

### 7.4 Combinators: transforming results without `match`

You'll see these constantly:

| Method | On | Does |
| --- | --- | --- |
| `.map_err(f)` | Result | transform the error |
| `.map(f)` | Option/Result | transform the success value |
| `.ok_or(err)` | Option → Result | `None` becomes `Err(err)` |
| `.ok_or_else(\|\| err)` | Option → Result | same, but only builds the error if needed |
| `.ok()` | Result → Option | drop the error |
| `.and_then(f)` | Option/Result | chain another fallible step (flatMap) |
| `.unwrap_or(v)` / `.unwrap_or_default()` | Option/Result | fallback value |
| `.transpose()` | `Option<Result<T,E>>` ↔ `Result<Option<T>,E>` | swap the layers |
| `.is_some_and(f)` / `.is_none_or(f)` | Option | test the value if present |

A good chained example, `Option` all the way down:

```rust
// crates/tk/src/main.rs:357
fn user_actor() -> Result<Actor, AppError> {
    std::env::var("USER")
        .ok()                                   // Result -> Option
        .and_then(|user| Name::try_new(user).ok())
        .map(Actor::Human)                      // a variant used as a function!
        .ok_or_else(|| {
            AppError::Config(String::from(
                "no actor: pass --as, set TK_ACTOR, or set `actor` in workspace.toml",
            ))
        })
}
```

`.transpose()` in action: an optional `--body` flag whose reading can fail.

```rust
// crates/tk/src/main.rs:149
body: Body::new(
    body.map(|b| text_or_stdin(&b))   // Option<Result<String, AppError>>
        .transpose()?                  // Result<Option<String>, AppError>, then ?
        .unwrap_or_default(),          // String
),
```

### 7.5 No panics: `unwrap` is denied

`unwrap()` on `None`/`Err` **panics** (crashes the thread), like Go's `panic` or an uncaught exception. tk denies `unwrap`, `expect`, `panic!`, `unreachable!` and indexing like `v[i]` (which panics when out of bounds) outside tests. Real clippy output:

```text
error: used `unwrap()` on an `Option` value
 --> crates/tk-core/examples/demo.rs:4:5
  |
4 |     s.chars().next().unwrap()
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^
  |
  = note: if this value is `None`, it will panic
```

Instead of `v[i]`, use `v.get(i)`, which returns an `Option`. Even "impossible" cases get handled:

```rust
// crates/tk-core/src/ids.rs:33
// The mask keeps the index below 32, so the fallback never runs.
ALPHABET.get(usize::from(byte & 0x1f)).copied().unwrap_or(b'0')
```

`clippy.toml` allows `unwrap` in tests (`allow-unwrap-in-tests = true`), where a panic *is* the test failure.

### 7.6 Errors that hand the value back

When a function takes ownership (section 4) and then fails, the caller would lose the value. tk's checks return it inside the error:

```rust
// crates/tk-core/src/graph.rs:188
pub fn ready(&self, t: Ticket) -> Result<Ready, (Ticket, NotReady)> {
```

`Err((ticket, reason))` is a tuple: you get your ticket back along with the reason. Callers that don't need it write `.map_err(|(_, e)| AppError::NotReady(e))`.

### 7.7 Problems that aren't errors

Some problems shouldn't undo a command, like a lease that couldn't be renewed after the file was saved. Those go into `ctx.warnings: Vec<String>`, and the binary prints and clears them:

```rust
// crates/tk/src/main.rs:333
fn warn(ctx: &mut Ctx) {
    for warning in ctx.warnings.drain(..) {
        eprintln!("warning: {warning}");
    }
}
```

`drain(..)` removes every element and yields them by value. The vector is left empty and keeps its allocation.

---

## 8. Traits and generics

### 8.1 Traits are interfaces, implemented explicitly

A **trait** is a set of methods a type can implement, like an interface. Two big differences from Go:

- **Explicit**: you write `impl Display for TicketId`. Go interfaces are satisfied implicitly. PHP is explicit too (`implements`).
- **You can implement your trait for someone else's type**, or someone else's trait for your type (but not someone else's trait for someone else's type: the "orphan rule").

tk's own trait:

```rust
// crates/tk-core/src/text.rs:75
/// Why text was rejected, as one sentence for people. Each match is exhaustive
/// over the error nutype generates, so a new rule fails to compile until it is explained.
pub trait Explain {
    fn explain(&self) -> &'static str;
}

impl Explain for TitleError {
    fn explain(&self) -> &'static str {
        match self {
            Self::NotEmptyViolated => "a title can't be blank",
            Self::LenCharMaxViolated => "a title has at most 120 characters",
            Self::PredicateViolated => "a title is one line",
        }
    }
}
```

`TitleError` is generated by a macro (section 12), and tk implements its own trait for it. To call `.explain()` elsewhere, the trait must be in scope: `use tk_core::Explain as _;` (in `cli.rs` and `main.rs`). That's a common surprise: **trait methods are only callable when the trait is imported.**

### 8.2 The two-way text pair: `Display` + `FromStr`

tk's IDs print as `app-k3x9` and parse from the same text. That's two standard traits:

```rust
// crates/tk-core/src/ids.rs:83
impl FromStr for TicketId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, IdError> {
        let (project, suffix) = s.split_once('-').ok_or(IdError::MissingDash)?;
        Ok(Self {
            project: ProjectKey::try_new(String::from(project)).map_err(IdError::Project)?,
            suffix: suffix.parse()?,
        })
    }
}

/// The exact inverse of `FromStr`.
impl fmt::Display for TicketId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.project, self.suffix)
    }
}
```

What you get for free:

- `"app-k3x9".parse::<TicketId>()` calls `from_str`. `::<TicketId>` is the "turbofish", which tells a generic function which type to produce when inference can't.
- `id.to_string()` works because the standard library implements `ToString` for **every** type that implements `Display` (a "blanket implementation").
- `format!("{id}")` works.

`type Err = IdError;` is an **associated type**: part of the trait that each implementor picks. A generic function can refer to it as `T::Err`:

```rust
// crates/tk/src/cli.rs:283
fn parsed<T: std::str::FromStr>(s: &str) -> Result<T, T::Err> {
    s.parse()
}
```

This is how clap is told to parse IDs, actors and versions with one helper: `value_parser = parsed::<TicketId>`.

There's an exhaustive test that every one of the 1,048,576 possible suffixes round-trips (`crates/tk-core/src/ids.rs:133`).

### 8.3 `From` and `Into`

`From` is for conversions that can't fail. Implementing `From<A> for B` gives you `B::from(a)` **and** `a.into()` (another blanket impl):

```rust
// crates/tk-store/src/version.rs:55
/// Every version read can be passed back as the expected one.
impl From<Version> for VersionHex {
    fn from(v: Version) -> Self {
        Self(v.0)
    }
}
```

Note what *doesn't* exist: `From<VersionHex> for Version`. A version typed by a user can be *compared* to a real one, but it can never pretend to be one.

### 8.4 `Drop`: like `defer`, but attached to the value

When a value goes out of scope, Rust calls its `Drop::drop`. Files close, locks release and memory frees this way. It's like Go's `defer`, but **tied to the value rather than the function**, so it also runs on every early `return` and every `?`.

The project lock in `Store::transaction` relies on it:

```rust
// crates/tk-store/src/store.rs:141
pub fn transaction<R, E: From<StoreError>>(
    &self,
    f: impl for<'t> FnOnce(&mut Tx<'t>) -> Result<R, E>,
) -> Result<R, E> {
    let lock = fs_err::File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(self.dir.join(LOCK))
        .map_err(StoreError::from)?;
    lock.file().lock().map_err(StoreError::from)?;
    // Released when `lock` drops, after the closure returns.
    f(&mut Tx {
        store: self,
        _lock: &lock,
    })
}
```

And the lease guard implements `Drop` itself:

```rust
// crates/tk-store/src/lease.rs:46
#[must_use]
#[derive(Debug)]
pub struct Lease {
    leases: Leases,
    id: TicketId,
    kept: bool,
}

impl Lease {
    /// The start this lease was taken for is saved: keep the lease.
    pub fn keep(mut self) {
        self.kept = true;
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if !self.kept && self.leases.release(&self.id).is_err() {
            // Nothing more to do here: an unreleased lease expires on its own.
        }
    }
}
```

If anything between "acquire the lease" and "save the ticket" fails, the `Lease` is dropped on the way out and releases itself. `keep(mut self)` consumes the guard, sets the flag, and the guard is then dropped *with* `kept = true`. Go's version would be `defer func() { if !kept { release() } }()` and you'd have to remember to write it.

> Why the odd `if ... .is_err() {}`? The usual "ignore this Result" idiom, `let _ = ...`, is denied by the `let_underscore_must_use` lint. This form makes ignoring the error explicit, with a comment saying why.

### 8.5 Derives: traits written for you

`#[derive(...)]` generates trait implementations:

| Derive | Gives you | Go/PHP analogy |
| --- | --- | --- |
| `Debug` | `{:?}` formatting | `%+v`, `var_dump` |
| `Clone` | `.clone()` | deep copy |
| `Copy` | implicit bitwise copies | Go value semantics |
| `PartialEq, Eq` | `==` | `==` on comparable structs |
| `PartialOrd, Ord` | `<`, sorting | `sort.Slice` with a less func |
| `Hash` | use as `HashMap` key | comparable map keys |
| `Default` | `T::default()` | Go zero values (opt-in here) |
| `Serialize, Deserialize` | serde (section 11) | `json:"..."` tags |

**Derived ordering follows declaration order.** That's why priorities sort the right way:

```rust
// crates/tk-core/src/fields.rs:12
/// Variants are declared most urgent first, so sorting puts P0 at the top.
pub enum Priority { P0, P1, #[default] P2, P3 }
```

Go has zero values for everything. Rust has **no implicit default**: a type gets `Default` only if you derive or implement it. `#[default]` picks the default variant.

The `strum` crate derives more: `Display`/`EnumString` (to and from `"p0"`), `VariantNames` (the list of names, which clap uses for `--priority` choices), `IntoStaticStr` (`<&str>::from(kind)`), `EnumIter`.

### 8.6 Generics, `impl Trait` and `dyn Trait`

**Generics with bounds.** `transaction` works for any result type `R` and any error type `E`, as long as a `StoreError` can be converted into `E`:

```rust
pub fn transaction<R, E: From<StoreError>>(&self, f: ...) -> Result<R, E>
```

That's what lets tk-app call it with `AppError`, and tk-store's tests with `StoreError`. Rust compiles a separate copy of a generic function for each concrete type (monomorphization), so there's no runtime cost. Go's generics use a mix of approaches. PHP has none (only docblock generics for static analysis).

Generic types work the same way: `UpdateError<E>`, `Capped<T>` in `brief.rs`, `fn reply<T: Serialize>(result: Result<T, AppError>)` in `mcp.rs`.

**`impl Trait` in argument position** means "any type implementing this", shorthand for a generic:

```rust
// crates/tk-core/src/graph.rs:145
pub fn build<'a>(tickets: impl IntoIterator<Item = &'a Ticket>) -> (Self, Vec<GraphProblem>)
```

Callers pass a `&[Ticket; 3]` in tests and `snapshot.tickets()` (an iterator) in the app. Both work.

**`impl Trait` in return position** means "some type I won't name", usually an iterator chain whose real type is unwritable:

```rust
// crates/tk-core/src/graph.rs:170
pub fn ready_ids(&self) -> impl Iterator<Item = &TicketId> {
```

**`dyn Trait`** is a runtime-polymorphic value behind a pointer, the closest thing to a Go interface value:

```rust
// crates/tk-app/src/ctx.rs:18
pub entropy: Box<dyn FnMut() -> io::Result<[u8; 4]> + Send>,
```

"A heap-allocated something you can call repeatedly (`FnMut`), returning 4 random bytes or an I/O error, safe to send to another thread (`Send`)." The binary passes `Box::new(entropy)`, a plain function. Tests could pass a closure returning fixed bytes. This is dependency injection with no container (section 14).

| | Static (`impl Trait` / generics) | Dynamic (`dyn Trait`) |
| --- | --- | --- |
| Dispatch | at compile time, inlinable | through a vtable at runtime |
| Can store different types in one field/Vec | no | yes |
| Go analogy | generics | interface values |

Also note `clock: fn() -> Timestamp` in `tui/mod.rs` and `mcp.rs`. Lowercase `fn` is a plain **function pointer** type, which is simpler than a closure trait and enough when nothing needs to be captured.

---

## 9. Closures and lifetimes

### 9.1 Closures and the three `Fn` traits

`|args| body` is a closure. It can capture variables from the scope around it. Which trait it implements depends on what it does with those captures:

| Trait | Can be called | Captures | tk example |
| --- | --- | --- | --- |
| `FnOnce` | once | can move captured values out | `Loaded::update(self, f: impl FnOnce(Ticket) -> ...)` |
| `FnMut` | many times, not at the same time | can mutate captures | `Ctx.entropy`, the notify callback in `tui/mod.rs` |
| `Fn` | many times, even concurrently | only reads captures | `spec: impl Fn() -> NewTicket` in `create` |

Why is `spec` an `Fn`? `create` may need a new random ID up to 16 times, and each try builds a fresh `NewTicket`. The builder *consumes* its inputs, so the closure clones what it captured each time it's called:

```rust
// crates/tk-app/src/execute.rs:61
let spec = || {
    NewTicket::builder()
        .title(title.clone())
        .kind(kind)                // Kind is Copy: no clone needed
        .priority(priority)
        .labels(labels.clone())
        .body(body.clone())
        .build()
};
```

A closure that captures a local by mutable reference, used to smuggle a value out:

```rust
// crates/tk-app/src/execute.rs:255 (start_ticket)
let mut lease = None;
let edited = loaded.update(|t| -> Result<Ticket, AppError> {
    let ready = graph.ready(t).map_err(|(_, e)| AppError::NotReady(e))?;
    if let Some(leases) = leases {
        lease = Some(leases.acquire(ready.ticket().id(), actor, now, lease_until(now)?)?);
    }
    Ok(ready.start(actor.clone(), now)?)
})?;
let saved = tx.save(edited)?;
if let Some(lease) = lease {
    lease.keep();
}
```

Follow the error paths: if `ready.start` fails, or `tx.save` fails, the function returns early through `?`, `lease` is dropped, and `Drop` releases it. The lease is only kept on the one path where everything succeeded. No cleanup code anywhere.

`move |...|` forces the closure to take ownership of its captures. It's needed when the closure outlives the current function, for example a file-watcher callback running on another thread:

```rust
// crates/tk/src/tui/mod.rs:34
let _watcher = watch(&ctx, move |result: DebounceEventResult| {
    // uses `tx` (a channel sender) moved into the closure
```

### 9.2 Lifetimes: how long a borrow is valid

A **lifetime** (`'a`, `'t`) is a name for the region of code where a reference is valid. Most of the time the compiler infers them ("lifetime elision"). You write them when a struct stores a reference, or when it's ambiguous which input a returned reference comes from.

- `TicketView<'a>`: a view that can't outlive the `Ticket` it borrows.
- `Tx<'t>`: a transaction handle that borrows the `Store` and the lock file.
- `fn ctx(&self) -> MutexGuard<'_, Ctx>`: `'_` means "the lifetime elision picked", here the lifetime of `&self`.
- `'static`: valid for the whole program. String literals are `&'static str`, which is why `explain()` can return them without allocating.

### 9.3 The transaction that can't escape

Here's the transaction handle:

```rust
// crates/tk-store/src/store.rs:215
/// Access to one project's files while its lock is held.
pub struct Tx<'t> {
    store: &'t Store,
    _lock: &'t fs_err::File,
}
```

It borrows `lock`, a local variable inside `transaction`. A borrow can't outlive what it borrows, so **a `Tx` cannot exist after `transaction` returns**, which is exactly when the lock is released.

Compare Laravel:

```php
DB::transaction(function () use (&$leaked) {
    $leaked = $this->repo;   // nothing stops you from keeping things that should die with the transaction
});
```

In Rust, trying to stash the `Tx` outside (real output):

```rust
let mut leaked: Option<&mut Tx<'_>> = None;
store.transaction(|tx| {
    leaked = Some(tx);
    Ok::<(), StoreError>(())
})?;
```

```text
error[E0521]: borrowed data escapes outside of closure
 --> crates/tk-store/examples/leak.rs:8:9
  |
6 |     let mut leaked: Option<&mut Tx<'_>> = None;
  |         ---------- `leaked` declared here, outside of the closure body
7 |     store.transaction(|tx| {
  |                        -- `tx` is a reference that is only valid in the closure body
8 |         leaked = Some(tx);
  |         ^^^^^^^^^^^^^^^^^ `tx` escapes the closure body here
```

The signature `f: impl for<'t> FnOnce(&mut Tx<'t>) -> Result<R, E>` reads as: "a closure that works for **any** lifetime `'t` that `transaction` picks." The closure can't assume the `Tx` lives one moment longer than the call. `for<'t>` is called a higher-ranked trait bound. You'll rarely write one, but now you can read it.

So "every read-check-write runs under the lock" is guaranteed by the compiler, not by a code reviewer.

---

## 10. Iterators and collections (Laravel Collections, compiled)

If you like Laravel Collections, you'll like Rust iterators. They're **lazy** (like `LazyCollection`): nothing runs until something consumes the chain, usually `.collect()`. They also compile down to the same code as a hand-written loop.

### 10.1 Three ways to iterate

| Call | Yields | Collection afterwards |
| --- | --- | --- |
| `v.iter()` | `&T` | still yours |
| `v.iter_mut()` | `&mut T` | still yours, changed in place |
| `v.into_iter()` | `T` (owned) | gone (moved) |

`for x in &v` is `v.iter()`, and `for x in v` is `v.into_iter()`.

### 10.2 Laravel → Rust

| Laravel Collection | Rust iterator |
| --- | --- |
| `map` | `map` |
| `filter` | `filter` |
| `flatMap` | `flat_map` |
| `first(fn)` | `find(f)` |
| `contains(fn)` | `any(f)` |
| `every(fn)` | `all(f)` |
| `sortBy` | collect, then `sort_by_key` (sorting needs all items) |
| `pluck` | `map(\|t\| &t.field)` |
| `merge` / `concat` | `chain` |
| `count` | `count` |
| `reduce` | `fold` |
| `values()->all()` | `collect::<Vec<_>>()` |
| `keyBy` | `map(\|t\| (key, t)).collect::<BTreeMap<_, _>>()` |
| `filterMap`-style | `filter_map` |
| `take(n)` | `take(n)` |

### 10.3 Real chains

**Variants as functions.** `Entry::RuledOut` is a tuple variant, which is also a function `fn(NonEmpty) -> Entry`, so it can go straight into `map`. *(This is the in-progress version of `handoff.rs`.)*

```rust
// crates/tk-core/src/handoff.rs:30
pub fn entries(self) -> Vec<Entry> {
    let Self { next, ruled_out, decided, refs } = self;
    ruled_out
        .into_iter()
        .map(Entry::RuledOut)
        .chain(decided.into_iter().map(Entry::Decided))
        .chain(refs.into_iter().map(Entry::Ref))
        .chain(next.into_inner().into_iter().map(Entry::Next))
        .collect()
}
```

**`Option` is an iterator of zero or one items**, so optional flags can be chained with repeated ones:

```rust
// crates/tk/src/cli.rs:215
let changes = title
    .map(FieldChange::Title)
    .into_iter()                          // Option -> 0 or 1 items
    .chain(kind.map(FieldChange::Kind))   // an Option can be chained directly
    .chain(priority.map(FieldChange::Priority))
    .chain(add_label.into_iter().map(FieldChange::AddLabel))
    .chain(rm_label.into_iter().map(FieldChange::RemoveLabel))
    .collect();
```

**`collect` is generic: the target type decides what gets built.** The same chain can produce a `Vec`, a `BTreeSet`, a `String`, a TOML `Array` or even a `Result<Vec<_>, E>` (which stops at the first error):

```rust
// crates/tk-core/src/graph.rs:147
let mut excluded: BTreeSet<TicketId> = problems
    .iter()
    .flat_map(GraphProblem::tickets)
    .cloned()            // &TicketId -> TicketId
    .collect();
```

```rust
// crates/tk-store/src/format.rs:173
fn ids_value<'a>(ids: impl IntoIterator<Item = &'a TicketId>) -> Value {
    Value::Array(ids.into_iter().map(ToString::to_string).collect::<Array>())
}
```

**Sorting by a tuple key.** Tuples compare element by element, so this sorts by priority, then creation time, then ID:

```rust
// crates/tk-core/src/graph.rs:170
pub fn ready_ids(&self) -> impl Iterator<Item = &TicketId> {
    let mut ready: Vec<(&TicketId, &Node)> = self
        .nodes
        .iter()
        .filter(|(_, n)| n.state == StateName::Todo && n.deps.iter().all(|d| self.is_done(d)))
        .collect();
    ready.sort_by_key(|&(id, n)| (n.priority, n.created, id));
    ready.into_iter().map(|(id, _)| id)
}
```

The closure parameter `|&(id, n)|` is a pattern too. It destructures the tuple reference right in the parameter list.

### 10.4 Which collection?

| Type | Use | Go | PHP |
| --- | --- | --- | --- |
| `Vec<T>` | growable list | slice | list array |
| `VecDeque<T>` | queue (BFS in `graph.rs:248`) | — | `SplQueue` |
| `BTreeMap<K, V>` | sorted map | map + sort | ordered array |
| `HashMap<K, V>` | fastest map, random order | map | assoc array |
| `BTreeSet<T>` | sorted set without duplicates | `map[T]struct{}` + sort | `array_unique` + sort |

tk uses **`BTree*` almost everywhere**. Iteration order is sorted and deterministic, so lists, files, JSON output and error reports come out the same on every run. Go randomizes map iteration on purpose, and PHP keeps insertion order. Here the order is a type-level property: `pub type Labels = BTreeSet<Label>;` is "sorted, without duplicates" by construction.

Map APIs you'll see: `.get(&k)` returns `Option<&V>` (never a zero value, never a panic). `.insert(k, v)` returns the *previous* value, which `graph.rs:105` uses to detect duplicates. `.entry(k).or_insert(v)` is upsert. `.retain(|k, v| ...)` filters in place.

---

## 11. Serde: data in and out

`serde` is Rust's serialization framework. `#[derive(Serialize, Deserialize)]` plus attributes play the role of Go struct tags and Laravel API Resources and casts. The same derive works for JSON, TOML and anything else, because the format lives in a separate crate (`serde_json`, `toml`).

### 11.1 The `Command` enum: one JSON line = one command

```rust
// crates/tk-app/src/command.rs:14 (abbreviated: some attributes and variants left out)
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    New {
        #[serde(default)]
        project: Option<ProjectKey>,
        title: Title,
        #[serde(default)]
        kind: Kind,
        // ...
    },
    Start {
        id: TicketId,
        #[schemars(with = "Option<String>")]
        if_version: Option<VersionHex>,
    },
    Block { id: TicketId, if_version: Option<VersionHex>, reason: NonEmpty },
    Finish {
        id: TicketId,
        if_version: Option<VersionHex>,
        #[serde(default = "completed")]
        resolution: Resolution,
    },
    // ...
}
```

The attributes mean:

- `tag = "op"`: "internally tagged". The variant name goes in an `op` field: `{"op":"start","id":"app-k3x9"}`.
- `rename_all = "snake_case"`: `DepAdd` becomes `"dep_add"`.
- `deny_unknown_fields`: a typo like `"reson"` is an error instead of being silently ignored. Laravel's `$request->validated()` drops unknown keys silently.
- `default`: the field may be missing, and `Default::default()` fills it. `default = "completed"` calls a function instead.
- A missing `Option<T>` field becomes `None` automatically.

So `tk apply` accepts lines like these:

```json
{"op":"new","title":"Export misses credit notes","priority":"p1","labels":["export"]}
{"op":"block","id":"app-k3x9","reason":"waiting on the API team"}
{"op":"edit","id":"app-k3x9","changes":[{"priority":"p0"},{"add_label":"urgent"}]}
{"op":"finish","id":"app-k3x9","resolution":{"duplicate":"app-aaaa"}}
```

> **Try it.** `cargo run -q -- schema | head -60` prints the JSON Schema generated from this enum by `schemars` (or run `tk schema` with your installed binary, if the working tree is mid-change and doesn't build). One type definition gives you the parser, the validation, the docs and the schema.

### 11.2 Deserializing *is* validating

Every field has a domain type: `Title`, `TicketId`, `NonEmpty`. Their `Deserialize` impls run the same rules as their constructors (section 12). So `serde_json::from_str::<Command>(line)` either gives you a fully valid command or an error naming the field. There's no "validated but still a string" stage.

Compare Laravel: a FormRequest validates `$request->input('title')`, but the controller still receives a string, and every later layer has to trust that validation happened.

### 11.3 How a ticket file maps to types

A real ticket file (`.tickets/tk-t3ed.md`):

```toml
+++
title = "Phase 6: tk archive moves old done tickets into .tickets/archive/"
kind = "feature"
priority = "p3"
labels = ["phase-6"]
depends_on = []
created = 2026-10-15T20:27:08Z
status = { state = "doing", by = "human:joaofelizardo3", since = 2026-10-15T20:27:21Z }
+++
```

`status = { state = "doing", ... }` is the `Status` enum with `#[serde(tag = "state")]`: the variant name in `state`, its fields next to it. `by = "human:..."` becomes an `Actor` through `FromStr` (11.4).

### 11.4 Serde through `Display`/`FromStr`

For types with a text form (`TicketId`, `Actor`), tk implements serde **by delegating to `Display` and `FromStr`**, so files, JSON and the CLI all parse through exactly one function:

```rust
// crates/tk-core/src/lib.rs:9
macro_rules! serde_via_str {
    ($($ty:ty),*) => {$(
        impl serde::Serialize for $ty {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }
        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = <alloc::string::String as serde::Deserialize>::deserialize(d)?;
                text.parse().map_err(serde::de::Error::custom)
            }
        }
    )*};
}
```

Then `serde_via_str!(TicketId);` in `ids.rs:102`. (Macros are covered in section 15.)

---

## 12. Newtypes: "parse, don't validate"

### 12.1 The idea

A **newtype** is a struct wrapping one value, like `struct Title(String)`. It costs nothing at runtime, and it's a different type to the compiler. If the only way to *create* a `Title` checks the rules, then **every `Title` anywhere in the program is valid**, and nothing needs to re-check it.

Go can do this (`type Title struct{ s string }` with a constructor), but zero values (`Title{}`) bypass the constructor, and same-package code can build one directly. PHP has value objects, but nothing stops `new Title('')` unless the constructor throws, and then it's a runtime error.

### 12.2 nutype: declare the rules, get the type

```rust
// crates/tk-core/src/text.rs:11
/// A ticket title: trimmed, 1-120 characters, one line.
#[nutype(
    sanitize(trim),
    validate(not_empty, len_char_max = 120, predicate = |s: &str| is_one_line(s)),
    derive(Debug, Clone, PartialEq, Eq, AsRef, Display, Serialize, Deserialize),
)]
pub struct Title(String);
```

The `nutype` macro generates:

- `Title::try_new(String) -> Result<Title, TitleError>`, which sanitizes, then validates.
- `enum TitleError { NotEmptyViolated, LenCharMaxViolated, PredicateViolated }`, one variant per rule.
- a `Deserialize` impl that runs the same rules.
- `.as_ref()` to read it as `&str`, and `.into_inner()` to unwrap it.
- **no other way in**. Real output from trying `Title("".to_string())`:

```text
error[E0423]: cannot initialize a tuple struct which contains private fields
  --> crates/tk-core/examples/demo.rs:17:5
   |
17 |     Title("".to_string())
   |     ^^^^^
   |
note: constructor is not visible here due to private fields
  --> crates/tk-core/src/text.rs:11:1
```

It's the same idea as Laravel validation rules (`'title' => 'required|max:120'`), but the rules are attached to the *type* instead of to one request.

The rule's error enum connects to section 8.1: `impl Explain for TitleError` matches every variant. If you add `len_char_min = 3` to `Title`, nutype adds a `LenCharMinViolated` variant and the `Explain` impl stops compiling until you write a message for it. That's exercise 3.

### 12.3 Validated at the edge, once

The CLI parses straight into domain types, so bad input is rejected by clap with a readable message before any tk logic runs:

```rust
// crates/tk/src/cli.rs (WriteCmd::New)
New {
    #[arg(value_parser = title)]
    title: Title,
    #[arg(long = "dep", value_parser = parsed::<TicketId>)]
    deps: Vec<TicketId>,
    // ...
}
```

From there on, function signatures *are* the documentation of what's been checked: `fn block(reason: NonEmpty)` can't receive an empty string.

### 12.4 Hand-written newtypes with private constructors

Not everything uses nutype. `Version` is a file hash that **only the store can create**:

```rust
// crates/tk-store/src/version.rs:6
/// Hash of a file's bytes when it was read. No public constructor:
/// the only way to get a `Version` is to read or write the file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Version(blake3::Hash);

impl Version {
    pub(crate) fn of(bytes: &[u8]) -> Self {
        Self(blake3::hash(bytes))
    }
    // ...
}
```

`Suffix` (`ids.rs:21`) is a `[u8; 4]` built only from random bytes mapped into a 32-symbol alphabet, or parsed from text. `TicketId` combines a `ProjectKey` and a `Suffix`. Each layer is valid because its parts are.

---

## 13. Typestate and proof values: tk's signature move

This is where everything above comes together. **A value that proves a check passed wraps the thing that was checked**, and it's the only way to reach the next step.

### 13.1 `Ready`: proof that a ticket may start

```rust
// crates/tk-core/src/graph.rs:34
/// A ticket that is `Todo` with every dependency `Done`.
/// Private field: [`ValidGraph::ready`] is the only way to make one.
#[derive(Debug)]
pub struct Ready(Ticket);

impl Ready {
    /// The only way to start a ticket.
    pub fn start(self, by: Actor, now: Timestamp) -> Result<Ticket, InvalidTransition> {
        self.0.apply(Transition::Start { by }, now)
    }
}
```

Follow the chain:

1. `Move` has no `Start` variant (6.6), so `ticket.transition(...)` can't start anything.
2. `Transition::Start` is `pub(crate)`, so other crates can't name it.
3. `Ready::start` is the only public path to `Start`.
4. `Ready`'s field is private, so you can't wrap a ticket yourself (real output):

```text
error[E0423]: cannot initialize a tuple struct which contains private fields
  --> crates/tk-core/examples/demo.rs:13:5
   |
13 |     Ready(t)
   |     ^^^^^
note: constructor is not visible here due to private fields
  --> crates/tk-core/src/graph.rs:37:18
   |
37 | pub struct Ready(Ticket);
   |                  ^^^^^^ private field
```

5. `ValidGraph::ready(&self, t: Ticket)` is the only function that produces a `Ready`. It takes the ticket **by value** and wraps *that same ticket*. You can't check ticket A and then start ticket B, because the check consumed A and handed back A.

In Go or Laravel, "only start a ticket whose dependencies are done" is a rule you'd enforce with an `if` in a service class and hope nobody adds a second code path.

### 13.2 `Loaded` → `Edited` → `Saved`: the write protocol

```rust
// crates/tk-store/src/store.rs
pub struct Loaded { ticket: Ticket, doc: TicketDoc, version: Version }      // what was read
pub struct Edited { before: Ticket, after: Ticket, doc: TicketDoc, base: Version }  // a change, ready to save
#[must_use]
pub struct Saved { pub id: TicketId, pub version: Version }                // proof it was written
```

- You get a `Loaded` only from `Tx::load` or `Tx::snapshot`, so only inside a transaction (9.3). It carries the hash of the bytes it came from.
- You get an `Edited` only from `Loaded::update(self, f)`, which consumes the `Loaded`:

```rust
// crates/tk-store/src/store.rs:287
pub fn update<E>(
    self,
    f: impl FnOnce(Ticket) -> Result<Ticket, E>,
) -> Result<Edited, UpdateError<E>> {
    let Self { ticket, doc, version } = self;
    let after = f(ticket.clone()).map_err(UpdateError::Rejected)?;
    if after.id() != ticket.id() {
        return Err(UpdateError::IdChanged);
    }
    Ok(Edited { before: ticket, after, doc, base: version })
}
```

- `Tx::save(&mut self, edit: Edited)` **consumes** the `Edited`, so one edit can't be saved twice (E0382 if you try). Before writing, it re-hashes the file on disk and refuses if it doesn't match `base`. That's optimistic locking: nobody else changed the file since you read it.
- `save` also renders the file and **parses it back** before writing (`check_reads_back`, `store.rs:489`), so a file tk couldn't read never reaches disk.

The module doc sums it up:

```rust
// crates/tk-store/src/store.rs:1
//! Four mistakes can't be expressed: writing outside a transaction, writing
//! without reading first, writing unvalidated data, and saving one edit twice.
```

### 13.3 Summary: the invariant and the mechanism

| Guarantee | Mechanism | Section |
| --- | --- | --- |
| A title is always valid | nutype, private constructor | 12 |
| A `Done` ticket always has a resolution | data inside the enum variant | 6.1 |
| Every status pair is decided | exhaustive `match`, no `_` | 6.2 |
| Only ready tickets start | `Ready` proof, `pub(crate) Transition` | 13.1 |
| Writes happen under the lock | `Tx<'t>` borrows the lock | 9.3 |
| No write without reading first | `Edited` only from `Loaded::update` | 13.2 |
| An edit is saved at most once | `save` consumes `Edited` | 4.1 |
| A failed start doesn't leave a lease | `Drop` on `Lease` | 8.4 |
| A new field is handled everywhere | destructuring without `..` | 5.4 |
| The core can't do I/O | `no_std` | 14 |

---

## 14. `no_std` and a pure core

```rust
// crates/tk-core/src/lib.rs:3
// The core cannot reach files, environment or clock.
#![cfg_attr(not(test), no_std)]
extern crate alloc;
```

- `#![...]` (with `!`) is an attribute on the whole crate.
- `no_std` removes the standard library. What's left is `core` (types, traits, `Option`, iterators, formatting) and, with `extern crate alloc`, heap types (`String`, `Vec`, `BTreeMap`, `Box`).
- There's no `std::fs`, no `std::env`, no `std::time::SystemTime`, no networking. tk-core *physically cannot* do I/O.
- `cfg_attr(not(test), ...)` lets the test build use `std` (for test helpers).

That's why tk-core files import things like `use alloc::string::String;` and `use core::fmt;`, which in normal Rust come from `std` automatically. `cargo lint-core` compiles tk-core for a bare-metal ARM target that *has* no OS, which proves nothing slipped in.

### 14.1 Time and randomness are arguments

Anything that depends on the outside world is passed in:

```rust
pub fn new(spec: NewTicket, id: TicketId, now: Timestamp) -> Self              // ticket.rs
pub fn from_entropy(bytes: [u8; 4]) -> Self                                    // ids.rs
pub fn transition(self, m: Move, now: Timestamp) -> Result<Self, InvalidTransition>
```

The binary reads the clock **once** per command, in one function:

```rust
// crates/tk/src/main.rs:369
/// The one clock read. Rounded to seconds so files stay readable.
fn clock() -> Timestamp {
    #[expect(
        clippy::disallowed_methods,
        reason = "the one clock read; everything else takes `now` as an argument"
    )]
    let now = Timestamp::now();
    now.round(jiff::Unit::Second).unwrap_or(now)
}
```

`clippy.toml` bans `Timestamp::now`, `SystemTime::now` and `getrandom::fill` everywhere, so this function has to *declare* that it's the exception.

Compare Laravel's `Carbon::setTestNow()` and `Str::random()` (global state you patch in tests), or Go's habit of passing a `clock` interface or `time.Time`. tk takes the Go approach to its logical end: the domain *can't* reach the clock at all.

### 14.2 `Ctx`: an explicit service container

```rust
// crates/tk-app/src/ctx.rs:12
/// Everything `execute` needs from outside: the projects, who is acting, the
/// time and a source of random bytes. The binary builds it; tests can fake it.
pub struct Ctx {
    pub stores: BTreeMap<ProjectKey, Store>,
    pub current: Option<ProjectKey>,
    pub actor: Actor,
    pub now: Timestamp,
    pub entropy: Box<dyn FnMut() -> io::Result<[u8; 4]> + Send>,
    pub warnings: Vec<String>,
}
```

Laravel resolves dependencies through the service container, implicitly. Here every dependency is a field you can see, and `execute(cmd, &mut ctx)` takes it explicitly.

---

## 15. Macros

### 15.1 Kinds you'll meet

| Kind | Looks like | Examples in tk |
| --- | --- | --- |
| function-like | `name!(...)` | `format!`, `vec!`, `matches!`, `write!`, `include_str!`, `env!`, `serde_json::json!` |
| derive | `#[derive(Name)]` | `Debug`, `Serialize`, `thiserror::Error`, `bon::Builder`, `clap::Parser` |
| attribute | `#[name(...)]` on an item | `#[nutype(...)]`, `#[tool_router]`, `#[tool(...)]` in `mcp.rs` |

Macros run at compile time and generate code. They're Rust's answer to Go's `go generate` and PHP attributes plus reflection, except the generated code is type-checked like hand-written code.

A few useful built-ins:

- `include_str!("guide.md")` embeds a file in the binary at compile time (`main.rs:55`), like Go's `//go:embed`.
- `env!("CARGO_BIN_EXE_tk")` reads a compile-time environment variable. Cargo sets this one in integration tests to the path of the built binary.

### 15.2 Reading a `macro_rules!`

Look again at `serde_via_str!` (11.4):

```rust
macro_rules! serde_via_str {
    ($($ty:ty),*) => {$(
        impl serde::Serialize for $ty { /* ... */ }
        impl<'de> serde::Deserialize<'de> for $ty { /* ... */ }
    )*};
}
```

- `$ty:ty` captures a *type* and names it `$ty`.
- `$( ... ),*` means "zero or more, separated by commas".
- On the right, `$( ... )*` repeats the block once per captured type.

So `serde_via_str!(TicketId, Actor)` writes both impls for both types. `schema.rs` uses the same technique (`string_schema!`) to write JSON Schemas for a table of text types.

When you need to see what a macro produces, `cargo expand` (installed with `cargo install cargo-expand`) prints the expanded code. Try it on `text.rs` to see everything nutype generates.

---

## 16. Concurrency and async, as tk uses them

tk is mostly single-threaded, but it uses enough concurrency to show the basics.

### 16.1 `Send`, `Sync` and "fearless concurrency"

The same ownership rules (one owner, either many `&` or one `&mut`) stop data races at compile time. Two marker traits extend them across threads:

- `Send`: the value can be **moved** to another thread.
- `Sync`: a `&` reference to it can be **shared** between threads.

The compiler implements them automatically when it's safe. You've seen `+ Send` on `Ctx.entropy`. It lets the context be moved into the MCP server's runtime.

### 16.2 Threads, `Arc` and a barrier (a test)

```rust
// crates/tk-store/src/lease.rs:270
let barrier = Arc::new(Barrier::new(2));
let racers: Vec<_> = ["a", "b"]
    .into_iter()
    .map(|name| {
        let (leases, id, barrier) = (leases.clone(), id.clone(), Arc::clone(&barrier));
        thread::spawn(move || {
            let actor = Actor::Agent(Name::try_new(name.to_owned()).unwrap());
            barrier.wait();
            leases.acquire(&id, &actor, now, until).map(Lease::keep).is_ok()
        })
    })
    .collect();
let wins = racers.into_iter().map(|r| r.join().unwrap()).filter(|&won| won).count();
assert_eq!(wins, 1);
```

- `thread::spawn(move || ...)` is `go func() { ... }()`, but the closure must own everything it uses (`move`) because the thread can outlive the current scope.
- `Arc<T>` is an atomically reference-counted pointer, the way to share one value between threads. `Arc::clone` only increments the count.
- `r.join()` waits for the thread and returns its result, like a `WaitGroup` plus a channel.

### 16.3 Channels, like Go's

The TUI's file watcher runs on a background thread and signals the UI thread through a channel:

```rust
// crates/tk/src/tui/mod.rs:33
let (tx, changes) = mpsc::channel();
// in the watcher callback (another thread):
if touched_ticket && tx.send(()).is_err() {
    // The TUI has quit; nothing is listening.
}
// in the event loop:
if changes.try_iter().count() > 0 {
    effects.extend(update(model, Msg::FilesChanged));
}
```

`mpsc` means multi-producer, single-consumer. `send` returns an error if the receiver is gone, where Go would panic on a closed channel or block forever. `try_iter()` drains without blocking, like a `select` with a `default` case.

### 16.4 `Mutex` and `OnceLock`

```rust
// crates/tk/src/mcp.rs:16
pub struct TkServer {
    ctx: Arc<Mutex<Ctx>>,
    clock: fn() -> Timestamp,
}

fn ctx(&self) -> Result<MutexGuard<'_, Ctx>, ErrorData> {
    let mut ctx = self
        .ctx
        .lock()
        .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
    ctx.now = (self.clock)();
    Ok(ctx)
}
```

In Rust **the mutex owns the data**. You can't touch the `Ctx` without locking, which is unlike Go's `sync.Mutex` sitting next to the fields it protects by convention. `lock()` returns a guard that unlocks on drop (`Drop` again). It returns a `Result` because a thread that panicked while holding the lock "poisons" it.

`OnceLock` is Go's `sync.Once`, used to run `git rev-parse` at most once per store (`store.rs:32`).

### 16.5 Async, briefly

`async fn` returns a *future*: a value describing work, which does nothing until something polls it. That's the main difference from goroutines. Rust has no built-in runtime, so you choose one. tk's MCP server builds a minimal single-threaded tokio runtime and blocks on the server:

```rust
// crates/tk/src/mcp.rs (serve)
let runtime = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()?;
runtime.block_on(async {
    let running = server.serve(stdio()).await /* ... */;
    running.waiting().await /* ... */;
    Ok(())
})
```

`.await` suspends until the future completes. The rest of tk is synchronous, and that's a valid choice: async is a tool for many concurrent I/O tasks, not a default.

Across *processes* (two agents running `tk`), tk uses OS file locks (`File::lock`) and atomic file creation (`persist_noclobber`), not language features.

---

## 17. Architecture patterns

### 17.1 One `Command`, one `execute`

```rust
// crates/tk-app/src/execute.rs:48
/// Runs one command. The CLI, `tk apply`, the TUI and later MCP all end up here.
pub fn execute(cmd: Command, ctx: &mut Ctx) -> Result<Outcome, AppError> {
    let now = ctx.now;
    match cmd {
        Command::New { project, title, kind, priority, labels, depends_on, body } => { /* ... */ }
        Command::Start { id, if_version } => { /* ... */ }
        Command::Block { id, if_version, reason } => moved(ctx, &id, if_version, Move::Block { reason }),
        // ... one arm per command, exhaustive
    }
}
```

Every interface turns its input into a `Command`:

- **CLI**: clap parses `argv` into `Cmd`/`WriteCmd`, and `main.rs::command()` converts that to `Command`.
- **`tk apply`**: one JSON line → `serde_json::from_str::<Command>`.
- **TUI**: a key press → `Effect::Run(Command)`.
- **MCP**: a tool call's JSON → `Command`.

That's the Laravel command bus (or Jobs) idea: a job class is data, a handler runs it. Here the "bus" is one exhaustive `match`, so a new command fails to compile until it's handled.

### 17.2 clap: the CLI as types

```rust
// crates/tk/src/cli.rs:13
#[derive(Parser, Debug)]
#[command(name = "tk", version, about = "Tickets as Markdown files in git: a TUI for you, a CLI for agents")]
pub struct Cli {
    /// Print JSON instead of text
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long = "as", global = true, env = "TK_ACTOR", value_name = "ACTOR", value_parser = parsed::<Actor>)]
    pub actor: Option<Actor>,
    #[command(subcommand)]
    pub cmd: Option<Cmd>,
}
```

It's Artisan's `$signature`, or cobra's flag definitions, but the result is a typed struct. The `///` doc comments become `--help` text.

### 17.3 Elm-style TUI: pure `update`, effects at the edge

```rust
// crates/tk/src/tui/model.rs:288
/// Pure: no I/O and no clock. Every write becomes an `Effect::Run`, which goes
/// through the same `execute` as the CLI.
pub fn update(model: &mut Model, msg: Msg) -> Vec<Effect> {
```

`Msg` is what happened (a key, files changed, a command finished). `update` changes the model and returns `Effect`s (run this command, open the editor, reload, quit). The runtime loop in `tui/mod.rs` is the only part that performs effects. This is the Elm architecture (also Redux, or Livewire's state → render cycle, minus the hidden server round-trips). It keeps almost all the UI logic pure and easy to reason about.

### 17.4 Store facts, derive views

`TicketJson` has a `waiting_on` field that is computed on every read from the dependencies' current states and never written to disk:

```rust
// crates/tk-app/src/query.rs:25
/// Dependencies that aren't done yet. Derived, never stored.
pub waiting_on: Vec<TicketId>,
```

Stored derived data goes stale, like a denormalized Eloquent counter column that someone forgot to update.

### 17.5 Atomic, durable writes

```rust
// crates/tk-store/src/store.rs:474
/// A synced temp file in the same directory, so the rename is atomic.
fn temp(&self, text: &str) -> Result<tempfile::NamedTempFile, StoreError> {
    let mut tmp = tempfile::NamedTempFile::new_in(&self.store.dir)?;
    tmp.write_all(text.as_bytes())?;
    tmp.as_file().sync_all()?;
    Ok(tmp)
}
```

Write to a temp file, `fsync` it, rename it over the target (atomic on POSIX), then `fsync` the directory (`sync_dir`). A crash leaves either the old file or the new one, never half of each. `persist_noclobber` makes the rename fail if the file exists, which is how two agents racing to create the same ID can't both win.

---

## 18. Testing

### 18.1 Unit tests live next to the code

```rust
// crates/tk-core/src/ids.rs:124
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_id_round_trips() {
        // ...
        assert_eq!(text.parse::<TicketId>(), Ok(id), "{text}");
    }
}
```

- `#[cfg(test)]` compiles the module only for `cargo test`.
- `use super::*` imports everything from the parent module, **private items included**. It's like Go's `_test.go` files in the same package.
- `assert_eq!(left, right, "message")` prints both sides on failure.

Bigger test suites move to their own file: `crates/tk-store/src/store.rs` ends with `#[cfg(test)] mod tests;`, which loads `store/tests.rs`.

### 18.2 Property tests

`proptest` generates random inputs and shrinks failures to a minimal case, like Go's fuzzing or PHP's Eris. `crates/tk-store/src/store/tests.rs` builds arbitrary valid tickets and checks that render → parse round-trips:

```rust
fn title() -> impl Strategy<Value = Title> {
    "[^\n\r]{1,120}".prop_filter_map("a valid title", |s| Title::try_new(s).ok())
}
```

A regex literal is a strategy for generating strings matching it.

### 18.3 Integration tests

Files in `crates/tk/tests/` are compiled as separate crates that use the binary from the outside:

```rust
// crates/tk/tests/merge_with_git.rs
type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[test]
#[ignore = "runs git init, commit and merge in a temporary directory"]
fn two_branches_change_status_and_append_logs_and_git_merges_them() -> Result<()> {
    let tk = env!("CARGO_BIN_EXE_tk");
    // ... runs real git and the real tk binary
    Ok(())
}
```

- A test can return `Result`. An `Err` fails it, so `?` works inside tests.
- `Box<dyn Error>` is "any error", handy in tests and scripts. Libraries prefer specific error enums.
- `#[ignore]` excludes a test unless you run `cargo test -- --ignored`.

### 18.4 The project's test policy

From `CLAUDE.md`: "Add a test only at a text, filesystem or git boundary." Everything else is covered by types. A Laravel project might have hundreds of tests asserting "you can't start a blocked ticket". Here that sentence doesn't compile, so it needs no test. Tests are kept for what types can't see: parsing real text, real file systems, real git, real concurrency.

---

## 19. Lints as guardrails

Clippy has hundreds of lints. tk turns on all of `pedantic` as warnings and denies a hand-picked set. `cargo lint` then turns every warning into an error.

| Lint | Why tk denies it |
| --- | --- |
| `unwrap_used`, `expect_used`, `panic`, `unreachable` | no crashes: return typed errors |
| `indexing_slicing`, `string_slice` | `v[i]` and `&s[a..b]` can panic |
| `wildcard_enum_match_arm` | `_ =>` hides new variants |
| `let_underscore_must_use` | no silently dropped `Result`s |
| `allow_attributes`, `allow_attributes_without_reason` | exceptions must be `#[expect]` with a reason |
| `unsafe_code = "forbid"` | no `unsafe` blocks anywhere |

**`#[expect]` vs `#[allow]`.** `#[allow(lint)]` silences a lint forever. `#[expect(lint, reason = "...")]` silences it *and errors if the lint stops firing*, so outdated exceptions get cleaned up:

```rust
// crates/tk-core/src/graph.rs:184
#[expect(
    clippy::result_large_err,
    reason = "the error hands the ticket back by value; Ok holds the same ticket, so boxing saves nothing"
)]
pub fn ready(&self, t: Ticket) -> Result<Ready, (Ticket, NotReady)> {
```

---

## 20. Reading order through the codebase

A path from simplest to richest, with what each file teaches:

| # | File | Teaches |
| --- | --- | --- |
| 1 | `crates/tk-core/src/fields.rs` | enums, derives, strum, `FromStr`/`Display` (Actor) |
| 2 | `crates/tk-core/src/text.rs` | nutype newtypes, the `Explain` trait, closures as predicates |
| 3 | `crates/tk-core/src/ids.rs` | hand-written newtypes, arrays, `FromStr`, the exhaustive test |
| 4 | `crates/tk-core/src/status.rs` | enums with data, the state machine `match` |
| 5 | `crates/tk-core/src/ticket.rs` | privacy, consuming methods, views, builder |
| 6 | `crates/tk-core/src/graph.rs` | collections, iterators, proofs (`Ready`), BFS/DFS |
| 7 | `crates/tk-store/src/version.rs` | small newtypes, manual serde |
| 8 | `crates/tk-store/src/error.rs` | thiserror, error design |
| 9 | `crates/tk-store/src/store.rs` | lifetimes, transactions, `Loaded`/`Edited`/`Saved`, atomic I/O |
| 10 | `crates/tk-store/src/lease.rs` | `Drop` guards, races, threads in tests |
| 11 | `crates/tk-store/src/format.rs` | string handling without slicing, `toml_edit` |
| 12 | `crates/tk-app/src/command.rs` | serde-tagged enums, JSON Schema |
| 13 | `crates/tk-app/src/error.rs` | error conversion and exhaustive mappings |
| 14 | `crates/tk-app/src/execute.rs` | putting it together, closures, borrow splitting |
| 15 | `crates/tk/src/cli.rs` and `main.rs` | clap, the edge of the program, injection |
| 16 | `crates/tk/src/tui/model.rs` | Elm architecture, `mem::replace` |
| 17 | `crates/tk/src/mcp.rs` | `Arc<Mutex>`, async, attribute macros |
| 18 | `crates/tk-store/src/merge.rs` | a bigger algorithm: three-way merge |

For each file: read the `//!` doc at the top, then the types, then the functions. In tk, the types tell most of the story.

---

## 21. Exercises

Do these in a scratch copy or a throwaway branch. Each one is a small change where **the compiler tells you what to do next.** Run `bacon` and watch the error list shrink.

### Exercise 1: The change that breaks nothing (5 min)

Add a `Docs` variant to `Kind` in `crates/tk-core/src/fields.rs` and run `cargo check --workspace --all-targets`.

**What happens:** it compiles with zero errors. Before reading on, explain why.

<details><summary>Answer</summary>

Nothing in tk matches on `Kind` by hand. Every use goes through derived code: strum's `Display`/`EnumString`/`VariantNames` (so `--kind docs` works in the CLI right away), serde, and `IntoStaticStr` for rendering. Derives regenerate themselves for the new variant. The compiler only makes you think about places where *decisions* are made.
</details>

### Exercise 2: Follow the compiler across crates (20 min)

Add `Obsolete` to `Resolution` in `crates/tk-core/src/status.rs` (between `WontDo` and `Duplicate`).

1. Run `cargo check --workspace`. The first error is in `crates/tk-store/src/format.rs` (`resolution_value`). Fix it so the file says `resolution = "obsolete"`.
2. Run again. Errors move outward to the next crate. Keep going until it compiles.
3. Add a `--obsolete` flag to `tk done` in `cli.rs` and handle it in `main.rs::resolution()`. Note how `conflicts_with` works on `--wont-do`.
4. Run `cargo lint`.

**Learn:** errors are reported crate by crate, innermost first. Each one is a decision only you can make.

### Exercise 3: A new rule, a new error variant (10 min)

Add `len_char_min = 3` to `Title`'s `validate(...)` in `crates/tk-core/src/text.rs`.

**What happens:** the `Explain for TitleError` `match` stops compiling. nutype added a `LenCharMinViolated` variant and the match must cover it. Write the message. Then update `schema.rs` so the JSON Schema says `"minLength": 3`. The compiler *can't* catch that one, which is why the schema strings are written next to each other in one table.

### Exercise 4: A new field change (20 min)

Add a `ClearLabels` variant to `FieldChange` in `crates/tk-app/src/command.rs`.

1. The compiler points at `crates/tk-app/src/execute.rs:314` (`fn edit`). Implement it. `BTreeSet` has a `clear()` method.
2. Add a `--clear-labels` flag to `EditArgs` in `cli.rs`, and chain it in `into_parts`. Hint: `clear_labels.then_some(FieldChange::ClearLabels)` gives an `Option`.
3. Try it through JSON: `{"op":"edit","id":"...","changes":["clear_labels"]}`. A unit variant serializes as a plain string.

### Exercise 5: Borrow-checker kata (15 min)

In `crates/tk/src/tui/model.rs`, replace `match std::mem::replace(&mut model.mode, Mode::Normal) {` with `match model.mode {`.

1. Read the E0507 error (section 4.6).
2. Apply the compiler's suggestion (`ref id`, ...) to one arm and read the *next* error.
3. Explain in one sentence why `mem::replace` is the right fix. Then revert.

### Exercise 6: A pure query in tk-core (30 min)

Add a method to `ValidGraph` in `crates/tk-core/src/graph.rs`:

```rust
/// Tickets that depend directly on `id`.
pub fn dependents<'a>(&'a self, id: &'a TicketId) -> impl Iterator<Item = &'a TicketId> {
    todo!()
}
```

Implement it with `self.nodes.iter().filter(...).map(...)`. Then experiment (each result below was checked against rustc 1.99):

- Write the filter as `|(_, n)| n.deps.contains(id)`. It compiles. Now write it as `|(_, n)| n.deps.iter().any(|d| d == id)`. Now you get **E0373, "closure may outlive the current function, but it borrows `id`"**, which is the same error `deps_of` in this file avoids with `move |&dep| ...`. The difference: `d == id` takes a reference to `id` (comparison borrows both sides), so the closure captures `&id`, a reference to a local variable that dies when the function returns. `contains(id)` copies the reference itself. `move` fixes both, because the closure then owns its copy of `id`.
- Remove `'a` from `id` (`id: &TicketId`). In **edition 2024** (this repo) it still compiles: a returned `impl Trait` automatically captures every lifetime in scope. Older editions needed the explicit `'a`, which is why you'll see it in a lot of Rust code online.

Note that `todo!()` is a warning, not an error, in this project (`todo = "warn"`), so you can sketch signatures first.

### Exercise 7: Write a trait impl by hand (15 min)

In a scratch file, write a tiny enum `Size { S, M, L }` with hand-written `FromStr` (accepting `"s"`, `"m"`, `"l"`) and `Display`, and a test that every variant round-trips. Then replace both with strum derives, as `fields.rs` does, and check the test still passes.

### Exercise 8: Read a real algorithm (30 min, reading only)

Read `cycles()` in `crates/tk-core/src/graph.rs:287`. It's an iterative depth-first search that keeps a **stack of iterators**. Explain:

- What does `stack.last_mut().map(Iterator::next)` return, and why `last_mut`?
- Why does `Some(dep) if finished.contains(&dep) => {}` come *before* `Some(dep) =>`?
- Why does `path` hold `&TicketId` instead of `TicketId`?

---

## 22. Compiler error field guide

| Code | Message (short) | Usual cause | Usual fix |
| --- | --- | --- | --- |
| E0382 | use of moved value | used a value after passing it by value | borrow (`&x`) instead, use the returned value, or `.clone()` if two owners are really needed |
| E0499 | cannot borrow as mutable more than once | two `&mut` alive at once | shorten one borrow's scope, or split the struct (4.5) |
| E0502 | cannot borrow as mutable because it is also borrowed as immutable | a `&` still in use when taking `&mut` | reorder, finish using the `&` first, destructure fields (4.5) |
| E0505 | cannot move out because it is borrowed | moved a value while a reference to it lives | drop the reference first, or clone |
| E0507 | cannot move out of ... behind a reference | taking ownership through `&`/`&mut` | `mem::replace`/`mem::take` (4.6), `.clone()`, or `Option::take()` |
| E0004 | non-exhaustive patterns | a new variant, or a missing case | handle it. Don't add `_` on domain enums |
| E0616 | field is private | touching a private field from another module | use the type's methods. The privacy is on purpose |
| E0423 | cannot initialize a tuple struct which contains private fields | building a newtype directly | use its constructor (`try_new`, a proof function) |
| E0521 | borrowed data escapes outside of closure | keeping a reference longer than its owner lives | return owned data instead |
| E0106 | missing lifetime specifier | returning a reference with unclear origin | add `'a` linking the output to one input |
| E0277 | the trait bound `X: Trait` is not satisfied | a type lacks a trait a function needs | derive or implement the trait, or import it |
| E0308 | mismatched types | often a stray `;` at the end of a block, or `&str` vs `String` | remove the `;`, or convert with `.to_owned()` / `&` |
| E0599 | no method named `x` found | often a trait not in scope | `use the::Trait as _;` (8.1) |

`rustc --explain <CODE>` prints a long explanation with examples.

---

## 23. Cheat sheet: Go / PHP → Rust

| Go | PHP / Laravel | Rust |
| --- | --- | --- |
| `x := 5` | `$x = 5;` | `let x = 5;` (`let mut` to change it) |
| `func f(a int) (int, error)` | `function f(int $a): int` (+ throws) | `fn f(a: i32) -> Result<i32, MyError>` |
| `if err != nil { return err }` | `try/catch` | `?` |
| `nil` | `null` | `Option::None` |
| `type T struct{...}` | `class T {...}` | `struct T {...}` + `impl T {...}` |
| methods with `(t *T)` | `$this` | `&self`, `&mut self`, `self` |
| `interface{ M() }` (implicit) | `interface I` (explicit) | `trait I { fn m(&self); }` + `impl I for T` |
| `any` / `interface{}` | `mixed` | generics `<T>`, `impl Trait`, or `Box<dyn Trait>` |
| `iota` consts | `enum Status: string` | `enum Status { Todo, Doing { by: Actor } }` |
| `switch` / type switch | `match` (PHP 8) | `match` (exhaustive, with patterns) |
| `defer f.Close()` | `finally` / `__destruct` | `Drop` (automatic) |
| `[]T` | `array` | `Vec<T>`, `&[T]` |
| `map[K]V` | assoc array | `HashMap<K, V>` / `BTreeMap<K, V>` |
| `string` | `string` | `&str` (borrowed) / `String` (owned) |
| `go f()` | queues | `thread::spawn(move \|\| f())`, or async + a runtime |
| `chan T` | — | `mpsc::channel::<T>()` |
| `sync.Mutex` next to data | — | `Mutex<T>` owning the data |
| `sync.Once` | — | `OnceLock` |
| `panic` | `throw` / `abort()` | `panic!` (denied here: return `Result`) |
| `go:generate` / reflection | attributes + reflection | macros (`#[derive]`, `macro_rules!`) |
| `json:"name,omitempty"` tags | API Resources / `$casts` | `#[serde(rename = ..., default)]` |
| `go:embed` | `file_get_contents` at boot | `include_str!` |
| `_test.go` | `tests/Unit` | `#[cfg(test)] mod tests` |
| `golangci-lint` | PHPStan / Larastan | `cargo clippy` |
| `internal/` | — | `pub(crate)` |
| `Carbon::setTestNow` / clock interface | — | pass `now: Timestamp` in |
| service container | `app()->make()` | an explicit `Ctx` struct |
| `DB::transaction(fn)` | — | `store.transaction(\|tx\| ...)` with a lifetime-bound `Tx` |

---

### Where to go next

- *The Rust Programming Language* ("the book"): https://doc.rust-lang.org/book/. Chapters 4 (ownership), 6 (enums), 9 (errors), 10 (traits and lifetimes) and 13 (closures and iterators) map onto sections 4–10 here.
- *Rust by Example*: https://doc.rust-lang.org/rust-by-example/
- "Parse, don't validate" by Alexis King, the essay behind section 12.
- `plan.md` in this repo, now that you can read the Rust in it.

The fastest way to get good at Rust is the loop this project is built around: **change a type, read every error, fix them one at a time.** After a few weeks the borrow checker stops feeling like an obstacle and starts feeling like a colleague who never gets tired of reviewing your code.
