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
- Track your work in this repo with tk: `tk next --claim`, `tk log`, `tk done`.
