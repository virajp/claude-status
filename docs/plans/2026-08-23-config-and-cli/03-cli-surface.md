---
type: vwf-plan
title: cli-surface — 2026-08-23
description: Cycle plan (a diff) reshaping the binary's flags — --refresh
  renamed, --configure added to wire Claude Code, --help documenting the repo
  layer, and --debug reporting a missing config as normal.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: [
  docs/plans/2026-08-23-config-and-cli/02-config-relocation.md,
]
timestamp: 2026-08-23T14:03:00Z
tags: [ cli, configure, settings, debug, help ]
---

# Plan: cli-surface — 2026-08-23

## Slice

Contract §5 (CLI surface). The binary's flag set becomes the whole product
surface, because after
[distribution/01](../2026-08-23-distribution/01-drop-npm.md) there is no
installer to carry any of it.

| Flag            | Does                                                                    |
| --------------- | ----------------------------------------------------------------------- |
| `--statusline`  | render the main bar — unchanged                                         |
| `--subagent`    | render the subagent panel — unchanged                                   |
| `--refresh`     | refresh the spend cache and exit — **renamed** from `--refresh-spend`   |
| `--caps-hook`   | unchanged                                                               |
| `--debug`       | report config, wiring and a sample render — **gains a no-config state** |
| `--version`     | print the version — unchanged                                           |
| `--help` / `-h` | full usage, **and the no-argument behaviour**                           |
| `--configure`   | **new** — wire Claude Code's `settings.json`                            |

## Current state (actual)

**The flags today** (`src/_runtime/cli.rs:84-90`): `--version`/`-V`, `--help`/
`-h`, `--debug`, `--statusline`, `--subagent`, `--refresh-spend`, `--caps-hook`.

**No-argument behaviour already matches**, per §5: TTY stdin → help; piped stdin
→ the one-line missing-flag error.

**`--version` is constrained by §5**: *"must be checked first and print nothing
but the version, because the installer distinguishes an installed binary from a
bundled one by the shape of that answer."* The installer is being deleted, so
that **reason** expires — the guarantee does not, and this cycle has to say why
it is kept rather than leave §5 asserting a rationale that no longer holds.

**The binary wires nothing.** All of it is in the npm installer:

- `installer/src/modules/settings.ts` (172 lines) — three keys, `statusLine`,
  `subagentStatusLine`, and the `PostToolUse` caps hook. It carries an
  `Ownership` type with four states (`absent`/`ours`/`ours-stale`/`foreign`)
  because "is this ours to change?" was a real question.
- **The three keys are not symmetric.** The first two are scalars. `PostToolUse`
  is a **list**, and the only key that *removes* an entry on uninstall — so it
  must be merged into and filtered out of, never replaced.
- `installer/src/modules/receipt.ts` (83 lines) — prior state, so uninstall
  restores rather than infers.

**`--debug` already reports the layers.** `debug_report_with`
(`src/_runtime/app.rs:321`) prints `CONFIG LAYERS (low to high)` with each
layer's label, load state and path, and it distinguishes the three reasons a
path can be absent (`<embedded>`, `<no $HOME>`, `<no git root>`). After
[plan 2](./02-config-relocation.md), "no config file at all" is the **normal**
case rather than a broken one, and the report still calls it `not found`.

## Target state (per contract)

`brew install` then `claude-status --configure` is the whole setup. `--help` is
where a user learns the repo layer exists. `--debug` describes a config-free
machine as working, not as missing something.

## Delta — ordered steps

### 1. Rename `--refresh-spend` to `--refresh`

A hard rename with no alias. Nothing has shipped, so an alias would be
compatibility with a version that never existed. `proc::spawn_detached` passes
this flag to its own child, so the caller in `app.rs` moves with it.

### 2. Add `--configure`

It writes three keys into `~/.claude/settings.json`:

- `statusLine` → `claude-status --statusline`
- `subagentStatusLine` → `claude-status --subagent`
- `PostToolUse` → the `claude-status --caps-hook` entry

**It overwrites.** An existing status line from another tool is replaced without
asking. That is the decision; it is not an accident, and steps 3 and 6 exist to
keep it from being a nasty surprise.

**The `PostToolUse` list is still merged, not replaced.** Overwriting the status
line keys is the decision; deleting another tool's unrelated hooks is not. Add
our entry, replace a previous entry of ours, leave everything else.

**Unrelated keys in `settings.json` are preserved.** The file holds far more
than these three, and it belongs to Claude Code.

### 3. Say what `--configure` will do, before it does it

Because there is no receipt and no undo, the destructive case must be visible:
if an existing `statusLine` or `subagentStatusLine` is present and is not ours,
print what is being replaced. `--dry-run` prints and writes nothing.

**No receipt, no `--unconfigure`.** Recorded as a decision, not an omission: the
flag overwrites, and a user who wants their old status line back sets it again.
The website documents this.

### 4. Seed an empty user config

`--configure` also creates `~/.config/claude-status/config.json` if absent,
containing `$schema` and nothing else — a starting point that an editor can
complete against.

**An existing config is never touched.** Not merged, not topped up, not
reordered.

### 5. Rewrite `--help`

It is now the only documentation that ships with the binary. It carries the flag
table, what `--configure` writes, the config paths, **how to write a repo-level
config** — the path, that `projectName` is its only key, and an example — and
the website URL.

The repo layer has no other discovery route after
[plan 2](./02-config-relocation.md) deleted the autoseed. If `--help` is vague
about it, the feature is gone in practice.

### 6. Make `--debug` treat "no config" as normal

`CONFIG LAYERS` distinguishes *absent* from *broken*. A missing user config
reads as using defaults, not as `not found`:

```text
CONFIG LAYERS (low to high)
  embedded loaded     <built in>
  user     using defaults  ~/.config/claude-status/config.json (no file)
  repo     using defaults  <no git root>
```

It also reports the ignored non-`projectName` repo keys from plan 2's step 4,
and — after [plan 4](./04-schema-and-validation.md) — the validation findings.

### 7. Docs

§5's table gains `--configure` and the rename. Its `--version` paragraph is
rewritten: the guarantee stays, and the reason becomes that `--version` is the
formula's `test do` assertion and the one output shape a script can rely on. §3
records the seeded-empty-config behaviour.

## Acceptance criteria (from contract)

1. Given a `settings.json` with unrelated keys, when `--configure` runs, then
   the three keys are set and every other key is byte-identical.
2. Given a `PostToolUse` list containing another tool's hook, when `--configure`
   runs, then our entry is present and theirs is untouched.
3. Given `--configure` run twice, then the second run leaves the file
   byte-identical to after the first.
4. Given a `statusLine` belonging to another tool, when `--configure` runs, then
   it is replaced **and** what was replaced is printed.
5. Given `--configure --dry-run`, then it prints its plan and no file on disk
   changes.
6. Given no `~/.config/claude-status/config.json`, when `--configure` runs, then
   one is created containing `$schema` alone; given an existing one, then it is
   byte-identical afterwards.
7. Given `--help`, then it names the repo config path, that `projectName` is its
   only key, and the website URL.
8. Given a machine with no config files, when `--debug` runs, then it reports
   defaults in use and does not describe the state as an error.
9. Given `--version`, then stdout is exactly the version — unchanged by anything
   in this cycle.
10. Given the repo after this cycle, when it is searched, then `--refresh-spend`
    appears nowhere.

## Risks / drift

**`--configure` edits a file it does not own, and now has no undo.** The receipt
that made this reversible is deliberately gone. Criteria 1–3 are the guard, and
they matter more than they look: a malformed `settings.json` breaks Claude Code
itself, not just the bar. Read-modify-write preserving unknown keys, and never a
regenerate-from-scratch.

**Criterion 3 — idempotence — is the one most likely to be quietly false.** A
hook list that grows by one entry per run is the classic failure, and it takes
three runs to notice.

**"Overwrite without asking" will generate the first support question.** A user
with an existing status line runs `--configure` and loses it, with nothing to
restore from. That is the accepted design, but it means step 3's printing is not
cosmetic — it is the entire mitigation, and it should be loud.

**Losing the receipt loses more than undo.** `receipt.ts` also recorded whether
a file existed before, which is how uninstall knew what to delete versus leave.
Nothing in this cycle needs that, but nothing can reconstruct it later either;
if an uninstall path is ever wanted, it starts from nothing.

**§5's `--version` rationale expires quietly.** The sentence stays true while
its stated reason stops being. Step 7 must actually rewrite it — a contract that
justifies a rule with a thing that no longer exists teaches the next reader that
the rule is vestigial.

## Out of scope for this cycle

- **An `--unconfigure` or any uninstall path.** Explicitly decided against.
  `brew uninstall` removes the binary; the `settings.json` keys are the user's
  to clear.
- **Generating a repo config.** Documented in `--help`, written by the user.
- **Schema validation in `--debug`.** [Plan 4](./04-schema-and-validation.md).
- **Deleting `installer/`.**
  [distribution/01](../2026-08-23-distribution/01-drop-npm.md), which is where
  the TypeScript this cycle reimplements actually gets removed.
- **A config-editing TUI.** The website's generator is that surface.

## Gaps surfaced during execution

*(filled in during execution)*
