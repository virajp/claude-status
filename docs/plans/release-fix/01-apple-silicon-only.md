---
type: vwf-plan
title: apple-silicon-only — 2026-08-22
description: Cut the published target set to aarch64-apple-darwin alone —
  Intel Mac out, and the Linux set surveyed and declined.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
timestamp: 2026-08-22T17:30:00Z
tags: [ distribution, targets, ci ]
---

# Plan: apple-silicon-only — 2026-08-22

## Slice

Contract §9. The published set goes from two targets to **one**:
`aarch64-apple-darwin`.

## Current state (actual)

- `.config/mise/tasks/_scripts/_rust` — `supported_targets()` has two rows,
  `aarch64-apple-darwin` and `x86_64-apple-darwin`. Everything that counts
  targets derives from it; a three-item comment names what does not.
- `.github/workflows/release.yml` — the `build` matrix pairs each target with a
  native runner; the `test` matrix runs on `macos-latest` and `macos-15-intel`.
  The test matrix comment explains why both exist: *"One runner per row of
  `supported_targets` keeps every artifact that reaches npm backed by a suite
  that ran on its own architecture."*
- `npm/claude-status/package.json` — `os: ["darwin"]`, **no `cpu`**, so npm
  permits both Mac architectures.
- `bin/checksums.json` — two entries, and the staged package pins both.
- The `v1.0.0` GitHub release carries `claude-status-darwin-x64`.
- `readme.md` — *"macOS only — Apple Silicon and Intel."*
- `installer/test/installer.test.mjs` — asserts the exact line
  `supported: darwin:arm64, darwin:x64`, deliberately: *"this is the only place
  `supportedPlatforms()` is observable from outside, so it is where a silently
  re-added platform would show up."*

## Target state (per contract)

One row in `supported_targets()`. One build job, one test job, both on
`macos-latest`.

`npm/claude-status/package.json` gains `cpu: ["arm64"]` beside its
`os: ["darwin"]`. With a single target the two arrays express the supported set
**exactly** — no cross product to worry about — so npm refuses an Intel Mac with
`EBADPLATFORM` before the installer runs at all, and the installer's own
unsupported-platform message goes back to being the second line of defence
rather than the first.

## Delta — ordered steps

### 1. Cut the target table to one row

`supported_targets()` loses the `x86_64-apple-darwin` row. Its comment block is
extensive, correct, and now wrong in its particulars — it says *"macOS only,
both architectures"* and explains a six-to-two reduction. Rewrite it to state
the one-target set and why, keeping the three-item list of what does not derive
from it.

→ **verify:** `target_count` returns 1; `mise run build:statusline` builds one
target; `build:all` reports one target pinned.

### 2. Drop the Intel runners

Both matrices go to a single entry. This also removes the `macos-15-intel`
runner, which is what could not install pnpm — so that failure disappears as a
consequence rather than being worked around.

The test matrix comment's reasoning survives the change and should be kept, not
deleted: one runner per row is still the rule, there is simply one row. The
paragraph about `macos-15-intel` versus `macos-13` becomes history and can go.

→ **verify:** a tag push runs one build and one test job, both on
`macos-latest`, and both go green.

### 3. Pin the architecture in the npm manifest

Add `cpu: ["arm64"]`. Update the manifest's `//` note, which currently explains
the `os` field alone.

→ **verify:** `npm install` on an Intel Mac fails with `EBADPLATFORM`. Since
that cannot be tested here, assert the manifest field directly and check the
message text by inspection.

### 4. Correct the unsupported-platform test

The asserted line becomes `supported: darwin:arm64`. The comment explaining
*why* it is asserted as an exact line is the valuable part and stays — it is
load bearing for exactly this kind of change.

Add `darwin:x64` to the table of hosts the suite tries, so an Intel Mac is
covered by the same assertion that covers Linux and Windows. It is now an
unsupported host like any other, and that is worth pinning rather than assuming.

→ **verify:** the suite fails if a target is silently re-added.

### 5. Remove the x64 asset from the `v1.0.0` release

The release carries a binary for a target that is no longer served. Nothing
consumes it — npm was never published — so removing it costs nothing and leaves
the release honest.

→ **verify:** `gh release view v1.0.0` lists one binary and a `SHA256SUMS`
regenerated to match.

### 6. Docs

- `readme.md` — *"Apple Silicon and Intel"* becomes Apple Silicon only. The
  Requirements section should say plainly that Intel Macs are not served and
  that npm will refuse the install, because that is the behaviour a user hits.
- Contract §9 — an amendment in the established style, dated, stating the
  one-target set. It should record the ecosystem reason rather than a
  preference: pnpm ships no macOS x64 binary at any recent version, and a target
  whose build tooling has stopped supporting it is a gap this repo would own
  indefinitely.
- Contract §9's struck-through six-target paragraph is **not** touched. It is
  the record of a decision and the bar it sets for adding targets back still
  stands.

→ **verify:** `/vwf:docs-sync` over the cycle's commit range.

## Acceptance criteria (from contract)

1. `supported_targets()` has exactly one row and `target_count()` returns 1.
2. A tag push runs one build and one test job and both pass.
3. The npm manifest declares `os: ["darwin"]` **and** `cpu: ["arm64"]`.
4. The suite asserts `supported: darwin:arm64` and covers `darwin:x64` as an
   unsupported host.
5. `v1.0.0` carries one binary, with `SHA256SUMS` agreeing.

## Risks / drift

**Intel Mac users get nothing, silently-ish.** npm's `EBADPLATFORM` is terse.
Nobody is stranded today — the package has never been published — but the readme
is the only place that will explain it, so it has to actually say it.

**One runner means one architecture's worth of evidence.** That is the point,
but it removes the property the test matrix comment was defending. Worth
re-reading that comment when any target is ever added back.

## Out of scope for this cycle

- **Linux.** Surveyed and declined; see the folder index for what the survey
  found and what adding it would cost.
- **A Homebrew tap**, which §9 keeps as a later option and this does not change.

## Gaps surfaced during execution

*(filled in during execution)*
