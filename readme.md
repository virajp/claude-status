# Statusline for Claude Code

A fast powerline status line for Claude Code, in Rust. It renders in **1–2 ms**
where the Node script it replaces took 30–50, which matters because the bar
redraws every four seconds in every open session.

```text
Opus 5 [high]   ▰▰▰▱▱▱▱▱▱▱ 259k/1M (26%)   7.0%  4h36m   $46.51
claude-status   main ↑ ±
```

One binary, three surfaces, each chosen by a flag:

| Flag           | What it renders                                        |
| -------------- | ------------------------------------------------------ |
| `--statusline` | the main bar — two powerline lines                     |
| `--subagent`   | the subagent panel — NDJSON, one row per subagent      |
| `--caps-hook`  | a `PostToolUse` actuator; silent unless a cap breached |

## Requirements

**macOS only** — Apple Silicon and Intel. Node 18+ is needed to run the
installer once; nothing after that.

Claude Code also runs on Linux and Windows, and this package does not serve
them. `npm install` there fails with `EBADPLATFORM` rather than installing
something that cannot work, and the installer names the platform it will not
serve before touching anything.

Building from source is the only escape hatch, and it is genuinely unsupported —
not a soft "we'd rather you didn't". On Linux, natively:

```sh
cargo build --release
# then point Claude Code's statusLine at target/release/claude-status
```

That is a normal native build with no cross-compilation, and it is the only
route anyone has a reason to try. Two things to know before you do. **Windows
will not compile** — the crate reaches for Unix APIs in three places (a
process-group call, the process-runner's test fixtures, and a `chmod` in the e2e
suite), so this is a small piece of work rather than a one-line `cfg`, and
fixing only the first still fails to build. And on Linux the TLS stack pulls in
`ring`, whose C code needs a working C toolchain — so the first failure you hit
is likely a missing compiler rather than anything Rust. Nothing outside macOS is
built, tested or checked in CI, so treat any of it as your own.

`supported_targets` in `.config/mise/tasks/_scripts/_rust` is where the platform
list lives, and its comment names everything a new platform has to touch.

## Install

```sh
npx @askviraj/claude-status --install
```

npm resolves the platform package for your Mac, and the installer puts the
binary at `~/.claude/bin/claude-status`, seeds `~/.config/claude-status.json` if
you have no config, and wires three keys into `~/.claude/settings.json`.

**The npm package is only ever the installer.** Claude Code runs the raw binary;
routing a render through a Node shim would pay Node's startup every four seconds
and give back everything the rewrite bought.

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

It restores what was there before rather than deleting and leaving you with
nothing. The receipt at `~/.config/claude-status/receipt.json` records **prior
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
| `AI_PLUGINS_USAGE_DIR`      | the previous name for it, still honoured              |
| `CLAUDE_STATUS_SPEND_CACHE` | override the spend cache path                         |
| `CLAUDE_STATUS_SPEND_URL`   | override the usage endpoint — for testing             |

**Moving from the `ai-plugins` statusline:** `$AI_PLUGINS_SPEND_CACHE` and
`$AI_PLUGINS_SPEND_URL` are **gone**, replaced by the `CLAUDE_STATUS_` names
above. `$AI_PLUGINS_USAGE_DIR` still works — it is read after
`$CLAUDE_STATUS_USAGE_DIR` and will be dropped once the JS bar is retired. The
installer moves `~/.config/statusline.json` to the new name, keeping your
theming, and offers to remove what the old install left behind.

## Building

```sh
mise run setup:all      # toolchain
mise run code:test      # the suite
mise run build:native   # a release binary for this machine
mise run build:all      # both published targets, then the npm packages
```

Both published targets are macOS, and Apple ships both slices of the system
libraries — so `build:all` on any Mac needs nothing but the two `rustup`
targets. There is no cross toolchain to install and no partial-build case to
handle. On a non-Mac host `build:cross` stops and says so rather than staging an
incomplete set.

`supported_targets` in `.config/mise/tasks/_scripts/_rust` is where the platform
list lives; see [contract §9](docs/spec/statusline-behaviour.md) for why it is
two rows.

Releases are tag-driven: bump `version` in `Cargo.toml` — the single source for
every published version — commit, and push a matching `v*` tag.

## Licence

MIT.
