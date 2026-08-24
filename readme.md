# Statusline for Claude Code

Know what your Claude Code session is up to — at a glance, without breaking
flow.

`claude-status` draws a powerline bar under Claude Code with the things you
actually keep checking: which model you're on, how much context you've burned,
how close you are to your rate limits, what the session has cost, and which
branch you're standing on. It's written in Rust and renders in **1–2 ms**, so a
bar that redraws every four seconds in every open session never gets in the way.

![The claude-status bar: two powerline lines. The first shows the model and
effort, a context gauge at 259k/1M (26%), the 5-hour rate limit at 7.0% with
4h35m to reset, the 7-day limit at 1.0% with 5d1h to reset, and $46.51 of
session cost. The second shows the project name and the git branch with a dirty
marker.](https://cdn.virajp.me/claude-status/statusline.png)

## Why you might like it

- **Context you can feel.** A gauge, the token count and the percentage — so you
  know when you're getting close before Claude tells you.
- **Rate limits before they bite.** Both the 5-hour and 7-day windows, each with
  its reset time.
- **Cost as you go.** What this session has spent, live.
- **Monthly budget, for team and enterprise seats.** If your limit is a spend
  cap rather than the rolling windows, the `spend` segment puts the month's
  budget right next to the session cost — so the person accountable for it can
  see where things stand without opening a dashboard. It knows to stay out of
  the way on Pro and Max seats, where those windows are the real limit.
- **Guardrails that act, not just inform.** A `PostToolUse` hook watches your
  usage and, when you cross a cap, tells Claude to wrap up and hand off rather
  than letting a session run past the line. It ships at **65%** context, 90% of
  the 5-hour window, 80% of the 7-day one and 90% of a monthly budget — and all
  four are yours to change in your own config. A repo you cloned cannot raise
  them.
- **Your subagents, at a glance too.** The same binary draws a second panel for
  subagent sessions — a row each with the agent's name, model, what it's working
  on, tokens and elapsed time, and a status glyph that colours the row: running,
  done, errored, still queued. Styled from the same config as the main bar.
- **Git-aware.** Branch, worktree and dirty markers, resolved straight from the
  filesystem.
- **Make it yours.** Every colour, glyph, segment and row order is config, in
  one file that's yours. A repo can tell the bar what it's called, and nothing
  else — so a project you cloned can't change how your bar looks.
- **Quick, and quiet.** It never phones home on a render, and it only ever
  writes the bar to stdout.

## Install

> **There is no published install route yet.** `claude-status` used to be an npm
> package; that channel was retired before it ever shipped a real version,
> because it asked you for a Node toolchain to deliver a binary that needs none.
> Homebrew replaces it, and the tap is not published yet — until it is, build
> from source below.

Once the tap exists it will be two commands:

```sh
brew install virajp/tap/claude-status
claude-status --configure
```

`--configure` wires Claude Code's `~/.claude/settings.json` to the binary.
Restart Claude Code and the bar is there.

The bar draws in full from the defaults baked into the binary, so no config file
is needed at all — `~/.config/claude-status/config.json` is yours to write when
you want something different, and `--configure` creates it holding a `$schema`
pointer and nothing else.

Already have a status line? `--configure` **replaces** it and prints what it
replaced. There is no undo, so set yours again to get it back.

### From source

Works today, and needs only a Rust toolchain:

```sh
git clone https://github.com/virajp/claude-status
cd claude-status
cargo build --release
# put target/release/claude-status somewhere on your PATH, then:
claude-status --configure
```

### Requirements

**Apple Silicon Mac.** Nothing else — no Node, no runtime, no second language;
Claude Code talks to the binary directly.

Intel Macs, Linux and Windows aren't served today, and **nothing currently stops
you installing on one** — the npm manifest that used to refuse went with the npm
channel, and the Homebrew formula that will refuse again is not published yet.
See [Building it elsewhere](CONTRIBUTING.md#platform-support) if you want to try
anyway.

### The commands

| Command       | Does                                                       |
| ------------- | ---------------------------------------------------------- |
| `--configure` | wire Claude Code to this binary, and seed your user config |
| `--debug`     | report configuration, wiring and a sample render           |
| `--help`      | the full list, with detail                                 |
| `--version`   | print the version                                          |

`--dry-run` pairs with `--configure` to print every change and write nothing.

> **`--configure` used to mean the opposite.** The npm installer's flag of the
> same name gave the repo you were standing in a config layer and wrote nothing
> under `~`. The binary's `--configure` writes only under `~`, and the per-repo
> layer is now written [by hand](#per-repo-settings). The name was reused on
> purpose: the installer is gone and this is what replaces it.

## Uninstalling

Delete the binary, and remove the `statusLine`, `subagentStatusLine` and
`PostToolUse` hook entries `--configure` added to `~/.claude/settings.json`.

There is no `--unconfigure` and no receipt of what was there before. That is
deliberate rather than missing — but it does mean a status line `--configure`
replaced is not recoverable from anything this tool kept. Run
`claude-status --debug` to see exactly what is wired before you change it.

## Making it yours

Configuration is three layers, merged low to high:

1. **Defaults baked into the binary** — so a fresh machine draws a full bar with
   no config file at all. This is a supported state, not a fallback.
2. **`~/.config/claude-status/config.json`** — yours. Holds only what you
   changed, so anything you leave alone follows the binary forward when you
   upgrade.
3. **`<repo-root>/.config/claude-status.json`** — per repo. Sets `projectName`
   and nothing else; any other key in it is ignored, and `claude-status --debug`
   will tell you which.

Nothing here is fragile: a layer that's missing, malformed or not a JSON object
is simply ignored, and the bar still draws. Objects merge key by key. Arrays and
scalars replace wholesale, so overriding `lines` in your user config gets you
the layout you asked for rather than yours plus the default.

**Drawing the bar never writes to disk.** Whatever it needs, it reads. The only
thing a render writes is the spend cache under `~/.cache/claude-status/`, and
that only from a background refresh.

The one command that *does* write is `claude-status --configure`, which you run
yourself: it wires `~/.claude/settings.json` and creates
`~/.config/claude-status/config.json` if you have none. Nothing else this tool
does touches either.

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

Styling resolves **inline override → `segments.<id>` → built-in fallback**, and
a colour can be a palette name, a `#rrggbb` string or an `[r, g, b]` triple.

### Per-repo settings

Want one project to look different, or just to be named properly in the bar?
Write the file yourself — the whole thing is two keys:

```jsonc
{
  "$schema": "https://raw.githubusercontent.com/virajp/claude-status/main/schemas/claude-status.schema.json",
  "projectName": "widget-service",
}
```

Nothing creates this file for you on the render path. That used to happen, and
it meant the one command that runs every four seconds was also the one command
that wrote to your repo.

`projectName` lives **only** at the repo level, by design. It isn't in the
defaults or your user config, so a repo you haven't configured quietly omits the
`project` segment instead of wearing a name that was never about it.

### Caps, and what happens when you cross one

The `--caps-hook` watches your usage after each tool call. Cross a threshold and
it injects a directive telling Claude to finish the current step, write a
handoff, and stop — once per escalation, so it will not nag.

```jsonc
{
  "caps": {
    "context": 65, // percent of the context window
    "fiveHour": 90, // percent of the 5-hour rate-limit window
    "sevenDay": 80, // percent of the 7-day window
    "spend": 90, // percent of the monthly budget
  },
}
```

Those are the shipped values. Set any of them in your own config to change them
everywhere, or in a repo's config to change them just there — they resolve
through the same three layers as everything else, and a repo layer wins. Set one
key and the others keep their defaults.

`spend` only ever fires on a seat that has a monthly budget, and it is checked
before the other three: a rate-limit window empties itself, whereas an exhausted
budget needs somebody to act. The figure comes from the same cache the `spend`
segment reads, so the hook never fetches — just like a render never does.

### Segments

Mix and match these in `lines`:

| id         | Shows                            | Sits out when                          |
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

Typo a segment id and you get a note on stderr and a bar without it — never a
broken render. stdout is only ever the bar.

### Where's my spend segment?

`spend` shows an account's monthly budget, and it's built for **team and
enterprise seats** whose limit is a spend cap rather than the 5-hour and 7-day
windows. On a Pro or Max seat it stays hidden under the default `show: "auto"` —
that's working as intended, and it's much the most common reason you don't see
it.

Four gates can hide it, in order: it isn't in your `lines`; there's no usable
cached figure yet; the account has no budget block; or the seat is one `auto`
hides. Run `claude-status --debug` and it'll tell you exactly which one applied.

**A render never fetches.** The figure comes from a cache at
`~/.cache/claude-status/spend.json`, refreshed in the background by a child
process the render never waits on.

## Something not right?

```sh
claude-status --debug
```

One command tells you the whole story: which config layers loaded, which are
simply absent — that is normal, not a problem — and which are there but
unreadable, plus how Claude Code is wired, the layout in effect, what git
reported, a sample render — and a live spend fetch naming the credential source,
the HTTP status and each of the four gates. Your access token never appears on
either stream.

It doubles as a modifier, too: `--statusline --debug` narrates to stderr and
leaves stdout byte-for-byte unchanged.

### Under the hood

One binary, four surfaces. Claude Code invokes the first three for you — you
won't run those by hand. The fourth is the one you type:

| Flag           | What it renders                                        |
| -------------- | ------------------------------------------------------ |
| `--statusline` | the main bar — two powerline lines                     |
| `--subagent`   | the subagent panel — NDJSON, one row per subagent      |
| `--caps-hook`  | a `PostToolUse` actuator; silent unless a cap breached |
| `--configure`  | wires the three keys above into Claude Code's settings |

Add `--dry-run` to `--configure` to see every change without making one. Unlike
the render flags, `--configure` rejects an argument it does not recognise — a
typo in `--dry-run` must not turn a preview into a real write.

And a few environment variables, if you need them:

| Variable                    | Does                                                  |
| --------------------------- | ----------------------------------------------------- |
| `CLAUDE_STATUS_USAGE_DIR`   | where the usage mirror the caps hook reads is written |
| `CLAUDE_STATUS_SPEND_CACHE` | override the spend cache path                         |
| `CLAUDE_STATUS_SPEND_URL`   | override the usage endpoint — for testing             |

## Contributing

Ideas and fixes are welcome — building, testing and releasing are all written up
in
[CONTRIBUTING.md](https://github.com/virajp/claude-status/blob/main/CONTRIBUTING.md).

## Licence

MIT.
