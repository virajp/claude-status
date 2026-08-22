# Statusline for Claude Code

A fast powerline status line for Claude Code, in Rust. It renders in **1–2 ms**,
which matters because the bar redraws every four seconds in every open session.

![The claude-status bar: two powerline lines. The first shows the model and
effort, a context gauge at 259k/1M (26%), the 5-hour rate limit at 7.0% with
4h35m to reset, the 7-day limit at 1.0% with 5d1h to reset, and $46.51 of
session cost. The second shows the project name and the git branch with a dirty
marker.](https://raw.githubusercontent.com/virajp/claude-status/main/assets/statusline.png)

One binary, three surfaces, each chosen by a flag. Claude Code invokes these —
you do not run them by hand:

| Flag           | What it renders                                        |
| -------------- | ------------------------------------------------------ |
| `--statusline` | the main bar — two powerline lines                     |
| `--subagent`   | the subagent panel — NDJSON, one row per subagent      |
| `--caps-hook`  | a `PostToolUse` actuator; silent unless a cap breached |

## Requirements

**Apple Silicon Mac only.** Node 18+ is needed to run the installer once;
nothing after that.

Intel Macs are **not** served, and neither are Linux and Windows, both of which
Claude Code itself runs on. `npm install` refuses all three with `EBADPLATFORM`
rather than installing something that cannot work — the package declares its
`os` and `cpu` — and an install forced past that is stopped by the installer,
which names the platform it will not serve before touching anything.

## Install

```sh
npx @askviraj/claude-status --install
```

The installer puts the binary at `~/.claude/bin/claude-status`, seeds
`~/.config/claude-status.json` if you have no config, and wires three keys into
`~/.claude/settings.json`. The binary ships inside the package, so `--install`
needs no network access at all.

**The npm package is only ever the installer.** Claude Code is wired straight to
the binary — routing each render through a Node process would cost more in
startup than the render itself, several times a minute.

Everything the installer does, it does through one of these:

| Command       | Does                                                                    |
| ------------- | ----------------------------------------------------------------------- |
| `--install`   | place the binary, seed your config, wire Claude Code                    |
| `--uninstall` | remove it, restoring the `settings.json` keys it changed                |
| `--configure` | add a [repo-level config](#getting-a-repo-layer) to the repo you are in |
| `--help`      | the same list, with detail                                              |
| `--version`   | print the installed version                                             |

| Modifier    | Does                                                                    |
| ----------- | ----------------------------------------------------------------------- |
| `--dry-run` | report every change and touch nothing                                   |
| `--yes`     | answer prompts in advance — for a setup script or CI, which have no TTY |
| `--force`   | replace a status line this installer did not write, without asking      |

Replacing a status line the installer did not write **needs a yes**. With no
terminal to ask in and no `--yes`, the run fails rather than guessing in either
direction.

## Uninstall

```sh
npx @askviraj/claude-status --uninstall
```

It removes this installer's own files and restores the `settings.json` keys it
changed. The receipt at `~/.config/claude-status/receipt.json` records **prior
state**, so a status line you had before is put back verbatim, a config you
edited after installing is kept, and a key that was absent before ends up absent
rather than set to a default.

## Configuration

Three layers, deep-merged low to high:

1. the defaults **embedded in the binary** — so a machine with no config file
   still draws a full bar;
2. `~/.config/claude-status.json` — yours, seeded at install;
3. `<repo-root>/.config/claude-status.json` — per-repo overrides, which win.

A layer that is missing, malformed, or not a JSON object is ignored rather than
fatal. Objects merge key by key; arrays and scalars replace wholesale, so a repo
overriding `lines` replaces the layout rather than appending to it.

```jsonc
{
  "projectName": "my-project",
  "lines": [
    ["model", "context", "rl5h", "rl7d", "spend", "cost"],
    ["project", "worktree", "branch"],
  ],
  "segments": { "cost": { "bg": "green", "bold": true } },
}
```

Styling resolves **inline override → `segments.<id>` → hard fallback**. A colour
is a palette name, a `#rrggbb` string, or an `[r, g, b]` triple.

### Getting a repo layer

`npx @askviraj/claude-status --configure`, run from inside a repo, writes layer
3, seeding `projectName` from the repo's directory name. An existing file is
kept and only gains `projectName` if that key was missing.

You usually do not have to. `autoConfigureRepo` is **true by default**, so a
`--statusline` render in a repo with no layer 3 creates it by the same rules.
Set `"autoConfigureRepo": false` in layer 2 to opt out. Only `--statusline` ever
writes; `--subagent` and the caps hook stay read-only, and every failure is
silent, because stdout is the bar.

`projectName` is **repo-level only**. It ships in neither the embedded defaults
nor the seeded user config, so a repo that has not been configured omits the
`project` segment rather than inheriting a name that was never about it.

### Segments

| id         | Shows                            | Omitted when                           |
| ---------- | -------------------------------- | -------------------------------------- |
| `model`    | model and effort                 | never — falls back to `Claude`         |
| `context`  | a gauge, tokens used, percentage | never                                  |
| `rl5h`     | 5-hour rate limit and its reset  | no percentage in the payload           |
| `rl7d`     | 7-day rate limit and its reset   | no percentage in the payload           |
| `session`  | the session name                 | absent or empty                        |
| `cost`     | session cost                     | never — an absent cost renders `$0.00` |
| `spend`    | monthly budget                   | four gates, below                      |
| `duration` | session wall time                | `total_duration_ms` absent             |
| `project`  | `projectName` from config        | not set **in config**                  |
| `worktree` | the worktree sub-path            | not inside a worktree                  |
| `branch`   | branch, ahead and dirty markers  | no branch resolved                     |

An unknown id in `lines` warns on stderr and omits the segment; the render still
succeeds. stdout is only ever the bar.

### The spend segment, and why it is usually hidden

`spend` shows the account's monthly budget, and it exists for **team and
enterprise seats** whose limit is a spend cap rather than the 5-hour and 7-day
windows. On a Pro or Max seat under the default `show: "auto"` it stays hidden,
which is correct and is the most common reason it does not appear.

Four gates hide it, in order: it is not in your `lines`; there is no usable
cached figure; the account has no budget block; or the seat is one `auto` hides.
`claude-status --debug` performs a live fetch and names which gate applied.

**A render never fetches.** The figure comes from
`~/.cache/claude-status/spend.json`, refreshed by a detached child that the
render never waits on.

## Diagnosing

```sh
claude-status --debug
```

Reports the three config layers and which resolved, how Claude Code is wired,
the effective layout, the git facts, a sample render — and performs a live spend
fetch, naming the credential source, the HTTP status, the extraction and each of
the four gates. The access token appears on neither stream.

`--debug` is also a modifier: `--statusline --debug` narrates to stderr and
leaves stdout byte-identical.

## Environment

| Variable                    | Does                                                  |
| --------------------------- | ----------------------------------------------------- |
| `CLAUDE_STATUS_USAGE_DIR`   | where the usage mirror the caps hook reads is written |
| `CLAUDE_STATUS_SPEND_CACHE` | override the spend cache path                         |
| `CLAUDE_STATUS_SPEND_URL`   | override the usage endpoint — for testing             |

## Contributing

Building, testing and releasing are documented in
[CONTRIBUTING.md](https://github.com/virajp/claude-status/blob/main/CONTRIBUTING.md).

## Licence

MIT.
