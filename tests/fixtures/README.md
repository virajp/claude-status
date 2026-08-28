# Reference payloads

The payloads the binary is demonstrated and tested against, as **files that can
be piped** rather than prose that can be transcribed wrong.

They used to live as shell examples in the behaviour contract's §12, and as
`const` strings inside `tests/e2e.rs` — three copies of the same JSON, one of
which documented **an invocation that does not work**. `tests/e2e.rs` now reads
these files with `include_str!`, so there is one copy and it is the one the
suite runs.

## The payloads

| File            | Invocation                   | Produces                                                  |
| --------------- | ---------------------------- | --------------------------------------------------------- |
| `main-bar.json` | `claude-status --statusline` | the powerline bar on stdout — **one line**, see below     |
| `subagent.json` | `claude-status --subagent`   | one line of NDJSON — `{"id": …, "content": …}` — per task |

**One line, not two.** The bar's default layout has a second line — `project`,
`worktree`, `branch` — and every one of those segments omits for this payload,
because its `current_dir` is `/tmp/demo` and that is not a git repository. A
line whose segments all omit is **dropped rather than printed blank**, so what
you get is the first line alone.

Running it from inside a checkout changes nothing — the payload pins
`current_dir`, and that wins over the process's own. Point that field at a real
checkout and the second line appears.

Piped, from the repository root:

```sh
# the main bar
claude-status --statusline < tests/fixtures/main-bar.json

# the subagent panel — NDJSON, so pipe through jq to see the rendered row
claude-status --subagent < tests/fixtures/subagent.json | jq -r .content
```

**`--statusline` is not optional, and this is the bug these files exist to
close.** §12's main-bar example piped to a bare `claude-status`, which resolves
to the missing-flag mode and prints

```text
claude-status: missing --statusline or --subagent (run --help)
```

— an error line, not a bar. Anyone who copied that example got the error and no
way to tell from the document that the example itself was wrong. The flag
chooses the **surface**, not the payload's shape: `subagent.json` piped without
`--subagent` renders the main bar, and `main-bar.json` piped without
`--statusline` renders nothing at all.

Both invocations, and the missing-flag case as a control, are executed by
`tests/e2e.rs`. A payload file added here without a row in the table above fails
that suite.

### What `main-bar.json` carries that §12's copy did not

`session_id`. §12 omitted it, so its payload exercised everything except the
**usage mirror** — which is keyed on exactly that field, and is a contract with
another repository. See
[the mirror's contract](../../docs/usage-mirror-contract.md).

## The spend diagnostics

Two invocations from §12 that take **no payload**. Kept here with the rest
because they are the same kind of thing: a reference command, and both carry a
warning worth not losing.

```sh
# spend, diagnosed — NOTE: this FETCHES, live, in the foreground.
# Both overrides matter: the cache one keeps it off ~/.cache/claude-status,
# and without the URL one it hits the real endpoint with your real token.
CLAUDE_STATUS_SPEND_CACHE=/tmp/spend.json \
  CLAUDE_STATUS_SPEND_URL=http://127.0.0.1:1/never \
  claude-status --doctor

# spend, against the real endpoint — one request, and it writes the real cache
claude-status --doctor
```

**`--doctor` as a mode fetches**, synchronously and in the foreground, and it
fetches even when your own config hides the spend segment. That is deliberate —
a passive report could not tell "you have it switched off" from "your token is
rejected" — but it means the second command above spends a real request against
a rate limit you wear for half an hour if you trip it. Run the first unless you
mean the second.
