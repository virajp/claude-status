+++
title = "claude-status"
description = "A powerline status line for Claude Code, in Rust."
template = "index.html"
+++

# Know what your Claude Code session is up to

`claude-status` draws a powerline bar under Claude Code with the things you
actually keep checking: which model you are on, how much context you have
burned, how close you are to your rate limits, what the session has cost, and
which branch you are standing on.

It is one Rust binary. No runtime, no Node, nothing to keep up to date — Claude
Code invokes it directly, and it starts in about a millisecond. Most of a render
is spent asking `git` about your working tree, so inside a repository the whole
thing costs tens of milliseconds rather than a couple; outside one it is barely
more than the process starting.

![Two powerline lines under a Claude Code prompt. The first carries the model
and effort, a context gauge with the token count and percentage, the 5-hour and
7-day rate-limit windows each with a reset countdown, and the session cost. The
second carries the project name and the git branch with a dirty
marker.](statusline.png)

## What it shows you

- **Context you can feel.** A gauge, the token count and the percentage — so you
  know you are getting close before Claude tells you.
- **Both rate-limit windows.** The 5-hour and the 7-day, each with the time
  until it resets.
- **Cost as you go**, and — on a seat whose limit is a monthly budget rather
  than the rolling windows — that budget beside it.
- **Your subagents.** The same binary draws a second panel: a row per subagent
  with its name, model, what it is working on, tokens, elapsed time and a status
  glyph. The status colours that first segment; the rest of the row keeps its
  own configured colours.
- **Git, from the filesystem.** Branch, worktree and dirty markers, resolved by
  reading the repository rather than shelling out for everything.

It also does one thing that is not display. A `PostToolUse` hook watches your
usage and, when you cross a cap, tells Claude to wrap up and hand off rather
than letting the session run past the line. The shipped caps are 65% of the
context window, 90% of the 5-hour window, 80% of the 7-day one and 90% of a
monthly budget — all four are yours to change, and a repository you cloned
cannot raise them past what you set. See [Configure](@/configure.md).

## Getting it

On Apple Silicon macOS, `brew install virajp/tap/claude-status` — the
fully-qualified form, which Homebrew 6 requires for third-party taps. On
anything else, build from source:

```sh
git clone https://github.com/virajp/claude-status
cd claude-status
cargo build --release
# put target/release/claude-status on your PATH, then:
claude-status --configure
```

`--configure` is the real second step, not a formality — it is what wires Claude
Code to the binary. [Install](@/install.md) covers both, including the Homebrew
route once the tap exists.

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
