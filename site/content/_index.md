+++
title = "claude-status"
description = "A powerline status line for Claude Code, in Rust."
template = "index.html"

# The hero's words live here rather than in the template, so the copy stays
# with the content and only the structure lives in `index.html`. Everything
# below the front matter is ordinary markdown and needs no template edit.
[extra]
headline = "Know what your Claude Code session is up to"
lede = "A powerline bar under Claude Code carrying what you actually keep checking: model, context, both rate-limit windows, session cost, and the branch you are on."
+++

## What it shows you

- **Context you can feel.** A gauge, the token count and the percentage — so you
  know you are getting close before Claude tells you.
- **Both rate-limit windows.** The 5-hour and the 7-day, each with the time
  until it resets.
- **Cost as you go.** The session's spend, and on a seat billed against a
  monthly budget rather than the rolling windows, that budget beside it.
- **Your subagents.** A second panel, a row per subagent, with its name, model,
  what it is working on, tokens, elapsed time and a status glyph.
- **Git, from the filesystem.** Branch, worktree and dirty markers, resolved by
  reading the repository rather than shelling out for everything.

It is one Rust binary. No runtime, no Node, nothing to keep up to date — Claude
Code invokes it directly, and it starts in about a millisecond. Most of a render
is spent asking `git` about your working tree, so inside a repository the whole
thing costs tens of milliseconds rather than a couple; outside one it is barely
more than the process starting.

## It can also stop a session running past the line

A `PostToolUse` hook watches your usage and, when you cross a cap, tells Claude
to wrap up and hand off rather than letting the session run on. The shipped caps
are 65% of the context window, 90% of the 5-hour window, 80% of the 7-day one
and 90% of a monthly budget — all four are yours to change, and a repository you
cloned cannot raise them past what you set. See [Configure](@/configure.md).

## Getting it

On Apple Silicon macOS:

```sh
brew install virajp/tap/claude-status
claude-status --configure
```

The fully-qualified form is the one that works — Homebrew 6 requires explicit
trust for third-party taps, so `brew tap` followed by `brew install` does not.
On anything else, build from source with `cargo build --release` and put the
binary on your `PATH`.

`--configure` is the real second step, not a formality: it is what wires Claude
Code to the binary. [Install](@/install.md) covers both routes in full.

## Where to go next

| Page                          | For                                                          |
| ----------------------------- | ------------------------------------------------------------ |
| [Install](@/install.md)       | getting the binary, and wiring Claude Code to it             |
| [Configure](@/configure.md)   | the three config layers, and what you can change             |
| [Per-repo](@/repo-config.md)  | naming one repository in the bar — the only per-repo setting |
| [Segments](@/segments.md)     | every segment, what it draws, and when it sits out           |
| [Diagnosing](@/diagnosing.md) | `--debug`, and the questions it answers                      |

Implementing against it, or wondering why something behaves the way it does? The
[behaviour contract](https://github.com/virajp/claude-status/blob/main/docs/spec/statusline-behaviour.md)
is the source of truth, written for implementers rather than users. These pages
are the user-facing documentation and do not restate it.
