# Building tk

The design is in `plan.md`. The compiler is the to-do list.

## Checks

- `bacon` runs clippy on every save (`n` switches to the no_std check of tk-core).
- Before merging, both must pass with no warnings: `cargo lint && cargo lint-core`.
- `cargo test -p tk -- --ignored` runs the real-git merge test (it runs git init/commit/merge in a temp dir).
- `cargo-clippy` must come from rustup (`~/.cargo/bin`), not Homebrew, or `rust-toolchain.toml` and the bare-metal target are ignored.

## Rules

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
