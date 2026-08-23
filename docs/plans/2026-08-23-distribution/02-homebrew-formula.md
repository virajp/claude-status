---
type: vwf-plan
title: homebrew-formula — 2026-08-23
description: Cycle plan (a diff) adding a Homebrew tap as a second install
  channel — a tarball release asset, a formula in virajp/homebrew-tap, and CI
  that bumps it on every tag.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: [
  docs/plans/2026-08-23-distribution/01-release.md,
]
timestamp: 2026-08-23T10:12:00Z
tags: [ distribution, homebrew, tap, release, ci ]
---

# Plan: homebrew-formula — 2026-08-23

## Slice

Contract §9 (Distribution). Add `brew install virajp/tap/claude-status` as a
second way in, alongside `npx @askviraj/claude-status --install`.

**This resumes a decision §9 deferred, rather than reopening a settled one.** §9
chose npm for day one and parked a tap as "can still come later". The one
argument its options table recorded against a tap — *"Linux users still need
another"* — stopped applying when `macos-only` and `release-fix` narrowed the
set to Apple Silicon alone, and §9 already carries a note saying that row is no
longer live.

npm stays. This cycle adds a channel; it does not replace one. Removing the npm
installer is [plan 3](./03-retire-installer.md), and it is gated.

## Current state (actual)

**No tap exists.** `virajp/homebrew-tap` has not been created.

**The release already produces most of what a formula consumes.** After
[plan 1](./01-release.md), a `v*` tag produces a GitHub Release carrying one
asset per `supported_targets()` row plus `SHA256SUMS`. `release.yml:213-215`
says so in its own words — the assets exist "for people who want the binary
directly, and for a Homebrew tap later".

**The asset is a raw binary, not an archive.** `asset_name()` in
`.config/mise/tasks/_scripts/_rust` produces `claude-status-darwin-arm64`, and
the collect step copies the binary to that name unarchived. `CONTRIBUTING.md`
advertises it for direct download.

**One target.** `supported_targets()` has a single row, `aarch64-apple-darwin`,
so there is one formula and no bottle matrix.

**The binary cannot install itself.** Its flags are `--statusline`,
`--subagent`, `--refresh-spend`, `--caps-hook`, `--debug`, `--version`,
`--help`. Everything that wires Claude Code lives in the npm installer.

## Target state (per contract)

§9 gains Homebrew as a documented second channel. A tag produces both the npm
publish and a formula bump, from one workflow, with the same digest.

A `brew install` user gets the binary on `PATH` and **is told, in caveats, that
they still need `npx @askviraj/claude-status --install` to wire Claude Code**.
That is an awkward instruction and this plan does not pretend otherwise — it is
the honest state until plan 3 moves the wiring into the binary.

## Delta — ordered steps

### 1. Add a tarball beside the raw binary

The collect step gains a `.tar.gz` per target —
`claude-status-<os>-<cpu>.tar.gz` — containing the binary at the archive root.
Both shapes are uploaded and both are in `SHA256SUMS`.

**Beside, not instead.** `CONTRIBUTING.md` already points people at the raw
asset; removing it to suit a formula would break a documented path for no gain,
and the extra asset costs a few megabytes on a release nobody is paying for.

### 2. Create the tap repository

`virajp/homebrew-tap`, public, with `Formula/claude-status.rb`. The naming is
Homebrew's convention, not a choice: the repo must be `homebrew-<name>` for
`brew tap virajp/tap` to resolve.

**Not homebrew-core.** Core imposes a notability bar and a release history this
project does not have — it has one release, from plan 1 — and hands release
timing to a review queue. Revisit if the project gets traction; nothing here
forecloses it.

### 3. Write the formula

`url` points at the tag's `.tar.gz`, `sha256` at its digest, `version` at the
crate version. `depends_on arch: :arm64` and `depends_on macos:` mirror the npm
manifest's `os`/`cpu`, so an unsupported host is refused by brew rather than at
run time.

`test do` asserts `#{bin}/claude-status --version` prints the bare version. That
is the one thing §5 guarantees will never gain decoration, which makes it the
only safe thing for a formula test to match on.

### 4. Caveats that say the true thing

The formula prints, after install, that the binary is on `PATH` and that Claude
Code is not yet wired — with the `npx @askviraj/claude-status --install` command
to run.

**Do not have caveats describe hand-editing `settings.json`.** Three keys, one
of which is a hook array that must be merged rather than replaced, is not a
paste-this instruction; getting it wrong silently breaks the caps hook. Point at
the installer, which already does it correctly and records a receipt.

### 5. Bump the formula from CI

A job in `release.yml`, after `publish`, that rewrites `url`, `sha256` and
`version` in the tap repo and commits.

**Automated, not hand-edited.** A tap bumped by hand is a tap that is stale
after the first release someone cuts in a hurry, and a stale formula installs an
old binary while claiming to be current — a silent wrong answer, which is worse
than a broken one.

Needs a token with write access to the tap repo, held as a repository secret.
This is the one long-lived credential this project keeps, and it is worth
naming: OIDC solved npm, and there is no equivalent for pushing to another repo.
Scope it to the tap repo only.

### 6. Docs

`readme.md`'s Install section gains the brew line as an alternative, **with the
wiring caveat stated** rather than left to the formula's output. §9 records
Homebrew as a second channel and marks the options-table row resolved.
`CONTRIBUTING.md` gains how to bump the formula by hand if CI cannot.

## Acceptance criteria (from contract)

1. Given a `v*` tag, when the release completes, then the GitHub Release carries
   a `.tar.gz` per target **and** the raw binary per target, all present in
   `SHA256SUMS`.
2. Given a clean Apple Silicon Mac with Homebrew, when
   `brew install virajp/tap/claude-status` runs, then `claude-status --version`
   prints the bare crate version.
3. Given that install, when the caveats are read, then they name the wiring step
   and the exact command.
4. Given an Intel Mac, when the formula is installed, then brew refuses it.
5. Given a new tag, when the release workflow completes, then the tap's formula
   already names the new version and digest, with no human step.
6. Given the formula's `sha256`, when it is compared with the release's
   `SHA256SUMS` entry for the same asset, then they agree.
7. Given `brew uninstall`, then the binary is gone from the prefix — and
   `~/.claude/settings.json` is **unchanged**, which is the limitation plan 3
   exists to fix, asserted here so it is a known state rather than a surprise.

## Risks / drift

**Two channels means two ways to have the wrong binary installed.** A user who
has run `npx ... --install` and then `brew install` has a binary at
`~/.claude/bin/claude-status` *and* one in the brew prefix, with `settings.json`
pointing at the first. Upgrading via brew then changes nothing they can see.
This is the single most likely support question this cycle creates, and the
readme should answer it directly rather than describing two channels as if they
were independent.

**The caveats instruction is genuinely bad UX.** "Install with brew, then run an
npx command" asks a user to have Node to finish installing a Rust binary. It is
accepted as a temporary state on the way to plan 3 — but if plan 3 stalls, this
is what ships, so it must at least be *clearly* stated rather than buried.

**A tap token is a long-lived credential in a repo that just eliminated one.**
Plan 1 revokes the npm token in favour of OIDC; this adds a tap token back.
Different blast radius — it can only push to the tap — but the repo can no
longer claim to have no standing credentials, and §9 should say so.

**Gatekeeper is unchanged and still unowned.** §9 deferred code signing and
notarisation. A brew-installed binary is downloaded rather than built, same as
npm's, so the exposure does not change — but "brew installed it" makes people
expect it is signed, and it is not.

**The digest must come from the same build as the publish.** If the formula bump
recomputes a digest from a rebuilt binary rather than reading `SHA256SUMS`,
Homebrew and npm can ship different bytes under one version. Criterion 6 exists
for that; the bump job must read the manifest, not rebuild.

## Out of scope for this cycle

- **Removing the npm installer.** [Plan 3](./03-retire-installer.md), gated on
  this tap being proven in real use.
- **Moving the wiring into the binary.** Also plan 3. This cycle deliberately
  ships the awkward two-step rather than doing half of plan 3's work.
- **homebrew-core.** See step 2.
- **A cask, or a bottle.** A single-binary formula with a prebuilt asset needs
  neither.
- **Code signing and notarisation.** Still deferred; see Risks.

## Gaps surfaced during execution

*(filled in during execution)*
