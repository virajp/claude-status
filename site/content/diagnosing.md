+++
title = "Diagnosing"
description = "One command answers every question about why the bar looks the way it does."
weight = 5
+++

```sh
claude-status --debug
```

That is the whole troubleshooting story. It reports which config layers loaded
and which are simply absent, how Claude Code is wired, the layout in effect,
what git resolved, a live spend fetch with each of the four gates, and a sample
render.

It is also a **modifier**: add it to any other surface and it narrates to stderr
while leaving stdout byte-for-byte unchanged. `--statusline --debug` gives you
the same bar and an explanation of it.

Your access token never appears on either stream.

## What it prints

An illustrative report, assembled to show every section at once — the layer
paths are shortened to `~/…` for width, and the `ignored` row only appears when
a repo config actually carries a key beyond `projectName`. Your own output
prints fully expanded absolute paths:

```text
claude-status 0.1.0

CONFIG LAYERS (low to high)
  embedded loaded         <embedded>
  user     using defaults ~/.config/claude-status/config.json (no file)
  repo     loaded         /path/to/repo/.config/claude-status.json
           ignored        caps — a repo layer may set projectName only

CLAUDE WIRING (~/.claude/settings.json)
  ~/.claude/settings.json does not exist — run --configure to create it

EFFECTIVE LAYOUT
  line 0: model, context, rl5h, rl7d, spend, cost
  line 1: project, worktree, branch

GIT
  cwd:      /path/to/repo
  root:     /path/to/repo
  branch:   main
  worktree: <none>
  ahead:    false
  dirty:    +4 -0

SPEND
  cache    ~/.cache/claude-status/spend.json
           MISSING — first run
  backoff  none
  lock     free
  creds    NONE — checked ~/.claude/.credentials.json and keychain "Claude Code-credentials"
  fetch    GET https://api.anthropic.com/api/oauth/usage
           not attempted
  gate 1   spend present in lines                ✓
  gate 2   data present                          ✗ HIDDEN
  gate 3   enabled=?, limitMinor=?               — not reached
  gate 4   show=auto, plan=<none>                — not reached

  VERDICT  no credentials — neither ~/.claude/.credentials.json nor keychain
           "Claude Code-credentials" yielded a token. Log in with Claude Code,
           then re-run.

SAMPLE RENDER
  <the two lines, drawn with sample figures>
```

## Reading it

**`using defaults` is not an error.** A layer that has no file is the normal
case for both the user and the repo layer, and the line says `using defaults`
rather than warning about it. A layer that exists but could not be read says so
differently — that is the one to look at.

**`ignored` names dropped keys.** Only the repo layer can produce these, and it
means exactly what it says: the key was thrown away. See
[Per-repo](@/repo-config.md).

**`EFFECTIVE LAYOUT` is what will actually draw**, after all three layers have
merged. If a segment you configured is not in this list, the problem is your
`lines`, not the segment.

**`GIT` is resolved from the filesystem**, so `root: <none>` means you are not
inside a repository — and the `project`, `worktree` and `branch` segments will
all sit out, which drops the second row entirely.

**The four gates run in order** and stop at the first that hides the segment.
`gate 4  show=auto, plan=pro` is the common answer for "where is my spend
segment" and it is working correctly; see [Configure](@/configure.md).

**`SAMPLE RENDER` uses sample figures** for the session — the model, context,
rate limits and cost are fixed values, not your live session. The git facts and
the project name in it are real, because those come from where you are standing.

## Common answers

| Symptom                          | Usually                                                                     |
| -------------------------------- | --------------------------------------------------------------------------- |
| No bar at all                    | `--configure` has not been run, or the binary is not on your `PATH` by name |
| Tofu boxes instead of separators | the terminal font is not a Nerd Font                                        |
| Flat colours, no gradient        | the terminal is not in 24-bit colour mode                                   |
| Second row missing               | you are not inside a git repository                                         |
| No project name                  | that repository has no `projectName` — see [Per-repo](@/repo-config.md)     |
| No `spend` segment               | gate 4 on a Pro or Max seat, which is intended                              |
| A segment silently absent        | typo in its id — the note is on stderr                                      |

## Reporting something

If `--debug` does not explain it,
[open an issue](https://github.com/virajp/claude-status/issues) and paste its
output. It is designed to be pasteable — no token appears on either stream, and
`SAMPLE RENDER` carries sample figures rather than your session's. Do glance
over it first, though: `GIT`, `CONFIG LAYERS` and `SPEND` all print absolute
paths, so your home directory and the name of whatever you were working on go
with it.
