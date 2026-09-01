<!-- Repo-relative, not the CDN the screenshot below uses: the site's copies are
     fingerprinted at build time, so their URLs change every release. Drawn at
     760px and shown at 380 so it stays crisp on a retina display. Regenerate
     with `.config/og-card.py`. -->
<img src="assets/lockup.png" alt="claude-status" width="380" />

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
marker.](assets/statusline.png)

## Install

Apple Silicon macOS, with Homebrew:

```sh
brew trust --formula virajp/tap/claude-status
brew tap virajp/tap
brew install --formula virajp/tap/claude-status
claude-status --configure
```

`brew trust` is not optional — Homebrew 6 will not load a third-party formula it
has not been told to trust, so the install fails without it. You do it once, not
per upgrade. **`brew tap` is not optional either**: the fully-qualified name is
enough for `brew trust`, but `brew install` reports it cannot find the formula
until the tap is added.

Or with [mise](https://mise.jdx.dev):

```sh
mise use --global "github:virajp/claude-status@latest"
claude-status --configure
```

Or, if you reach for `npx` first — `pnpx` and `bunx` take the same arguments:

```sh
npx @virajp.dev/claude-status --install --configure
```

Nothing is installed globally by that one: the package is an installer, and it
downloads the same released binary the other two routes get.

Or hand it to Claude Code — [install.md](install.md) is a runbook it can follow,
asking which route you want and offering to configure afterwards:

```text
Install claude-status by following https://claude-status.virajp.dev/install.md
```

`--configure` is the last step, not a formality — it's what wires Claude Code's
`~/.claude/settings.json` to the binary. Restart Claude Code and the bar is
there.

**[claude-status.virajp.dev](https://claude-status.virajp.dev)** has the rest:
all four routes in full, what `--configure` writes and what it replaces, and the
platforms that are and aren't served.

## Documentation

Everything user-facing lives on the site. This file is a pointer at it, on
purpose — a complete readme mirrored by a complete site is two documents that
disagree within a month, and the disagreement gets found by a user.

| Page                                                       | For                                                    |
| ---------------------------------------------------------- | ------------------------------------------------------ |
| [Install](https://claude-status.virajp.dev/install/)       | getting the binary, and wiring Claude Code to it       |
| [Configure](https://claude-status.virajp.dev/configure/)   | the three config layers, and what you can change       |
| [Generate](https://claude-status.virajp.dev/generate/)     | building a config file in the browser, from the schema |
| [Per-repo](https://claude-status.virajp.dev/repo-config/)  | naming one repository in the bar                       |
| [Segments](https://claude-status.virajp.dev/segments/)     | every segment, what it draws, and when it sits out     |
| [Diagnosing](https://claude-status.virajp.dev/diagnosing/) | `claude-status --doctor`, and the questions it answers |

Implementing against it? Behaviour is pinned by the test suite, and
[docs/decisions.md](docs/decisions.md) records why each decision was taken —
including the ones that were later reversed, and why they changed.

## Contributing

Ideas and fixes are welcome — building, testing and releasing are all written up
in [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence

MIT.
