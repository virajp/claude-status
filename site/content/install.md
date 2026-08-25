+++
title = "Install"
description = "Get the binary, then wire Claude Code to it."
weight = 1
+++

## Homebrew

Apple Silicon macOS. One command:

```sh
brew install virajp/tap/claude-status
```

**Use that fully-qualified form.** Homebrew 6 requires explicit trust for
third-party taps, so the two-step `brew tap virajp/tap` followed by
`brew install claude-status` fails until you also run `brew trust`. Most
tutorials still show the two-step version; this is the one that works.

`brew upgrade` keeps it current from then on.

It used to be an npm package. That channel was retired before it shipped a real
version, because it asked you for a Node toolchain in order to deliver a binary
that needs none.

## From source

For anything that is not Apple Silicon macOS, or if you would rather not use a
tap. Needs a Rust toolchain and nothing else.

```sh
git clone https://github.com/virajp/claude-status
cd claude-status
cargo build --release
```

That leaves the binary at `target/release/claude-status`. Put it somewhere on
your `PATH` — the wiring step below invokes it **by name**, so a binary that is
not on your `PATH` will not be found by Claude Code even though it works when
you run it yourself. Homebrew does this part for you.

Linux builds natively; Windows does not compile. Neither is tested or supported
— see the repository's `CONTRIBUTING.md` before you try.

## Then wire Claude Code to it

Both routes end at the same command, and it is the step people skip:

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

> **`--configure` used to mean the opposite.** The retired npm installer had a
> flag of the same name that gave the repository you were standing in a config
> layer and wrote nothing under `~`. The binary's `--configure` writes only
> under `~`, and the per-repo layer is now written [by hand](@/repo-config.md).
> The name was reused on purpose: the installer is gone and this is what
> replaces it.

## The surfaces

One binary, several surfaces, each selected by an explicit flag. Claude Code
invokes the first three for you — you will not run those by hand.

| Flag           | Does                                                   |
| -------------- | ------------------------------------------------------ |
| `--statusline` | render the main bar from a payload on stdin            |
| `--subagent`   | render the subagent panel from stdin (NDJSON)          |
| `--caps-hook`  | the `PostToolUse` cap actuator; silent unless breached |
| `--configure`  | wire Claude Code to this binary                        |
| `--refresh`    | refresh the spend cache and exit                       |
| `--debug`      | report configuration, wiring and a sample render       |
| `--version`    | print the version and exit                             |
| `--help`       | the full list, with detail                             |

Two modifiers pair with them: `--debug` works on any of the above and narrates
to stderr without changing a byte of stdout, and `--dry-run` pairs with
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

Building from source has no such gate, and nothing stops you trying.
`cargo build --release` may well work on your platform; it is simply not
something anyone checks.

## Removing it

Delete the binary, then remove the `statusLine`, `subagentStatusLine` and
`PostToolUse` hook entries from `~/.claude/settings.json`.

There is no `--unconfigure` and no receipt of what was there before. That is
deliberate rather than missing, but it does mean a status line that
`--configure` replaced is not recoverable from anything this tool kept.
