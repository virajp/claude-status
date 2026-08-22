# Statusline for Claude Code

A fast powerline status line for Claude Code, in Rust. It renders in **1–2 ms**
where the Node script it replaces took 30–50, which matters because the bar
redraws every four seconds in every open session.

![The claude-status bar: two powerline lines. The first shows the model and
effort, a context gauge at 259k/1M (26%), the 5-hour rate limit at 7.0% with
4h35m to reset, the 7-day limit at 1.0% with 5d1h to reset, and $46.51 of
session cost. The second shows the project name and the git branch with a dirty
marker.](https://raw.githubusercontent.com/virajp/claude-status/main/assets/statusline.png)

One binary, three surfaces, each chosen by a flag:

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

Building from source is the only escape hatch, and it is genuinely unsupported —
see [Running it elsewhere](#running-it-elsewhere) if that is you.

## Install

```sh
npx @askviraj/claude-status --install
```

The installer downloads the binary for your Mac from its GitHub release, checks
it against the SHA-256 this package pins, and only then puts it at
`~/.claude/bin/claude-status`. It also seeds `~/.config/claude-status.json` if
you have no config, and wires three keys into `~/.claude/settings.json`.

**`--install` needs network access**, and that is the one thing it needs beyond
your own machine. A digest mismatch fails the install and writes nothing. Behind
a proxy, note that Node does not honour `HTTPS_PROXY` for this download — the
installer says so when it sees one set, and the fallback is to fetch the binary
from [the release](https://github.com/virajp/claude-status/releases) by hand and
drop it at `~/.claude/bin/claude-status`.

Why a download rather than an npm payload: release assets are mutable and npm
versions are not, so the digest lives in the immutable half and the bytes come
from the other. That keeps the trust root on npm while leaving one package to
publish instead of one per platform.

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

### Running it elsewhere

Unsupported, and not a soft "we'd rather you didn't". On Linux, natively:

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

## Uninstall

```sh
npx @askviraj/claude-status --uninstall
```

It removes this installer's own files and restores the `settings.json` keys it
changed. The receipt at `~/.config/claude-status/receipt.json` records **prior
state**, so a status line you had before is put back verbatim, a config you
edited after installing is kept, and a key that was absent before ends up absent
rather than set to a default.

A `statusline.json` the install migrated is **not** brought back. Uninstall
removes what belongs to this project and nothing else — reviving the JS bar's
config would hand you a file for a tool you no longer have.

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
3: it migrates a repo-level `statusline.json` if it finds one, and otherwise
seeds `projectName` from the repo's directory name. An existing file is kept and
only gains `projectName` if that key was missing.

You usually do not have to. `autoConfigureRepo` is **true by default**, so a
`--statusline` render in a repo with no layer 3 creates it by the same rules.
Set `"autoConfigureRepo": false` in layer 2 to opt out. Only `--statusline` ever
writes; `--subagent` and the caps hook stay read-only, and every failure is
silent, because stdout is the bar.

`projectName` is **repo-level only**. It ships in neither the embedded defaults
nor the seeded user config, so a repo that has not been configured omits the
`project` segment rather than inheriting a name that was never about it.

Migrating a `statusline.json` — at either level — rewrites rather than renames:
the old file points `$schema` at the `ai-plugins` repo, and a file kept under
that URL is validated against the wrong schema forever. Every other key is
carried across untouched, and the old file is deleted once the new one is on
disk. A legacy file that is not a JSON object has no key to set `$schema` on, so
it is discarded for a fresh config rather than carried across.

The one exception is `projectName`, and only when migrating the **user** layer:
the JS bar read it from that file, but here it is repo-level only, so keeping it
would name every repo you open after whichever one you set it in. It is dropped
rather than moved — the installer cannot know which repo it meant — and
`--configure` derives the right name from the repo you run it in.

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
installer migrates `~/.config/statusline.json` to the new name, keeping your
theming and repointing `$schema`, then adds any top-level keys the template has
and your file lacks. `projectName` is dropped, because it is repo-level here —
run `--configure` in a repo to set it there. It also offers to remove what the
old install left behind.

## Building

```sh
mise run setup:all         # toolchain
mise run code:test         # the suite
mise run build:statusline  # the bar, for both published targets
mise run build:installer   # the npm packages, staged into target/npm/
mise run build:all         # both of the above, in order
```

`build:statusline` builds every published target and smoke-tests the host slice
— the one this machine can actually execute; the other arch is proven the same
way on its own CI runner. Pass `--target <triple>` to build just one.

Both published targets are macOS, and Apple ships both slices of the system
libraries — so `build:all` on any Mac needs nothing but the two `rustup`
targets. There is no cross toolchain to install and no partial-build case to
handle. On a non-Mac host `build:statusline` stops and says so rather than
staging an incomplete set.

`supported_targets` in `.config/mise/tasks/_scripts/_rust` is where the platform
list lives; see [contract §9](docs/spec/statusline-behaviour.md) for why it is
two rows.

Releases are tag-driven: bump `version` in `Cargo.toml` — the single source for
every published version — commit, and push a matching `v*` tag.

## Licence

MIT.
