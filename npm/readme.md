<!-- Image URLs are absolute and the site's, on purpose. See docs/decisions.md. -->

<img
  src="https://claude-status.virajp.dev/media/lockup.png"
  alt="claude-status"
  width="380"
/>

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
marker.](https://claude-status.virajp.dev/media/statusline.png)

## Install

```sh
npx @virajp.dev/claude-status --install --configure
```

`pnpx` and `bunx` take the same arguments.

**This package is an installer, not the tool.** Nothing is installed globally.
It downloads the released binary, puts it somewhere on your `PATH` under your
home directory, and `--configure` wires Claude Code to it. Restart Claude Code
and the bar is there.

Running it again upgrades what it placed.

### Or hand it to Claude Code

Paste this at Claude Code's prompt and it does the whole thing — installs
through this package, offers to wire Claude Code afterwards, and diagnoses with
`claude-status --doctor` if anything goes wrong:

```text
Install claude-status via npx by following https://claude-status.virajp.dev/install.md
```

Swap `npx` for `pnpx` or `bunx` in that line and it uses the runner you named.
Every step that writes anything is asked about first.

### Flags

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

With neither `--configure` nor `--no-configure` it asks. A script has no TTY to
be asked on, so there it skips silently — which is why `--no-configure` exists:
it lets a script decline as explicitly as it consents. Passing both is refused
rather than ranked.

### What it will and won't do

- **Apple Silicon macOS only**, and it refuses anything else before it runs.
- **Verifies what it downloads** against a SHA-256 pinned inside this package —
  not one fetched alongside the binary. A mismatch is fatal, not something to
  retry.
- **Never installs outside your home directory.** Not `/usr/local/bin`, not
  Homebrew's prefix.
- **Won't touch a binary Homebrew or mise installed.** It recognises both and
  tells you to upgrade through that channel instead.

## Other ways to install

`brew` and `mise` both work and end at the same place. All four routes are
written up on the site:

**[claude-status.virajp.dev/install](https://claude-status.virajp.dev/install/)**

## Documentation

Everything user-facing lives on the site.

| Page                                                       | For                                                    |
| ---------------------------------------------------------- | ------------------------------------------------------ |
| [Install](https://claude-status.virajp.dev/install/)       | getting the binary, and wiring Claude Code to it       |
| [Configure](https://claude-status.virajp.dev/configure/)   | the three config layers, and what you can change       |
| [Generate](https://claude-status.virajp.dev/generate/)     | building a config file in the browser, from the schema |
| [Per-repo](https://claude-status.virajp.dev/repo-config/)  | naming one repository in the bar                       |
| [Segments](https://claude-status.virajp.dev/segments/)     | every segment, what it draws, and when it sits out     |
| [Diagnosing](https://claude-status.virajp.dev/diagnosing/) | `claude-status --doctor`, and the questions it answers |

## Requirements

A Nerd Font in your terminal, and 24-bit colour — the separators, the branch
glyph and the gauge blocks are all font glyphs. Nothing else: no Node at
runtime, no second language.

Node 20 or newer is needed to run **this installer**, and not afterwards.

## Licence

MIT. Source at
[github.com/virajp/claude-status](https://github.com/virajp/claude-status).
