# Installing claude-status

**This file is a runbook for an agent.** Someone pointed their Claude Code at it
and asked for `claude-status` — the powerline status line for Claude Code — to
be installed on their machine. Work through it top to bottom.

It has three decision points, and every one of them belongs to the user, not to
you: which route to install by, whether to wire Claude Code afterwards, and
whether to apply any fix a diagnosis turns up. Ask, and wait for the answer.
Everything here writes under someone else's home directory.

Anything this file does not cover is on the site:
<https://claude-status.virajp.dev>.

## 0. Check the platform, and check for an existing install

```sh
uname -sm
```

Anything but `Darwin arm64` — stop, and say so plainly. Apple Silicon macOS is
what is built and tested; every route refuses the rest, and the npm route
refuses before it downloads rather than after.

```sh
claude-status --version
```

If that prints a version, this is an upgrade or a repair, not an install — skip
to [Already installed](#already-installed).

## 1. Ask which route

If the user's prompt already named one ("install it with brew", "use bunx"),
take them at their word and skip the question.

Otherwise find out what the machine actually has, so you offer only routes that
can work:

```sh
for c in brew mise npx pnpx bunx; do command -v "$c" >/dev/null && echo "$c"; done
```

Ask them to pick one of the ones that printed. The routes all leave the same
released binary on the `PATH` and differ only in who upgrades it afterwards:

| Route               | Upgrades with                       |
| ------------------- | ----------------------------------- |
| Homebrew            | `brew upgrade`                      |
| mise                | `mise upgrade claude-status`        |
| `npx`/`pnpx`/`bunx` | re-running the same install command |

If more than one is available, do not pick for them. Two channels putting the
same name on one `PATH` is precisely the mess the npm installer refuses to
create.

## 2. Run the route they picked

### Homebrew

Three commands, and none of them is optional:

```sh
brew trust --formula virajp/tap/claude-status
brew tap virajp/tap
brew install --formula virajp/tap/claude-status
```

`brew trust` comes first because Homebrew 6 will not load a third-party formula
it has not been told to trust, so the install fails on its own. `brew tap` comes
second because `brew install` reports it cannot find the formula without it,
fully-qualified name notwithstanding. Both are one-time; `brew upgrade` is all
that is needed from then on.

If one of these sits waiting for input, you have no TTY to answer on — hand it
back and ask the user to run it themselves by typing `! <command>` at their
prompt.

### mise

```sh
mise use --global "github:virajp/claude-status@latest"
```

### npx, pnpx, bunx

**Pass `--configure` or `--no-configure` explicitly, always.** With neither, the
installer asks — on a terminal. The shell you run in has no TTY, so it will skip
the wiring silently and you will report a success that left Claude Code unwired.
That means asking the user
[question 3](#3-ask-whether-to-wire-claude-code-to-it) *before* you run this,
and passing their answer:

```sh
npx @virajp.dev/claude-status --install --configure
```

`pnpx` and `bunx` take identical arguments. Passing both `--configure` and
`--no-configure` is refused rather than ranked, so send exactly one.

Nothing is installed globally by this route: the package is an installer, it
downloads the same binary the other two routes get, verifies it against a
SHA-256 pinned inside the published package, and puts it in `~/.local/bin` or
another directory under the user's home that is already on their `PATH`.

If it exits non-zero saying nothing on the `PATH` qualified, it still installed
into `~/.local/bin` and printed the line to add to the user's shell config. That
is a real failure of the install, and the fix is a change to their dotfiles —
see step 4, and get consent before editing anything.

## 3. Ask whether to wire Claude Code to it

Installing puts the binary on the `PATH`. It does not draw anything yet. This is
the step people skip:

```sh
claude-status --configure
```

Ask before running it. It writes three keys into `~/.claude/settings.json` —
`statusLine`, `subagentStatusLine`, and a `PostToolUse` hook — leaving every
other key, and any other tool's hooks, alone.

**A status line belonging to something else is replaced, and there is no undo.**
If the user has one — starship, a script of their own — say so first, and offer
the preview, which prints every change and writes nothing:

```sh
claude-status --configure --dry-run
```

**`--configure` is the one surface whose exit code means anything.** Every other
mode exits 0 whatever it found; this one exits 1 when it refuses, and a refusal
means nothing was written. Do not re-run it verbatim — go to step 4.

When it succeeds, tell the user to restart Claude Code. The bar appears in the
next session, not the one they are talking to you in.

If they decline, print the command for them and stop there. A decline is not a
failure.

## 4. When something goes wrong

Diagnose before you touch anything:

```sh
claude-status --doctor
```

**Read the report, not the exit code — `--doctor` always exits 0.** It reaches
the network for a live spend fetch, and prints six sections. Find the one that
covers what failed:

| Section            | Answers                                                        |
| ------------------ | -------------------------------------------------------------- |
| `CONFIG LAYERS`    | which config files were found, and what each one set           |
| `CLAUDE WIRING`    | what is in `~/.claude/settings.json` now, `<not set>` included |
| `EFFECTIVE LAYOUT` | the segments that would be drawn                               |
| `GIT`              | what the git segment can see from here                         |
| `SPEND`            | cache, backoff, lock, credentials, and a real fetch            |
| `SAMPLE RENDER`    | the bar itself                                                 |

`SPEND` ends in a `VERDICT` line that states in one sentence whether spend can
work and what is stopping it. Start there for anything cost-related.

The failures worth recognising:

| Symptom                                                 | Usually                                                           | Fix                                                                       |
| ------------------------------------------------------- | ----------------------------------------------------------------- | ------------------------------------------------------------------------- |
| `command not found` right after a successful install    | the install directory is not on the `PATH`                        | add the line the installer printed to their shell config — **their** file |
| no bar after a successful `--configure`                 | Claude Code has not been restarted                                | restart it                                                                |
| boxes or tofu where the separators should be            | no Nerd Font selected in the terminal                             | a terminal-app setting; hand it to the user                               |
| `--configure` exited 1 naming `~/.claude/settings.json` | that file is malformed, or a shape the merge cannot read          | show them the file, fix the JSON with consent, re-run                     |
| `CLAUDE WIRING` shows a command that is not this binary | something else owns the status line, or an old `--debug` spelling | `--configure` replaces it, but only with consent — there is no undo       |
| `SPEND` reports `creds NONE`                            | no credential was found in either place it checks                 | report it; nothing to fix on disk                                         |

**Propose, then wait.** Say what the report showed, say exactly what you would
change, and get a yes before changing it. Editing a user's shell config or their
`~/.claude/settings.json` on your own initiative is not a fix, whatever it
repairs.

## Already installed

Which channel owns the binary decides what an upgrade is:

```sh
command -v claude-status
mise which claude-status
```

- A `Cellar` path segment means Homebrew's — `brew upgrade claude-status`.
- `mise which` printing that same path means mise's —
  `mise upgrade claude-status`.
- Otherwise it is the npm installer's, and re-running
  `npx @virajp.dev/claude-status --install --no-configure` upgrades it in place,
  printing the old version and the new.

The npm installer makes this same check itself and stops rather than overwrite
another channel's binary, printing that channel's upgrade command instead. **Do
not reach for `--force` to get past that.** `--force` is for a binary the
installer placed but can no longer prove it placed — a deleted receipt at
`~/.local/state/claude-status/install-receipt.json` — not for taking a file away
from brew or mise.

To switch channels, uninstall the old one first. Two of them fighting over one
name on the `PATH` produces a version that changes depending on which tool ran
last.

## Uninstalling

The npx route has an uninstaller:

```sh
npx @virajp.dev/claude-status --uninstall
```

It removes the binary and takes those three keys back out of
`~/.claude/settings.json`, leaving another tool's `PostToolUse` hooks in that
array alone. It refuses a binary it did not place, so it will not remove a
Homebrew or mise install. `~/.config/claude-status/config.json` is left where it
is.

After brew or mise, removal is through that tool —
`brew uninstall claude-status`, or `mise use --global --remove claude-status` —
and then the three keys come out of `~/.claude/settings.json` by hand. That is
an edit to the user's settings: show them the keys you would remove, and ask.
