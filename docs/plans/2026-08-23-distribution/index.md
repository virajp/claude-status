---
type: vwf-plan-index
title: distribution — 2026-08-23
description: Three ordered plans that delete the npm installer, ship a Homebrew
  tap pointing at the website, and cut the first release through the whole
  chain.
status: done
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
Release, and nothing on the npm registry at all. There are no users, so there is
no migration, no deprecation window and no compatibility to keep. The
`installer/` directory is deleted rather than ported.

**Nothing is on the registry under the npm name.** This folder recorded a
`0.0.1` placeholder holding `@askviraj/claude-status`; an authenticated fetch
returns 404, so there is nothing to deprecate and nothing to unpublish, and
[plan 2](./02-homebrew-formula.md) drops the `npm deprecate` step it carried for
that reason.

Whether it was *ever* published is not decidable from outside — an unpublished
package 404s exactly like one that never existed — so this says what was
measured rather than the stronger claim. Nothing turns on the difference. The
name is unclaimed today; taking it is a separate decision nobody has made.

## Order

**1 → 3 → 2.** Plan 1 removes npm and adds the `.tar.gz` asset a formula needs.
Plan 3 cuts the tag that produces that asset. Plan 2 then writes the formula
against a digest that actually exists.

**This reverses the order originally recorded here, and the reversal resolves a
contradiction rather than creating one.** Plan 3's frontmatter declared
`requires: 2`, while plan 2's out-of-scope list said "Cutting the release that
produces the asset. [Plan 3]". Each declared it needed the other; the cycle was
in the documents. Only one order breaks it, because a formula cannot pin a
digest that has never been published.

The paragraph that stood here argued the release must come last, so a tap would
never point at an asset that was never uploaded. Under the new order that risk
cannot arise at all: the asset exists before the formula naming it is written.

One consequence travelled with the reversal — the deterministic-archive fix,
assigned to plan 2 by name in `release.yml` and plan 1, moved into plan 3. It
has to precede the digest being pinned, and under this order plan 3 is what
precedes it.

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
