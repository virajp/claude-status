+++
title = "Install"
description = "Get the binary, then wire Claude Code to it."
weight = 1
+++

Apple Silicon macOS. Three routes, and all of them end at the same wiring step.

## Homebrew

Three commands, and none of them is optional:

```sh
brew trust --formula virajp/tap/claude-status
brew tap virajp/tap
brew install --formula virajp/tap/claude-status
```

**Homebrew 6 will not load a third-party formula it has not been told to
trust**, so the install fails on its own. `brew trust` records the decision in
`~/.homebrew/trust.json` and you make it once, not per upgrade. Most tutorials
predate this and show a single `brew install`; that is why it does not work.

**`brew tap` is needed even though the formula is fully qualified.** The
qualified name is enough for `brew trust`, which is why the first command
succeeds without it — but `brew install` then reports it cannot find the
formula. Adding the tap explicitly is what fixes it, and the order above is the
order that works.

`brew upgrade` keeps it current from then on, and neither of the first two
commands is repeated.

## mise

If you already manage your tools with [mise](https://mise.jdx.dev), one command:

```sh
mise use --global "github:virajp/claude-status@latest"
```

That pulls the released binary straight from the GitHub release and puts it on
your `PATH`. `mise upgrade` moves it forward.

## npx

Pick the runner you already have — the arguments are identical:

```sh
npx  @virajp.dev/claude-status --install
pnpx @virajp.dev/claude-status --install
bunx @virajp.dev/claude-status --install
```

**The package is an installer, not the tool.** Nothing is installed globally: it
downloads the same released binary the other two routes use and puts it on your
`PATH`. `claude-status --configure` still does the wiring. Running the same
command again upgrades what it placed, printing the old version and the new.

It refuses anything but Apple Silicon macOS, and it refuses *before* it runs
rather than after downloading. It verifies what it downloads against a SHA-256
**pinned inside the published package** — not one fetched alongside the binary —
and a mismatch is fatal rather than something to retry.

**It will not touch a binary Homebrew or mise installed.** It recognises both,
prints that channel's upgrade command, and stops — so two channels never end up
fighting over the same file on your `PATH`.

The binary lands in a directory under your home that is already on your `PATH`,
`~/.local/bin` first. Never `/usr/local/bin`, never Homebrew's prefix. If
nothing qualifies it installs into `~/.local/bin` anyway, prints the line to add
to your shell, and exits non-zero.

It also writes a receipt to `~/.local/state/claude-status/install-receipt.json`.
That file is what makes the next run an upgrade rather than a refusal — it is
how the installer knows the binary on your `PATH` is one it placed. Delete it
and the next `--install` will decline to touch that binary without `--force`.

### The installer's flags

These are the installer's own, and they are not the binary's — the binary's
surfaces are further down.

| Flag          | Does                                            |
| ------------- | ----------------------------------------------- |
| `--install`   | place the binary on your `PATH`                 |
| `--uninstall` | remove it, and unwire `~/.claude/settings.json` |
| `--help`      | the flag list                                   |

Three modifiers pair with `--install`:

| Flag             | Does                                             |
| ---------------- | ------------------------------------------------ |
| `--configure`    | run `claude-status --configure` after installing |
| `--no-configure` | skip it, and print the command instead           |
| `--force`        | replace a binary this installer did not place    |

**With neither `--configure` nor `--no-configure`, it asks.** A script has no
TTY to be asked on, so there it skips silently — which is why `--no-configure`
exists at all: it lets a script decline as explicitly as it consents.

**Passing both is refused, not ranked.** A contradiction resolved silently is
how a script ends up doing the opposite of what it says.

## Then wire Claude Code to it

Every route ends here, and it is the step people skip:

```sh
claude-status --configure
```

Restart Claude Code and the bar is there. The npx installer will offer to run
this for you; brew and mise cannot.

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
> back. Run `claude-status --doctor` first if you want to see what is currently
> wired.

Want to see the changes without making them?

```sh
claude-status --configure --dry-run
```

`--dry-run` prints every change and writes nothing. Note that `--configure` is
the one surface that **refuses** an argument it does not recognise: every other
surface names the stray token and carries on, but here a typo in `--dry-run`
must not turn a preview into a real write.

## The surfaces

One binary, several surfaces, each selected by an explicit flag. They split by
who does the calling.

### Called for you

You will not type any of these, and `--help` does not list them.

The first three are wired by `--configure` and invoked by Claude Code. The
fourth is invoked by **the bar itself**: when the spend cache goes stale a
render spawns this same binary with `--refresh`, detached, and draws the cached
figure immediately rather than waiting. A render never fetches.

| Flag           | Called by   | Does                                                   |
| -------------- | ----------- | ------------------------------------------------------ |
| `--statusline` | Claude Code | render the main bar from a payload on stdin            |
| `--subagent`   | Claude Code | render the subagent panel from stdin (NDJSON)          |
| `--caps-hook`  | Claude Code | the `PostToolUse` cap actuator; silent unless breached |
| `--refresh`    | the bar     | refresh the spend cache and exit                       |

Typing `--refresh` yourself is the **same call the child makes**, not a stronger
one: it honours the same staleness rule, so on a fresh cache it does nothing,
and it prints nothing either way — it was built for a child whose output goes to
`/dev/null`. To force a fetch and see the result, use `--doctor`.

### You call these

| Flag          | Does                                             |
| ------------- | ------------------------------------------------ |
| `--configure` | wire Claude Code to this binary                  |
| `--doctor`    | report configuration, wiring and a sample render |
| `--version`   | print the version and exit                       |
| `--help`      | the flag list, and a link back here              |

Two modifiers pair with them: `--doctor` works on any surface and narrates to
stderr without changing a byte of stdout, and `--dry-run` pairs with
`--configure`.

> **`--doctor` was called `--debug`.** The old spelling is gone in both of its
> jobs — as the surface *and* as the modifier — so `--statusline --debug` now
> draws the bar with narration off rather than on. If you have `--debug` written
> into `~/.claude/settings.json` or a script, change it.

**An argument this binary does not recognise is named on stderr, with the full
help after it — and then every surface but `--configure` carries on.** A stray
token can never cost you a status bar and can never change a byte of stdout.
`--configure` refuses instead: it writes, there is no undo, and a typo in
`--dry-run` must not turn a preview into a real overwrite.

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
rather than installing something that cannot run. The npm package declares the
same requirement, so that route refuses too.

## Removing it

The npx route has an uninstaller:

```sh
npx @virajp.dev/claude-status --uninstall
```

That removes the binary and takes the `statusLine`, `subagentStatusLine` and
`PostToolUse` entries back out of `~/.claude/settings.json`, leaving another
tool's hooks in that array alone. **It refuses a binary it did not place**, so
it will not remove a Homebrew or mise install. Your
`~/.config/claude-status/config.json` is left where it is.

After brew or mise it is by hand: delete the binary, then remove those three
entries from `~/.claude/settings.json` yourself.
