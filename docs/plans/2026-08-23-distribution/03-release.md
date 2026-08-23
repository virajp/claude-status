---
type: vwf-plan
title: release — 2026-08-23
description: Cycle plan (a diff) cutting v0.1.0 — the first tag this project
  has
  ever pushed — exercising the release, the tarball asset and the formula bump
  end to end.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: [
  docs/plans/2026-08-23-distribution/02-homebrew-formula.md,
]
timestamp: 2026-08-23T14:13:00Z
tags: [ distribution, release, ci, homebrew, tag ]
---

# Plan: release — 2026-08-23

## Slice

Contract §9 (Distribution) and §10 (Build phases). Ship `v0.1.0`.

**Everything before this has been rehearsal.** The workflow, the tarball step
and the formula bump have all been written and none has run on a tag. This cycle
is where the chain executes for the first time, which makes it a **verification
cycle** as much as a shipping one.

## Current state (actual)

**No git tag exists.** No GitHub Release. The npm registry holds
`@askviraj/claude-status@0.0.1`, deprecated by
[plan 2](./02-homebrew-formula.md) and pointing at the tap.

**`Cargo.toml` is at `0.1.0`**, with the comment *"one line for the binary and
the npm package; 1.0.0 ships once tested"*. The npm half of that comment is
stale after [plan 1](./01-drop-npm.md) and should be corrected here.

**`verify` gates the tag against the crate.** It parses `cargo pkgid` and fails
in seconds if `v$TAG` disagrees with `Cargo.toml` — before anything is built.

**`test` runs one runner per published architecture.** One row today, so
`macos-latest`. §9 makes this the bar for adding a target at all: a binary that
is built but never executed is a claim nobody checked.

**The release steps are idempotent by construction.** The GitHub Release upload
uses `--clobber` for an existing tag. The npm skip-if-published check is gone
with npm. The formula bump's idempotence is **untested** — it commits to another
repo, and "commits nothing when nothing changed" is a property nobody has
verified.

## Target state (per contract)

§9's distribution decision moves from *resolved* to *shipped*.
`brew install virajp/tap/claude-status && claude-status --configure` works on a
clean Mac, and every subsequent release is a tag push with no manual step.

## Delta — ordered steps

### 1. Confirm the version, and stop

`0.1.0`. Not `1.0.0`. This is a deliberate pause: it is the last point at which
the first published number can be chosen, and a tag that has been pushed and
consumed is not cleanly retractable.

Correct `Cargo.toml`'s comment while here — it still mentions the npm package.

### 2. Dry-run the whole chain on a throwaway tag first

Push `v0.1.0-rc.1` (or run the workflow in a fork) and watch all four jobs plus
the bump. **This is the actual point of the cycle.** The likely failures are
mechanical — a runner label, an artifact path, a permission scope, a tap token
without write access — and finding them on a real version number is strictly
worse than finding them on a throwaway one.

Delete the rc release and tag afterwards. Nothing consumed it.

### 3. Cut `v0.1.0`

`git tag v0.1.0 && git push origin v0.1.0`, after `Cargo.toml` reads `0.1.0` on
`main` — otherwise `verify` fails, which is the job working.

### 4. Verify the release is complete

Per target: a raw binary, a `.tar.gz`, and both in `SHA256SUMS` with digests
that check out. Then confirm the tap's formula moved to `0.1.0` **and that its
`sha256` equals the manifest entry**, not merely that it changed.

### 5. Re-run the tag, deliberately

Re-run the workflow on `v0.1.0` and confirm it is a no-op: assets clobbered
identically, and the formula bump committing nothing. A release pipeline you
cannot safely retry is one you will be afraid to use the first time it
half-fails — and this is the untested half.

### 6. Install as a user would

On a Mac with no Rust toolchain and no repo checkout:

```sh
brew install virajp/tap/claude-status
claude-status --configure
```

Then confirm the bar renders in Claude Code, `--debug` reports the wiring, and —
with no config file written — that it reports defaults in use rather than an
error. That last one is
[config-and-cli/03](../2026-08-23-config-and-cli/03-cli-surface.md)'s criterion
8, proven for the first time on a real install.

### 7. Docs

§9's decision is marked **shipped**, with the version and date. §10's phases are
marked complete. `readme.md` and the website carry the real install command.
`docs/plans/index.md` loses its "Nothing has shipped" section, because it stops
being true.

## Acceptance criteria (from contract)

Criterion 2 is carried forward from the archived `distribution` plan, where it
was one of two left open, and restated for Homebrew — the other was
`EBADPLATFORM`, which died with npm.

1. Given `v0.1.0`, when the GitHub Release is read, then it carries
   `target_count()` raw binaries and the same number of `.tar.gz` archives, all
   present in `SHA256SUMS` with matching digests.
2. Given a machine with **no Rust toolchain and no checkout**, when
   `brew install` then `claude-status --configure` run, then the bar renders in
   Claude Code.
3. Given the tap after the release, then its formula names `0.1.0` and a
   `sha256` equal to the release's `SHA256SUMS` entry.
4. Given a re-run of `v0.1.0`, then the workflow completes, the assets are
   unchanged, and the tap receives **no new commit**.
5. Given the installed binary, when `--version` runs, then stdout is exactly
   `0.1.0`.
6. Given that install with no config file, when `--debug` runs, then it reports
   defaults in use and exits 0.
7. Given a pushed tag whose version disagrees with `Cargo.toml`, then `verify`
   fails before any build runs.

## Risks / drift

**Step 2 is the whole cycle and it is the step most likely to be skipped.**
Everything here has been written against a workflow nobody has run. Going
straight to `v0.1.0` because the code looks right is how the first release
becomes a sequence of `0.1.1`, `0.1.2`, `0.1.3` fixing CI.

**The formula bump is the least-tested step and fails quietly.** If it errors
after the release is created, the release succeeds and the tap silently keeps
serving nothing. Criterion 3 must be checked by reading the tap, not by the
workflow reporting green.

**A tag is not as reversible as it looks.** Deleting and re-pushing a tag with
different content is the one thing the `--clobber` idempotence cannot protect
against — anyone who fetched in between has different bytes under the same
version. Prefer fixing forward with `0.1.1`. The rc tag in step 2 exists so that
this never has to be decided under pressure.

**Criterion 2 needs a machine that is genuinely clean.** The maintainer's Mac
has Rust, a checkout and probably a wired `settings.json`. A stale
`~/.claude/bin/claude-status` from earlier development would make a broken
install look like a working one. Use a fresh user account or a VM, and say which
in Gaps.

**`0.1.0` sets an expectation.** A version below `1.0.0` invites users to expect
breaking changes, which is the intent — but once someone installs it, the config
format and the flag names are effectively public. Anything in `config-and-cli`
that is still uncertain should land **before** this tag, not after.

## Out of scope for this cycle

- **`1.0.0`.** A later decision, taken on evidence from this release.
- **Adding Linux targets.** Deferred; see the folder [index](./index.md).
- **Code signing and notarisation.** Still deferred, still unowned.
- **Publishing anything to npm.** The name stays a deprecated placeholder.
- **The website's own release.** [website/01](../2026-08-23-website/01-site.md)
  ships on a separate `site-v*` tag, deliberately decoupled from this one.

## Gaps surfaced during execution

*(filled in during execution)*
