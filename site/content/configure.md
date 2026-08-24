+++
title = "Configure"
description = "Three layers, one file of yours, and the keys you can set in it."
weight = 2
+++

## Three layers

Configuration is three layers, deep-merged low to high:

| Layer        | Path                                     | Who writes it                |
| ------------ | ---------------------------------------- | ---------------------------- |
| **defaults** | compiled into the binary                 | nobody — they ship with it   |
| **user**     | `~/.config/claude-status/config.json`    | you                          |
| **repo**     | `<repo-root>/.config/claude-status.json` | you, per repository, by hand |

Objects merge key by key. Arrays and scalars **replace wholesale**, so setting
`lines` in your config gets you exactly the layout you asked for rather than
yours appended to the default.

A layer that is missing, malformed or not a JSON object is ignored and the bar
still draws. Nothing here is fragile, and nothing here is required — with no
file anywhere at all, every default below is in effect.

The repo layer is a special case: it may set **one** key, and it is covered on
its own page — see [Per-repo](@/repo-config.md).

## Your file

Put only what you changed in it. Anything you leave alone follows the binary
forward when you upgrade.

```json
{
  "$schema": "https://raw.githubusercontent.com/virajp/claude-status/main/schemas/claude-status.schema.json",
  "lines": [
    ["model", "context", "rl5h", "rl7d", "spend", "cost"],
    ["project", "worktree", "branch"]
  ],
  "segments": {
    "cost": { "bg": "green", "bold": true }
  }
}
```

The `$schema` pointer is worth keeping: an editor that understands JSON Schema
will then complete the key names and reject the ones that do not exist. The
[schema itself](https://github.com/virajp/claude-status/blob/main/schemas/claude-status.schema.json)
is generated from the binary's own types, so it cannot drift from what the
binary accepts.

**Drawing the bar never writes to disk.** Whatever it needs, it reads. The only
thing a render writes is the spend cache under `~/.cache/claude-status/`, and
only from a background refresh. The one command that writes is `--configure`,
which you run yourself.

## The keys

| Key               | Is                                                                          |
| ----------------- | --------------------------------------------------------------------------- |
| `lines`           | ordered rows of segment entries — the layout. See [Segments](@/segments.md) |
| `segments`        | default styling per segment id                                              |
| `palette`         | named colours as `[r, g, b]` triples                                        |
| `defaultFg`       | foreground for any segment that does not set its own `fg`                   |
| `powerline`       | the divider glyphs: `cap`, `sep`, `sepThin`, `thinFg`                       |
| `gauge`           | the context meter: `width`, `filled`, `empty`                               |
| `symbols`         | the glyph drawn before each kind of value                                   |
| `typeSymbols`     | glyph per subagent `type`, with `_default` as the fallback                  |
| `caps`            | the four thresholds the `PostToolUse` hook measures against                 |
| `spend`           | `refreshMinutes` and `show` for the monthly-budget segment                  |
| `subagent`        | styling and the description budget for the subagent panel                   |
| `worktreePattern` | the regex that decides a checkout is a worktree                             |
| `projectName`     | **repo layer only** — see [Per-repo](@/repo-config.md)                      |

### Colours

A colour is one of three things, anywhere one is expected:

- a **palette name** — `"aqua"`, `"orange"`, or any key you added to `palette`
- a **hex string** — `"#d79921"` or `"#fa0"`
- an **RGB triple** — `[215, 153, 33]`

`null` clears a colour the defaults set, so that segment falls through to
`defaultFg`.

Styling resolves **inline override → `segments.<id>` → the built-in fallback**.
An entry in `lines` can therefore be a plain id, or an object that names one and
overrides its colours just for that position:

```json
{
  "lines": [
    ["model", { "name": "cost", "bg": "red", "bold": true }]
  ]
}
```

### Caps

The `--caps-hook` runs after each tool call and compares your usage to these.
Cross one and it injects a directive telling Claude to finish the current step,
write a handoff, and stop — once per escalation, so it will not nag.

```json
{
  "caps": {
    "context": 65,
    "fiveHour": 90,
    "sevenDay": 80,
    "spend": 90
  }
}
```

Those are the shipped values, as percentages. Set any one of them and the others
keep their defaults.

`spend` only ever fires on a seat that has a monthly budget, and it is checked
**before** the other three: a rate-limit window empties itself on a timer,
whereas an exhausted budget needs somebody to act. The figure comes from the
same cache the `spend` segment reads, so the hook never fetches.

A cap that is absent, negative, non-numeric or above 1000 falls back to its
shipped default. `0` is a real cap, meaning "breach on any usage at all".

### The spend segment

`spend` shows an account's **monthly budget**, and it is built for team and
enterprise seats whose limit is a spend cap rather than the rolling windows. On
a Pro or Max seat it stays hidden under the default `show: "auto"` — that is
working as intended, and it is much the most common reason you do not see it.

```json
{
  "spend": {
    "show": "auto",
    "refreshMinutes": 15
  }
}
```

`show: "always"` renders it whenever budget data exists — useful for watching an
extra-usage credit cap on a Pro or Max seat. `refreshMinutes` is the minimum gap
between fetches; `0` disables the background refresh entirely and the segment
then renders whatever the cache already holds.

Four gates can hide it, in order: it is not in your `lines`; there is no usable
cached figure yet; the account has no budget block; or the seat is one that
`auto` hides. [`--debug`](@/diagnosing.md) tells you which one applied.

**A render never fetches.** The figure comes from a cache at
`~/.cache/claude-status/spend.json`, refreshed in the background by a child
process the render never waits on.

## Environment variables

Three, and none is needed in normal use:

| Variable                    | Does                                                  |
| --------------------------- | ----------------------------------------------------- |
| `CLAUDE_STATUS_SPEND_CACHE` | override the spend cache path                         |
| `CLAUDE_STATUS_SPEND_URL`   | override the usage endpoint — for testing             |
| `CLAUDE_STATUS_USAGE_DIR`   | where the usage mirror the caps hook reads is written |
