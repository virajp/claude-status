---
type: vwf-plan-index
title: distribution — 2026-08-23
description: Three ordered plans that ship the first release, add a Homebrew
  tap, and — once the tap is proven — move the wiring into the binary and
  retire the npm installer.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
supersedes: [
  docs/plans/2026-08-21-2122-release.md,
]
timestamp: 2026-08-23T10:10:00Z
tags: [ distribution, npm, homebrew, release, installer, ci ]
---

# distribution — 2026-08-23

| # | Plan                                         | What it changes                                                    |
| - | -------------------------------------------- | ------------------------------------------------------------------ |
| 1 | [release](./01-release.md)                   | the first publish: one manual bootstrap, then OIDC and a real tag  |
| 2 | [homebrew-formula](./02-homebrew-formula.md) | a tap, and a tarball asset for it to consume                       |
| 3 | [retire-installer](./03-retire-installer.md) | the wiring moves into the binary; the npm package goes — **gated** |

Plans 2 and 3 graduate the **Distribution** entries in
[`backlog.md`](../backlog.md). Plan 1 is not from the backlog: it replaces
`2026-08-21-2122-release.md`, which was written for three npm packages and two
targets and no longer describes this repo.

## Order, and the gate

**1 → 2 → 3, and 3 does not start on a schedule.**

Plan 2 needs plan 1 because a formula points at a release asset, and there is no
release. Plan 3 needs plan 2 to have shipped *and* to have been used: it removes
the only working install path, so it runs when the maintainer confirms the tap
is proven in real use, and not before. Plan 3 says this in its own Slice as well
— it is the one thing in this folder that a reader must not miss.

## Nothing has shipped

No git tag. No GitHub Release. The registry holds one `0.0.1` placeholder
reserving `@askviraj/claude-status`, published to hold the name.

The pipeline that would ship it is **complete and unexercised**.
`.github/workflows/release.yml` has four jobs — `verify` (tag against
`Cargo.toml`), `test` (one runner per published architecture), `build`, and
`publish` (GitHub Release, then npm with OIDC). Every one is idempotent by
construction: the release upload uses `--clobber`, the npm publish skips an
already-published version rather than failing. None of it has ever run on a tag.

## The one thing that is not reversible

Plan 1 is the only cycle in this folder whose steps cannot be undone by editing
a file. A published version number cannot be republished. That is why it is
alone in its own plan rather than folded into plan 2, and it is the same reason
the superseded `2026-08-21-2122-release.md` gave — the argument survived its
plan.

## What Homebrew cannot do, and why plan 3 exists

A formula installs a binary into the brew prefix. It **cannot** write
`~/.claude/settings.json` — Homebrew does not permit a formula to write outside
its prefix, and `caveats` can only print text.

`--install` does far more than place a binary: it wires three `settings.json`
keys (`statusLine`, `subagentStatusLine`, and the `PostToolUse` caps hook),
seeds and migrates `~/.config/claude-status.json`, and records a **receipt** of
prior state so `--uninstall` can restore rather than infer. There is 1,783 lines
of TypeScript under `installer/src/` and 1,204 lines of tests behind that.

So the npm package cannot simply be deleted once a tap exists. Plan 3 is "move
the wiring into the binary, then delete", and moving it is most of the work.

## Version line

**`0.1.0` ships first.** `Cargo.toml` already says so, and its comment reserves
`1.0.0` for "once tested" — the publish path, the tap and the install have never
been exercised end to end, and a version number that does not promise stability
is the right one to prove them on. `1.0.0` is a later decision, taken when there
is evidence for it.
