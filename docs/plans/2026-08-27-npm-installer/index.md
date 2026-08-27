---
type: vwf-plan-index
title: npm-installer — 2026-08-27
description: One plan adding npx as a third install channel — a
  dependency-free
  installer that downloads the release asset against a digest pinned in the
  package, reopening two decisions the distribution cycle closed.
status: draft
covers: [
  docs/decisions.md,
]
timestamp: 2026-08-27T17:22:00Z
tags: [ distribution, npm, npx, installer, ci, release ]
---

# npm-installer — 2026-08-27

| # | Plan                                   | What it changes                                                                |
| - | -------------------------------------- | ------------------------------------------------------------------------------ |
| 1 | [npm-installer](./01-npm-installer.md) | `npm/`, a `publish-npm` job, `tests/npm.rs`, a third route in the install docs |

## This is a third channel, not a replacement

Homebrew and mise both stay exactly as they are. The `npx` route joins them, and
all three end at the same place: a `claude-status` on `PATH`, and
`claude-status --configure` doing the wiring.

**It reopens two decisions rather than one, and both are in
[§11 Distribution](../decisions.md).** Neither is being quietly walked back —
the plan states what each reversal costs and what specifically changed.

| Closed by               | Said                                                                | Why it reopens                                                                                                              |
| ----------------------- | ------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `drop-npm` (2026-08-23) | npm is retired; it meant a second language in the tree              | The cost it names is a *build toolchain* — TypeScript, `tsup`, `pnpm`, a lockfile, a `tsconfig`. None of those come back.   |
| `release-fix` (08-22)   | the binary travels **inside** the package; the download was deleted | Those costs were fatal only because embedding was the alternative. For a third channel there is no embedding option at all. |

## What is genuinely new since those closed

Three things, and the reversal rests on them rather than on a change of taste:

- **`--configure` exists.** `drop-npm`'s whole argument was that the installer's
  1,783 lines did work the binary should do. That work has since moved into the
  binary. What is left for an installer to do is *place a file*, which is why
  this one is one dependency-free `.mjs` and not a TypeScript project.
- **The release is already the distribution.** `drop-npm` made the release carry
  a `.tar.gz`, a raw binary and a `SHA256SUMS`. The installer consumes what
  already ships; it adds no artifact.
- **The formula already proves the pattern.** `bump-tap` reads a digest out of
  the published release and pins it in an immutable artifact. `publish-npm` is
  the same job against a different registry.

## Prerequisite outside this repository

**The npm scope must exist and Trusted Publishing must be configured**, and
neither is something a commit here can do. `docs/decisions.md:1660` records
`@askviraj/claude-status` as unclaimed and says taking it "is a separate
decision nobody has made" — **this cycle does not take it.** It publishes under
`@virajp.dev/claude-status` instead, because a scoped publish needs a scope its
publisher owns and that is the identifier this project already serves from. See
[plan 1's prerequisites](./01-npm-installer.md#prerequisites-outside-this-repository).

## Version line

**No new version is cut by this cycle.** The package publishes at whatever the
crate is at when the next tag lands, because its version *is* the crate's
version — that is the mechanism, not a convention. `1.1.0` is current.
