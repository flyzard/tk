+++
title = "Rust toolchain: put ~/.cargo/bin before Homebrew on PATH"
kind = "chore"
priority = "p0"
labels = ["setup"]
depends_on = []
created = 2026-10-15T08:55:59Z
status = { state = "done", at = 2026-10-15T09:48:14Z, resolution = "completed" }
+++

Homebrew's cargo-clippy ignores rust-toolchain.toml and lacks the bare-metal target, so `cargo lint-core` fails. Run `brew uninstall rust` or fix PATH in ~/.zshrc and in VS Code.