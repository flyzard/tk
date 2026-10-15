# tk: tickets as Markdown files in git

Each project keeps its tickets in `.tickets/<id>.md`. tk changes files; you commit them.
Add `--json` to any command for machine-readable output, and `--as agent:<name>` (or set
`TK_ACTOR`) so your changes are attributed to you.

## Working loop

One ticket per session. Start from its brief, not from memory; end with a handoff.

    tk next --claim --brief       # take the top ready ticket, start it, print its brief
    tk brief                      # later: the brief of your doing ticket in this checkout
    tk log <id> "what you found"  # append to its ## Log as you work
    tk log <id> --ruled-out "..." # something that didn't work, so nobody tries it again
    tk log <id> --decided "..."   # a choice made, so nobody reopens it
    tk done <id>                  # finished; or hand off before your context fills up:
    tk handoff <id> --next "..." [--next ...] [--ruled-out ...] [--decided ...] [--release | --block "..."]

A brief is the ticket's text, the latest handoff's next steps (with how long ago, and how
many entries were logged since), everything ruled out and decided, and the latest notes.
A handoff keeps the ticket yours unless you release or block it; the next handoff replaces
its next steps. Hand off when you stop for any reason and the ticket isn't done.

## Rules

- Status: todo -> doing (start) -> done (done). block/unblock pause it, release puts it back
  to todo, reopen brings a done ticket back to todo. Anything else is refused.
- A ticket can only start when every ticket it depends on is done (`tk next` only offers those).
- Pass `--if-version <version>` from `tk show` or `tk brief --json` to any write; if someone else changed the
  ticket since, tk refuses with code "conflict" instead of overwriting their edit.
- Errors go to stderr as {"code","message","hint"}, with a distinct exit status per code.

## Parallel agents

- `tk start` and `tk next --claim` take a lease in the git directory all worktrees share,
  so two agents in two worktrees never start the same ticket (error code "claimed").
  Leases last 2 hours, renew on every write by their holder, and end on release, block or done.
- Ticket files merge field by field, and `## Log` lines from both branches are kept. Once per clone:
      git config merge.tk.driver "tk merge-driver %O %A %B %P"

## MCP

`tk mcp` serves the same commands over MCP on stdio: tools `run` (any command `tk schema`
describes), `next`, `list`, `show`, `brief`, `handoff`, `lint` and `guide`. Register it as a stdio server with
`TK_ACTOR=agent:<name>` in its environment, started from inside the project.

## Starting every session from the brief (Claude Code)

A SessionStart hook prints the brief into a new session's context, and again after
compaction, so the summary can't drop the plan. In `.claude/settings.json`:

    {
      "env": { "TK_ACTOR": "agent:claude" },
      "hooks": {
        "SessionStart": [{ "matcher": "startup|compact",
          "hooks": [{ "type": "command", "command": "tk brief" }] }]
      }
    }

A hook can't write the handoff for you: compaction gives you no turn. Hand off yourself.

## More

    tk new "Title" --kind bug --priority p1 --label export --dep app-a1b2 --body -
    tk ls --json                  # open tickets; --all includes done, --workspace all projects
    tk ls --fields id,state,title --json   # only the fields you need
    tk edit <id> --priority p0 --add-label urgent
    tk dep add <id> <other-id>    # cycle-checked
    tk lint                       # validate every ticket file; non-zero exit on problems
    tk apply < commands.jsonl     # one JSON command per line; `tk schema` describes them
