# Statusline for Claude Code

Know what your Claude Code session is up to — at a glance, without breaking
flow.

`claude-status` draws a powerline bar under Claude Code with the things you
actually keep checking: which model you're on, how much context you've burned,
how close you are to your rate limits, what the session has cost, and which
branch you're standing on. One Rust binary, no runtime, nothing to keep up to
date.

![The claude-status bar: two powerline lines. The first shows the model and
effort, a context gauge at 259k/1M (26%), the 5-hour rate limit at 7.0% with
4h35m to reset, the 7-day limit at 1.0% with 5d1h to reset, and $46.51 of
session cost. The second shows the project name and the git branch with a dirty
marker.](https://cdn.virajp.dev/claude-status/statusline.png)

## Install

> **There is no published install route yet.** `claude-status` used to be an npm
> package; that channel was retired before it ever shipped a real version,
> because it asked you for a Node toolchain to deliver a binary that needs none.
> Homebrew replaces it, and the tap is not published yet.

Building from source is what works today, and needs only a Rust toolchain:

```sh
git clone https://github.com/virajp/claude-status
cd claude-status
cargo build --release
# put target/release/claude-status somewhere on your PATH, then:
claude-status --configure
```

`--configure` is the second step, not a formality — it's what wires Claude
Code's `~/.claude/settings.json` to the binary. Restart Claude Code and the bar
is there.

**[claude-status.virajp.dev](https://claude-status.virajp.dev)** has the rest:
the [Homebrew route](https://claude-status.virajp.dev/install/) for when the tap
exists, what `--configure` writes and what it replaces, and the platforms that
are and aren't served.

## Documentation

Everything user-facing lives on the site. This file is a pointer at it, on
purpose — a complete readme mirrored by a complete site is two documents that
disagree within a month, and the disagreement gets found by a user.

| Page                                                       | For                                                   |
| ---------------------------------------------------------- | ----------------------------------------------------- |
| [Install](https://claude-status.virajp.dev/install/)       | getting the binary, and wiring Claude Code to it      |
| [Configure](https://claude-status.virajp.dev/configure/)   | the three config layers, and what you can change      |
| [Per-repo](https://claude-status.virajp.dev/repo-config/)  | naming one repository in the bar                      |
| [Segments](https://claude-status.virajp.dev/segments/)     | every segment, what it draws, and when it sits out    |
| [Diagnosing](https://claude-status.virajp.dev/diagnosing/) | `claude-status --debug`, and the questions it answers |

The [behaviour contract](docs/spec/statusline-behaviour.md) is the source of
truth for implementers, and is not user documentation.

## Contributing

Ideas and fixes are welcome — building, testing and releasing are all written up
in [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence

MIT.
