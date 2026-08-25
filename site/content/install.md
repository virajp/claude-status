+++
title = "Install"
description = "Get the binary, then wire Claude Code to it."
weight = 1
+++

Apple Silicon macOS. Two routes, and both end at the same wiring step.

## Homebrew

Two commands, and the first is not optional:

```sh
brew trust --formula virajp/tap/claude-status
brew install --formula virajp/tap/claude-status
```

**Homebrew 6 will not load a third-party formula it has not been told to
trust**, so the install fails on its own. `brew trust` records the decision in
`~/.homebrew/trust.json` and you make it once, not per upgrade. Most tutorials
predate this and show a single `brew install`; that is why it does not work.

`brew upgrade` keeps it current from then on.

## mise

If you already manage your tools with [mise](https://mise.jdx.dev), one command:

```sh
mise use --global "github:virajp/claude-status@latest"
```

That pulls the released binary straight from the GitHub release and puts it on
your `PATH`. `mise upgrade` moves it forward.

## Then wire Claude Code to it

Both routes end here, and it is the step people skip:

```sh
claude-status --configure
```

Restart Claude Code and the bar is there.

### What `--configure` writes

Three keys in `~/.claude/settings.json`, each invoking `claude-status` by name
from your `PATH`:

```json
{
  "statusLine": {
    "type": "command",
    "command": "claude-status --statusline",
    "padding": 0,
    "refreshInterval": 4
  },
  "subagentStatusLine": {
    "type": "command",
    "command": "claude-status --subagent"
  },
  "hooks": {
    "PostToolUse": [
      {
        "hooks": [{ "type": "command", "command": "claude-status --caps-hook" }]
      }
    ]
  }
}
```

Every other key in that file is left as it was, and another tool's `PostToolUse`
hooks are kept alongside this one.

It also creates `~/.config/claude-status/config.json` if you have none, holding
a `$schema` pointer and nothing else. An existing one is never touched.

> **A status line belonging to something else IS replaced.** `--configure`
> prints what it replaced, and there is no undo — set yours again to get it
> back. Run `claude-status --debug` first if you want to see what is currently
> wired.

Want to see the changes without making them?

```sh
claude-status --configure --dry-run
```

`--dry-run` prints every change and writes nothing. Note that `--configure` is
the one surface that **refuses** an argument it does not recognise: every other
flag ignores a stray token, but here a typo in `--dry-run` must not turn a
preview into a real write.

## The surfaces

One binary, several surfaces, each selected by an explicit flag. They split by
who does the calling.

### Claude Code calls these

Wired by `--configure`, and you will not type them yourself.

| Flag           | Does                                                   |
| -------------- | ------------------------------------------------------ |
| `--statusline` | render the main bar from a payload on stdin            |
| `--subagent`   | render the subagent panel from stdin (NDJSON)          |
| `--caps-hook`  | the `PostToolUse` cap actuator; silent unless breached |

### You call these

| Flag          | Does                                             |
| ------------- | ------------------------------------------------ |
| `--configure` | wire Claude Code to this binary                  |
| `--refresh`   | refresh the spend cache and exit                 |
| `--debug`     | report configuration, wiring and a sample render |
| `--version`   | print the version and exit                       |
| `--help`      | the full list, with detail                       |

Two modifiers pair with them: `--debug` works on any surface and narrates to
stderr without changing a byte of stdout, and `--dry-run` pairs with
`--configure`.

**Every surface but `--configure` ignores an argument it does not recognise**,
so a stray token can never cost you a status bar.

## Nothing has to exist

With no config file anywhere, the bar renders in full from the defaults compiled
into the binary. That is a supported state, not a degraded one — see
[Configure](@/configure.md) when you want something different.

## What you need

**A Nerd Font in your terminal**, and 24-bit colour. The powerline separators,
the branch glyph and the gauge blocks are all font glyphs; without one you get
tofu where the seams should be. Nothing else is required — no Node, no runtime,
no second language.

**Apple Silicon macOS** is what is built and tested. The Homebrew formula
refuses anything else — it declares an `arm64` and a macOS requirement, so an
Intel Mac is told *"The arm64 architecture is required for this software"*
rather than installing something that cannot run.

## Removing it

Delete the binary, then remove the `statusLine`, `subagentStatusLine` and
`PostToolUse` hook entries from `~/.claude/settings.json`.
