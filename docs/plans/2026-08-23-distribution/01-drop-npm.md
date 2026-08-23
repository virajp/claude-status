---
type: vwf-plan
title: drop-npm — 2026-08-23
description: Cycle plan (a diff) deleting the npm installer and its publish
  path, and adding the tarball release asset a Homebrew formula consumes.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: [
  docs/plans/2026-08-23-config-and-cli/03-cli-surface.md,
]
timestamp: 2026-08-23T14:11:00Z
tags: [ distribution, npm, ci, release, cleanup ]
---

# Plan: drop-npm — 2026-08-23

## Slice

Contract §9 (Distribution). npm stops being the channel. `installer/` is
deleted, the npm half of the release workflow goes with it, and the release
grows the `.tar.gz` asset a formula will consume.

**This requires
[config-and-cli/03](../2026-08-23-config-and-cli/03-cli-surface.md) to have
landed.** That plan puts `--configure` in the binary. Until it does, deleting
the installer removes the only way to wire Claude Code.

## Current state (actual)

**`installer/src/` is 1,783 lines of TypeScript** across eleven modules, with
**1,204 lines of tests** in `installer/test/installer.test.mjs`. It builds
through `tsup` and `pnpm`, staged by `mise run build:installer` into
`target/npm/claude-status`.

**The release workflow has four jobs** (`.github/workflows/release.yml`):
`verify` (tag against `Cargo.toml`), `test` (one runner per published
architecture), `build`, `publish`. The `publish` job reassembles artifacts, runs
`build:installer`, creates the GitHub Release, then publishes to npm with OIDC
trusted publishing — `permissions:` already declares `id-token: write`, and the
job installs `npm@latest` because OIDC needs npm ≥ 11.5.1.

**The npm publish is idempotent**: an `npm view` check skips an already
published version rather than failing. The GitHub Release step uses `--clobber`.
Neither has ever run on a tag.

**The release asset is a raw binary, not an archive.** `asset_name()` in
`.config/mise/tasks/_scripts/_rust` produces `claude-status-darwin-arm64`, and
the collect step copies the binary to that name unarchived. `SHA256SUMS` is
written beside it. `release.yml:213-215` says in its own words that these exist
"for people who want the binary directly, and for a Homebrew tap later".

**On the registry:** `@askviraj/claude-status@0.0.1`, a placeholder reserving
the name, with `latest` pointing at it. No real version has ever been published.

**`npm/claude-status/package.json`** carries the `os`/`cpu` gate and the
`0.0.0-managed-by-cargo` placeholder that `build:installer` substitutes.

## Target state (per contract)

§9's channel becomes Homebrew. The repo builds one thing — a Rust binary — and
publishes it as GitHub Release assets. No Node toolchain anywhere in the build,
no npm step in the release, and no second language in the tree.

The npm name stays reserved at `0.0.1`, untouched.

## Delta — ordered steps

### 1. Add a `.tar.gz` beside the raw binary

The collect step gains `claude-status-<os>-<cpu>.tar.gz` per target, containing
the binary at the archive root. Both shapes are uploaded, and both are entered
in `SHA256SUMS`.

**Beside, not instead.** `CONTRIBUTING.md` already points people at the raw
asset; removing a documented path to suit a formula costs a user something and
saves nothing. The extra asset is a few megabytes on a release nobody pays for.

Done **first**, before anything is deleted, so the asset exists in the workflow
before the workflow is disturbed.

### 2. Strip the npm half of `publish`

The reassemble-and-stage steps, the `npm install -g npm@latest`, the publish
step, and `id-token: write` from `permissions:` — OIDC has no remaining
consumer, and a permission nothing uses is a permission granted for no reason.

`contents: write` stays; the GitHub Release needs it.

### 3. Delete `installer/`

The source, the tests, `build:installer`, the `npm/` manifest directory,
`_rust_reassemble` if nothing else calls it, and `pnpm`/`tsup` from the mise
tool list and any lockfile.

### 4. Remove Node from the toolchain

After step 3, check whether anything still needs it. If not, it leaves
`.config/mise.toml` — the repo becomes single-language, which is the point.

The website ([website/01](../2026-08-23-website/01-site.md)) uses Zola, a single
Rust binary, precisely so this stays true.

### 5. Check what else referenced the installer

`installer/src/modules/config.ts` held one of the two `SCHEMA_URL` constants;
`src/modules/config/autoseed.rs` held the other and was deleted in
[config-and-cli/02](../2026-08-23-config-and-cli/02-config-relocation.md).
Confirm the surviving constant is the one the binary uses, and that nothing
imports the deleted one.

`--debug`'s `CLAUDE WIRING` section reads `settings.json` and is unaffected, but
it should be checked rather than assumed — it was written when the installer was
the thing that wrote those keys.

### 6. Docs

§9 records npm as retired before it ever shipped, and why: the channel asked for
a Node toolchain to deliver a Rust binary, and Homebrew is what macOS developers
use. §10's phases lose the npm steps. `CONTRIBUTING.md` loses the installer
build. `readme.md` is rewritten in
[website/01](../2026-08-23-website/01-site.md) — this cycle only removes install
instructions that are now wrong.

## Acceptance criteria (from contract)

1. Given a `v*` tag, when the release workflow runs, then the GitHub Release
   carries a `.tar.gz` **and** a raw binary per target, all entered in
   `SHA256SUMS`, and nothing is published to npm.
2. Given the `.tar.gz`, when it is extracted, then `claude-status` is at the
   archive root and is executable.
3. Given `SHA256SUMS`, when each digest is checked against its asset, then all
   agree.
4. Given the repo after this cycle, when it is searched, then `installer/`,
   `npm/`, `tsup`, `pnpm` and `build:installer` are all gone.
5. Given `.config/mise.toml`, then Node is absent unless something still needs
   it — and if it is present, the reason is recorded.
6. Given `release.yml`, then `id-token: write` is gone and `contents: write`
   remains.
7. Given `mise run code:all`, then it passes with no Node toolchain installed.
8. Given the npm registry, then `@askviraj/claude-status@0.0.1` is still present
   and unmodified.

## Risks / drift

**Ordering across folders is the real risk.** This cycle deletes the only way to
wire Claude Code. If it lands before
[config-and-cli/03](../2026-08-23-config-and-cli/03-cli-surface.md), the product
has no setup path at all. The `requires:` records it; it is worth checking
rather than trusting, because the two folders read as independent.

**Deleting 1,204 lines of tests deletes knowledge, not just code.** They encode
edge cases — foreign status lines, stale ownership, orphan sweeps — that the
prose never captured. `config-and-cli/03` reimplements a *subset* deliberately
(no receipt, no undo), so this is not a like-for-like loss, but anything that
suite asserted and the new `--configure` does not is a gap. Read the suite
before deleting it, and record in Gaps what was dropped on purpose.

**A half-stripped workflow is worse than either state.** Step 2 touches a job
that has never run. Removing the wrong step leaves the release either publishing
nothing or failing on a missing artifact — and it will not be discovered until
[plan 3](./03-release.md) cuts the first tag. Worth running the workflow on a
throwaway tag in a fork before merging.

**`asset_name()` and the target table are shared machinery.** The tarball step
must derive its name from the same table, not hard-code one filename. There is
one target today, which is exactly the condition under which hard-coding looks
harmless and silently breaks the day a second is added.

**Keeping the npm placeholder is a small ongoing lie.** `0.0.1` says
"Placeholder to reserve the name — see 1.0.0", and 1.0.0 will now never appear
on npm. Worth a `npm deprecate` pointing at the tap once
[plan 2](./02-homebrew-formula.md) exists — noted here, owned there, because
until the tap exists there is nowhere to point.

## Out of scope for this cycle

- **The tap and the formula.** [Plan 2](./02-homebrew-formula.md).
- **Cutting a release.** [Plan 3](./03-release.md).
- **Unpublishing the npm placeholder.** Not possible, and not wanted.
- **Adding Linux targets.** Deferred; see the folder [index](./index.md).
- **The readme rewrite.** [website/01](../2026-08-23-website/01-site.md).

## Gaps surfaced during execution

*(filled in during execution)*
