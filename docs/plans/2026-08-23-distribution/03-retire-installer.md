---
type: vwf-plan
title: retire-installer — 2026-08-23
description: Cycle plan (a diff) moving the wiring, config seeding and receipt
  out of the npm installer and into the binary, then retiring the npm package.
  Blocked on maintainer confirmation that the tap is proven.
status: blocked
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: [
  docs/plans/2026-08-23-distribution/02-homebrew-formula.md,
]
timestamp: 2026-08-23T10:13:00Z
tags: [ distribution, installer, homebrew, npm, rust, deprecation ]
---

# Plan: retire-installer — 2026-08-23

> ## ⛔ Do not execute this plan yet
>
> It runs when **the maintainer confirms the Homebrew tap is proven in real
> use** — not when [plan 2](./02-homebrew-formula.md) merges, and not when the
> tap looks finished.
>
> This plan removes the only install path that has ever worked. A plan that cuts
> the working path because its replacement looks ready is the specific failure
> this gate exists to prevent. The gate is a person, and the person is the
> maintainer.

## Slice

Contract §9 (Distribution) and §5 (CLI surface). Everything the npm installer
does moves into the binary as install/uninstall/configure subcommands, and the
npm package is deprecated.

**This is not a deletion.** Roughly 85% of the cycle is porting TypeScript to
Rust; the deletion is the last step and the easy one.

## Current state (actual)

`installer/src/` is **1,783 lines of TypeScript** with **1,204 lines of tests**
in `installer/test/installer.test.mjs`. What it does, beyond placing a binary:

| Module                                                | Lines | Owns                                                                       |
| ----------------------------------------------------- | ----- | -------------------------------------------------------------------------- |
| `install.ts`                                          | 327   | the install flow, prompts, orphan sweep                                    |
| `config.ts`                                           | 263   | seeding from `assets/claude-status.defaults.json`, migration, `SCHEMA_URL` |
| `uninstall.ts`                                        | 197   | restoring from the receipt, edited-since-install guards                    |
| `settings.ts`                                         | 172   | the three `settings.json` keys and ownership detection                     |
| `receipt.ts`                                          | 83    | the record of prior state                                                  |
| `configure.ts`                                        | —     | the repo-level layer                                                       |
| `paths.ts`, `binary.ts`, `repo.ts`, `io.ts`, `cli.ts` | —     | plumbing                                                                   |

**The three wired keys are not symmetric.** `statusLine` and
`subagentStatusLine` are scalar keys this installer sets and restores. The
`PostToolUse` caps hook is a **list**, and it is the only key that *removes* an
entry on uninstall — so it must be merged in and filtered out, never replaced.
`settings.ts` has an `Ownership` type (`absent` / `ours` / `ours-stale` /
`foreign`) precisely because "is this ours to change?" is a real question with
four answers.

**The receipt records prior state, not inferred state.** The reasoning is
written at the top of `receipt.ts` and is the single most important thing to
carry across: deducing on uninstall what an install *must* have written is safe
right up until the user edits a value, at which point it deletes something it
never created.

**The binary installs nothing.** `--statusline`, `--subagent`,
`--refresh-spend`, `--caps-hook`, `--debug`, `--version`, `--help`.

**§5 constrains what can be added.** *"`--version` must be checked first and
print nothing but the version, because the installer distinguishes an installed
binary from a bundled one by the shape of that answer."* If the installer goes
away, that distinction loses its only consumer — which does not make it safe to
change, but does mean the reason recorded in §5 stops being the reason.

**`--configure` is the least affected.** `src/modules/config/autoseed.rs`
already seeds the repo layer from the render path, so the repo config survives
the npm package going away regardless of what this cycle does.

## Target state (per contract)

`brew install virajp/tap/claude-status && claude-status --install` is the whole
install. The binary wires Claude Code, seeds the config, writes the receipt, and
can undo all of it. The npm package is deprecated on the registry, pointing at
the tap.

## Delta — ordered steps

### 1. Confirm the gate, in writing

Record in this plan that the maintainer has confirmed the tap is proven, with a
date. If that line is absent, the cycle has not started.

### 2. Port the receipt

First, because everything else writes to it and because it is the part with the
clearest existing reasoning. Same on-disk format — a receipt written by the npm
installer must be readable by the binary, or an existing user's uninstall
silently stops working.

### 3. Port `settings.json` wiring

`Ownership`'s four states, the two scalar keys, and the hook list with its
merge-and-filter semantics. This is the highest-risk module: it edits a file the
user did not create, that other tools also write to, and that breaks Claude Code
if malformed.

Read it, write it, and preserve unknown keys **byte-for-byte where possible** —
a user's `settings.json` holds far more than these three keys.

### 4. Port config seeding and migration

Seeding from `assets/claude-status.defaults.json` byte-for-byte, and the
`statusline.json` migration with its `$schema` repoint. `autoseed.rs` already
has the repo-layer half of this and its rules are documented as matching the
installer's — reuse rather than duplicate.

### 5. Add the subcommands

`--install`, `--uninstall`, `--configure`, and the `--dry-run` / `--yes` /
`--force` modifiers the npm installer already has.

**§5's `--version` rule holds unchanged.** Whatever else is added, `--version`
is checked first and prints a bare version. Its stated reason expires with the
installer; the guarantee does not, and §5 should be amended to say why it is
kept rather than left asserting a reason that no longer holds.

### 6. Port the tests

The 1,204-line suite is the specification for all of the above — it encodes the
edge cases (foreign status lines, stale ownership, edited configs, orphan
sweeps) that the prose does not. Port it before deleting it.

**A ported test that was weakened to pass is worse than no test**, because it
launders a regression as coverage. Any test that could not be carried across
faithfully is a Gaps entry.

### 7. Update the formula's caveats

Plan 2's caveats tell the user to run an npx command. They now tell them to run
`claude-status --install`. Homebrew's two-step becomes a real two-step rather
than a cross-ecosystem detour.

### 8. Deprecate the npm package

`npm deprecate @askviraj/claude-status "installs via Homebrew now: brew install virajp/tap/claude-status"`.

**Deprecate, not unpublish.** npm does not allow unpublishing after 72 hours,
and it is the right outcome anyway: an existing user running `npx` gets a
message rather than a 404.

Publish one final version whose installer prints the migration message and exits
non-zero, so a fresh `npx` is loud rather than mysteriously absent.

### 9. Delete `installer/`

The directory, its tests, its build task, `pnpm`/`tsup` if nothing else uses
them, and the `publish` job's npm half.

### 10. Docs

§9 records npm as retired, with the date and the deprecation message. §5 gains
the new subcommands. `readme.md` leads with brew. `CONTRIBUTING.md` loses the
installer build steps.

## Acceptance criteria (from contract)

1. Given a receipt written by the **npm** installer, when the ported binary runs
   `--uninstall`, then it restores exactly what the npm installer would have.
2. Given a `settings.json` carrying unrelated user keys, when `--install` then
   `--uninstall` run, then the file is byte-identical to before.
3. Given a `PostToolUse` list containing another tool's hook, when `--install`
   then `--uninstall` run, then that hook is present and untouched throughout.
4. Given a foreign status line, when `--install` runs without `--force`, then it
   asks, and without a terminal it refuses.
5. Given a config edited since install, when `--uninstall` runs, then it is left
   alone.
6. Given a throwaway `$HOME`, when install → render → uninstall runs, then the
   tree is byte-identical to before — carried forward from §10 Phase 4.
7. Given the ported suite, when it runs, then every behaviour the 1,204-line
   TypeScript suite asserted is still asserted.
8. Given `npx @askviraj/claude-status`, when it runs after deprecation, then it
   prints the migration message and exits non-zero.
9. Given the repo after this cycle, when it is searched, then `installer/` is
   gone and no task references it.

## Risks / drift

**This is a rewrite disguised as a removal.** 1,783 lines of TypeScript and
1,204 lines of tests, in a different language, for logic that edits files in a
user's home directory. The estimate that matters is not the deletion — it is
steps 2 through 6, and if they are not done properly this cycle makes the
product worse than leaving npm in place.

**Porting loses the thing the original knows and does not say.** `settings.ts`
and `receipt.ts` carry reasoning in comments, but the *tests* carry the cases.
Step 6 is ordered after the ports for exactly that reason — port, then prove
against the old suite — and criterion 7 is the gate.

**An existing npm-installed user must not be stranded.** They have a binary at
`~/.claude/bin/claude-status`, a receipt, and wired settings. Criterion 1 covers
the receipt format; what is *not* covered by any criterion here is the migration
path itself — brew-installing over an npm install leaves two binaries and a
`settings.json` pointing at the old one. **That is a gap in this plan and should
be closed before execution**, either as a step or as an accepted, documented
manual step.

**The gate can rot.** If plan 2 ships and this sits for months, its "current
state" goes stale — the line counts, the module list, the flag set. Re-survey
before executing rather than trusting the table above.

**Removing a channel is not reversible in the way adding one is.** The npm name
stays reserved, so it can be revived — but a user who moved to brew will not
move back, and the deprecation message is public the moment it is set.

## Out of scope for this cycle

- **Unpublishing the npm package.** Not possible after 72 hours, and not
  desirable; deprecation is the outcome.
- **Keeping npm as a second channel.** A legitimate alternative outcome — if the
  port proves harder than it looks, abandoning this cycle and keeping npm as the
  wiring tool is a better answer than half a port. Say so in Gaps rather than
  pressing on.
- **The `ai-plugins` cutover (Phase 5).** Still a different repository.
- **Code signing.** Still deferred, still unowned.

## Gaps surfaced during execution

*(filled in during execution)*
