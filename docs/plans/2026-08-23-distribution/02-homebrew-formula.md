---
type: vwf-plan
title: homebrew-formula — 2026-08-23
description: Cycle plan (a diff) creating the virajp/homebrew-tap repository,
  writing the formula against the tarball asset, pointing its caveats at the
  website, and bumping it from CI on every tag.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: [
  docs/plans/2026-08-23-distribution/01-drop-npm.md,
]
timestamp: 2026-08-23T14:12:00Z
tags: [ distribution, homebrew, tap, ci, caveats ]
---

# Plan: homebrew-formula — 2026-08-23

## Slice

Contract §9 (Distribution). `brew install virajp/tap/claude-status` becomes the
install, putting the binary on `$PATH`. The formula's caveats send the user to
`claude-status.virajp.dev` and to `claude-status --configure`.

**This resumes a decision §9 deferred rather than reopening a settled one.** §9
chose npm for day one and parked a tap as "can still come later". Its one
recorded argument against — *"Linux users still need another"* — stopped
applying when the supported set narrowed to Apple Silicon alone, and §9 already
carries a note saying that row is no longer live.

## Current state (actual)

**No tap exists.** `virajp/homebrew-tap` has not been created.

**After [plan 1](./01-drop-npm.md)**, a `v*` tag produces a GitHub Release
carrying, per target, a raw binary and a `.tar.gz`, plus `SHA256SUMS`. npm is
gone from the workflow.

**One target.** `supported_targets()` has a single row, `aarch64-apple-darwin`,
so there is one formula and no bottle matrix.

**§5 guarantees `--version` prints nothing but the version.** That makes it the
one output shape a formula's `test do` block can safely match on.

**The binary can wire itself.**
[config-and-cli/03](../2026-08-23-config-and-cli/03-cli-surface.md) added
`--configure`, so the caveats have a real command to name rather than a
cross-ecosystem detour.

**The website exists or is being built in parallel.**
[website/01](../2026-08-23-website/01-site.md) owns `claude-status.virajp.dev`.
The URL is decided, so this cycle can reference it before the site is live — but
see Risks.

## Target state (per contract)

§9's channel is Homebrew. A tag produces the release and a formula bump from one
workflow, with the formula's digest read from `SHA256SUMS` rather than
recomputed. A user runs two commands and has a working bar.

## Delta — ordered steps

### 1. Create the tap repository

`virajp/homebrew-tap`, public, with `Formula/claude-status.rb`. The name is
Homebrew's convention, not a choice: the repo must be `homebrew-<name>` for
`brew tap virajp/tap` to resolve.

**Not homebrew-core.** Core imposes a notability bar and a release history this
project does not have — it has none — and hands release timing to a review
queue. Revisit if the project gets traction; nothing here forecloses it.

### 2. Write the formula

`url` at the tag's `.tar.gz`, `sha256` at its digest, `version` at the crate
version. `depends_on arch: :arm64` and a macOS requirement, mirroring what the
npm manifest's `os`/`cpu` used to enforce, so an unsupported host is refused by
brew rather than at run time.

`test do` asserts `#{bin}/claude-status --version` prints the bare version — the
one thing §5 guarantees will never gain decoration.

### 3. Caveats that point at the website

Printed after `brew install` **and** by
`brew info --formula virajp/tap/claude-status`. Homebrew shows the same
`caveats` block in both, so this is one thing to write, not two.

They say: the binary is on `$PATH`; run `claude-status --configure` to wire
Claude Code; **and that `--configure` overwrites any existing status line**;
then the website for docs and the config generator.

The overwrite warning belongs here because this is the last text a user reads
before running the command that does it.

### 4. Bump the formula from CI

A job in `release.yml` after the release is created: rewrite `url`, `sha256` and
`version` in the tap repo and commit.

**Automated, not hand-edited.** A tap bumped by hand goes stale after the first
release cut in a hurry, and a stale formula installs an old binary while
claiming to be current — a silent wrong answer, worse than a broken one.

**The digest is read from `SHA256SUMS`, never recomputed.** Recomputing from a
rebuilt binary lets the tap and the release ship different bytes under one
version. Criterion 6 is this.

Needs a token with write access to the tap repo, as a repository secret. **This
is a long-lived credential**, and plan 1 has just removed the repo's other one.
OIDC solved npm; there is no equivalent for pushing to another repository. Scope
it to the tap repo alone and record it in §9.

### 5. Deprecate the npm placeholder

Now that there is somewhere to point:
`npm deprecate @askviraj/claude-status "installs via Homebrew now: brew install virajp/tap/claude-status"`.

The name stays reserved. Anyone who finds it gets sent to the tap.

### 6. Docs

§9 records Homebrew as the channel, the tap's location, the tap token, and marks
the options-table Homebrew row resolved. `CONTRIBUTING.md` gains how to bump the
formula by hand if CI cannot.

## Acceptance criteria (from contract)

1. Given a clean Apple Silicon Mac with Homebrew, when
   `brew install virajp/tap/claude-status` runs, then `claude-status --version`
   prints the bare crate version from any directory.
2. Given that install, when the caveats are read, then they name
   `claude-status --configure`, warn that it overwrites an existing status line,
   and give the website URL.
3. Given `brew info --formula virajp/tap/claude-status`, then the same caveats
   appear without installing.
4. Given an Intel Mac or a Linux host, when the formula is installed, then brew
   refuses it.
5. Given a new tag, when the release workflow completes, then the tap's formula
   already names the new version and digest, with no human step.
6. Given the formula's `sha256`, when compared with the `SHA256SUMS` entry for
   the same asset, then they agree — and the bump job read it rather than
   computing it.
7. Given `brew uninstall`, then the binary leaves the prefix and
   `~/.claude/settings.json` is **unchanged** — the known limitation, asserted
   so it is a documented state rather than a surprise.
8. Given the npm registry, then `@askviraj/claude-status` is deprecated with a
   message naming the tap.

## Risks / drift

**The tap token is a new long-lived credential.** Plan 1 removed the repo's last
one by deleting OIDC's consumer; this adds one back with a different blast
radius. It can push to the tap, which means a compromise can ship a formula
pointing anywhere. Scope it to that repo, and §9 should stop claiming the repo
has no standing credentials.

**The digest is the integrity story now.** npm's immutability used to anchor it.
A GitHub Release asset is *mutable* — it can be deleted and re-uploaded at the
same URL — so the formula's `sha256` is what stands between a user and
substituted bytes. That makes step 4's "read, never recompute" a security
property rather than a tidiness one.

**Caveats referencing a site that may not exist yet.** This cycle and
[website/01](../2026-08-23-website/01-site.md) are in different folders with no
dependency between them. If the tap ships first, the caveats point at a 404.
Either sequence them, or have the caveats degrade gracefully — but do not ship a
formula whose first impression is a dead link.

**`brew uninstall` cannot un-wire.** Criterion 7 pins it as a known state. A
user who removes the binary keeps three `settings.json` keys pointing at a
command that no longer exists, and Claude Code will show a broken status line.
The website should say how to clear them; nothing in this plan does it for them.

**Gatekeeper is unchanged and still unowned.** §9 deferred signing and
notarisation. A brew-installed binary is downloaded rather than built, same as
npm's was — but "brew installed it" makes people expect it is signed, and it is
not.

**The formula bump job runs after the release and has never run.** It is the
most likely thing to fail on the first tag, and its failure mode is quiet: the
release succeeds, the tap does not move, and `brew install` keeps serving the
previous version. [Plan 3](./03-release.md) checks it explicitly.

## Out of scope for this cycle

- **Cutting the release that produces the asset.** [Plan 3](./03-release.md).
- **homebrew-core.** See step 1.
- **A cask, or bottles.** A single-binary formula with a prebuilt asset needs
  neither.
- **Linux and Linuxbrew.** Deferred; see the folder [index](./index.md).
- **Code signing and notarisation.** Still deferred, still unowned.
- **An uninstall path for the `settings.json` keys.** Decided against in
  `config-and-cli/03`.

## Gaps surfaced during execution

*(filled in during execution)*
