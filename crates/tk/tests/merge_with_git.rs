//! Needs real git: two branches change one ticket and git merges them through `tk merge-driver`.
//!
//! Ignored by default because it runs git write commands (init, commit, merge),
//! all inside a temporary directory. Run it with `cargo test -p tk -- --ignored`.

use std::error::Error;
use std::path::Path;
use std::process::Command;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn run(dir: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(dir)
        .env("XDG_CONFIG_HOME", dir.join(".config"))
        .env("TK_ACTOR", "human:test")
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "{program} {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

#[test]
#[ignore = "runs git init, commit and merge in a temporary directory"]
fn two_branches_change_status_and_append_logs_and_git_merges_them() -> Result<()> {
    let tk = env!("CARGO_BIN_EXE_tk");
    let dir = tempfile::tempdir()?;
    let repo = dir.path();
    let git = |args: &[&str]| run(repo, "git", args);
    git(&["init", "--quiet", "--initial-branch=main"])?;
    git(&["config", "user.name", "test"])?;
    git(&["config", "user.email", "test@example.com"])?;
    git(&[
        "config",
        "merge.tk.driver",
        &format!("{tk} merge-driver %O %A %B %P"),
    ])?;
    run(repo, tk, &["init", "--key", "app"])?;
    let id = run(repo, tk, &["new", "Export misses credit notes"])?;
    git(&["add", "-A"])?;
    git(&["commit", "--quiet", "-m", "file the ticket"])?;

    git(&["checkout", "--quiet", "-b", "agent-a"])?;
    run(repo, tk, &["--as", "agent:a", "start", &id])?;
    run(
        repo,
        tk,
        &["--as", "agent:a", "log", &id, "reproduced on branch a"],
    )?;
    git(&["commit", "--quiet", "-am", "a starts"])?;

    git(&["checkout", "--quiet", "main"])?;
    run(
        repo,
        tk,
        &["--as", "agent:b", "edit", &id, "--priority", "p0"],
    )?;
    run(repo, tk, &["--as", "agent:b", "log", &id, "bumped on main"])?;
    git(&["commit", "--quiet", "-am", "b bumps"])?;

    git(&["merge", "--quiet", "--no-edit", "agent-a"])?;
    run(repo, tk, &["lint"])?;
    let text = std::fs::read_to_string(repo.join(".tickets").join(format!("{id}.md")))?;
    assert!(text.contains("state = \"doing\""), "{text}");
    assert!(text.contains("priority = \"p0\""), "{text}");
    assert!(
        text.contains("reproduced on branch a") && text.contains("bumped on main"),
        "{text}"
    );
    Ok(())
}
