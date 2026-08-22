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
- **Git-aware.** Branch, worktree and dirty markers, resolved straight from the
  filesystem.
- **Make it yours.** Every colour, glyph, segment and row order is config. Per
  machine, and per repo when a project wants its own look.
- **Quick, and quiet.** It never phones home on a render, and it only ever
  writes the bar to stdout.

## Install

```sh
npx @askviraj/claude-status --install
```

That's it. The binary lands at `~/.claude/bin/claude-status`, you get a starter
config at `~/.config/claude-status.json`, and Claude Code gets wired up for you.
The binary ships inside the package, so the install needs no network access at
all — it works on a plane.

Restart Claude Code and the bar is there.

Already have a status line? The installer won't stomp on it without asking, and
`--uninstall` puts your old one back exactly as it was.

### Requirements

**Apple Silicon Mac.** You'll also need Node 18+ to run the installer once —
nothing after that, because Claude Code talks to the binary directly rather than
through Node.

Intel Macs, Linux and Windows aren't served today. `npm install` will politely
refuse rather than leave you with something that can't work.

### The commands

| Command       | Does                                                               |
| ------------- | ------------------------------------------------------------------ |
| `--install`   | place the binary, seed your config, wire Claude Code               |
| `--uninstall` | remove it, and restore the `settings.json` keys it changed         |
| `--configure` | give the repo you're in its own [config layer](#per-repo-settings) |
| `--help`      | the same list, with detail                                         |
| `--version`   | print the installed version                                        |

Add these to any of them:

| Modifier    | Does                                                                         |
| ----------- | ---------------------------------------------------------------------------- |
| `--dry-run` | show every change and touch nothing                                          |
| `--yes`     | answer prompts in advance — handy in a setup script or CI, which have no TTY |
| `--force`   | replace a status line this installer didn't write, without being asked       |

One thing to know if you're scripting it: replacing a status line the installer
didn't write needs a yes, and with no terminal to ask in it stops rather than
guessing. Pass `--yes` or `--force` and it'll go ahead.

## Uninstalling

```sh
npx @askviraj/claude-status --uninstall
```

It takes back what it added and nothing else. A receipt at
`~/.config/claude-status/receipt.json` remembers how your machine looked
**before** the install, so the status line you had returns verbatim, a config
you've since edited is left alone, and a setting that was absent goes back to
being absent rather than set to some default.

## Making it yours

Configuration is three layers, merged low to high:

1. **Defaults baked into the binary** — so a fresh machine draws a full bar with
   no config file at all.
2. **`~/.config/claude-status.json`** — yours, created at install.
3. **`<repo-root>/.config/claude-status.json`** — per-repo overrides, which win.

Nothing here is fragile: a layer that's missing, malformed or not a JSON object
is simply ignored, and the bar still draws. Objects merge key by key. Arrays and
scalars replace wholesale, so a repo that overrides `lines` gets the layout it
asked for rather than yours plus its own.

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

Want one project to look different, or just to be named properly in the bar? Run
this inside it:

```sh
npx @askviraj/claude-status --configure
```

That writes layer 3 and names the project after its directory. An existing file
is kept as-is and only gains `projectName` if it was missing.

Mostly you won't need to. `autoConfigureRepo` is **on by default**, so the first
render inside a new repo creates that layer for you by the same rules. Set
`"autoConfigureRepo": false` in layer 2 if you'd rather it never happened. Only
the main bar ever writes — the subagent panel and the caps hook are strictly
read-only — and if writing fails for any reason the render carries on without
complaint, because stdout belongs to the bar.

`projectName` lives **only** at the repo level, by design. It isn't in the
defaults or your user config, so a repo you haven't configured quietly omits the
`project` segment instead of wearing a name that was never about it.

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

One command tells you the whole story: which config layers were found and which
won, how Claude Code is wired, the layout in effect, what git reported, a sample
render — plus a live spend fetch naming the credential source, the HTTP status
and each of the four gates. Your access token never appears on either stream.

It doubles as a modifier, too: `--statusline --debug` narrates to stderr and
leaves stdout byte-for-byte unchanged.

### Under the hood

One binary, three surfaces. Claude Code invokes these for you — you won't run
them by hand:

| Flag           | What it renders                                        |
| -------------- | ------------------------------------------------------ |
| `--statusline` | the main bar — two powerline lines                     |
| `--subagent`   | the subagent panel — NDJSON, one row per subagent      |
| `--caps-hook`  | a `PostToolUse` actuator; silent unless a cap breached |

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
