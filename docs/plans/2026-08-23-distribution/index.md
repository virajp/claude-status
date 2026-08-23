---
type: vwf-plan-index
title: distribution — 2026-08-23
description: Three ordered plans that delete the npm installer, ship a Homebrew
  tap pointing at the website, and cut the first release through the whole
  chain.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
timestamp: 2026-08-23T14:10:00Z
tags: [ distribution, homebrew, npm, release, ci, tap ]
---

# distribution — 2026-08-23

| # | Plan                                         | What it changes                                                   |
| - | -------------------------------------------- | ----------------------------------------------------------------- |
| 1 | [drop-npm](./01-drop-npm.md)                 | `installer/` deleted, npm out of the release, tarball asset added |
| 2 | [homebrew-formula](./02-homebrew-formula.md) | the tap, the formula, caveats pointing at the website, a CI bump  |
| 3 | [release](./03-release.md)                   | cut `v0.1.0` and exercise the whole chain for the first time      |

## Homebrew replaces npm outright

Not a second channel — a replacement. `brew install virajp/tap/claude-status`
puts the binary on `$PATH`, and `claude-status --configure` does the wiring that
the npm installer used to do.

**This is only cheap because nothing has been released.** No git tag, no GitHub
Release, and the only thing on the registry is a `0.0.1` placeholder holding the
name. There are no users, so there is no migration, no deprecation window and no
compatibility to keep. The `installer/` directory is deleted rather than ported.

**The npm name is kept, not unpublished.** `@askviraj/claude-status@0.0.1` stays
where it is. It costs nothing, it stops someone else taking the name, and npm
does not allow unpublishing after 72 hours anyway.

## Order

**1 → 2 → 3.** Plan 1 removes npm and adds the `.tar.gz` asset a formula needs.
Plan 2 writes the tap and formula against an asset that does not exist yet. Plan
3 cuts the tag that produces it — and is therefore the first time any of it
runs.

Putting the release last is deliberate. A release that only publishes half the
chain would leave a tap pointing at an asset that was never uploaded, and the
formula bump is the step most likely to be wrong on the first attempt.

## What plan 1 deletes, and where it went

`installer/src/` is 1,783 lines of TypeScript with 1,204 lines of tests. Most of
what it *does* is not being dropped — it moves into the binary in
[config-and-cli/03](../2026-08-23-config-and-cli/03-cli-surface.md):

| The installer did                                 | Now                                       |
| ------------------------------------------------- | ----------------------------------------- |
| wire three `settings.json` keys                   | `claude-status --configure`               |
| seed `~/.config/claude-status.json` from defaults | `--configure` seeds `$schema` only        |
| seed the repo layer                               | the user writes it; `--help` explains how |
| migrate the JS bar's `statusline.json`            | **deleted** — nothing to migrate          |
| record a receipt so uninstall could restore       | **deleted** — no undo, by decision        |
| download / place the binary                       | Homebrew                                  |

So plan 1 is a genuine deletion, but only because `config-and-cli/03` lands
first. **That ordering crosses folders and is the one real dependency between
them.**

## Platforms

**Apple Silicon macOS only.** `supported_targets()` has one row and this folder
does not change it.

Linux was evaluated for this round and deferred rather than rejected. The
technical cost is low — TLS is already `rustls` with baked roots, every
dependency is pure Rust, home resolution is `$HOME`, and the keychain is already
gated as a capability check that falls back to `~/.claude/.credentials.json`.
The blockers are that nobody has confirmed Claude Code on Linux writes that
credentials file (if it does not, the spend segment silently never renders), and
that Homebrew serves Linux poorly enough that supporting it properly means a
second channel. Revisit with evidence; do not re-derive the evaluation.

## Version line

**`0.1.0` ships first.** `Cargo.toml` already says so, and its comment reserves
`1.0.0` for "once tested". The publish path, the tap and the install have never
been exercised end to end, and a version that does not promise stability is the
right one to prove them on.
