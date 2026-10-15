# tk: tickets as Markdown files in git

Each project keeps its tickets in `.tickets/<id>.md`. tk changes files; you commit them.
Add `--json` to any command for machine-readable output, and `--as agent:<name>` (or set
`TK_ACTOR`) so your changes are attributed to you.

## Working loop

    tk next --claim --json        # take the top ready ticket and start it
    tk show <id> --json           # read it; note its "version"
    tk log <id> "what you found"  # append to its ## Log as you work
    tk done <id>                  # or: tk block <id> --reason "..."; tk release <id>

## Rules

- Status: todo -> doing (start) -> done (done). block/unblock pause it, release puts it back
  to todo, reopen brings a done ticket back to todo. Anything else is refused.
- A ticket can only start when every ticket it depends on is done (`tk next` only offers those).
- Pass `--if-version <version>` from `tk show` to any write; if someone else changed the
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
describes), `next`, `list`, `show`, `lint` and `guide`. Register it as a stdio server with
`TK_ACTOR=agent:<name>` in its environment, started from inside the project.

## More

    tk new "Title" --kind bug --priority p1 --label export --dep app-a1b2 --body -
    tk ls --json                  # open tickets; --all includes done, --workspace all projects
    tk ls --fields id,state,title --json   # only the fields you need
    tk edit <id> --priority p0 --add-label urgent
    tk dep add <id> <other-id>    # cycle-checked
    tk lint                       # validate every ticket file; non-zero exit on problems
    tk apply < commands.jsonl     # one JSON command per line; `tk schema` describes them
