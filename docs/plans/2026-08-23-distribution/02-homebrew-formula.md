---
type: vwf-plan
title: homebrew-formula — 2026-08-23
description: Cycle plan (a diff) adding the formula to the existing
  virajp/homebrew-tap, pinning the published tarball by a digest read out of
  SHA256SUMS, and bumping it from CI on every tag.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: [
  docs/plans/2026-08-23-distribution/01-drop-npm.md,
  docs/plans/2026-08-23-distribution/03-release.md,
]
timestamp: 2026-08-23T14:12:00Z
tags: [ distribution, homebrew, tap, ci, caveats ]
---

# Plan: homebrew-formula — 2026-08-23

> **Revised 2026-08-25 against a three-agent recon.** The original was written
> before `drop-npm`, `site` and `config-generator` landed, before `v0.1.0` was
> cut, and before Homebrew 6.0.0 changed how third-party taps install. Six
> blockers were found; two have since been closed by other cycles and four are
> resolved here. The recon is at `docs/memory/runs/2026-08-25-cycle-08-recon/`
> (gitignored) — `cycle-08-recon-consolidated.md` is the summary,
> `rec-c-report.md` holds a formula verified clean at `brew audit`,
> `brew audit --strict` and `brew style`.
>
> **The goal did not change.** A tap is still the right channel and the original
> reasoning still holds. What had rotted was its picture of the world.

## Slice

Contract §9 (Distribution). `brew install virajp/tap/claude-status` becomes the
install, putting the binary on `$PATH`. The formula's caveats send the user to
the website and to `claude-status --configure`.

**This resumes a decision §9 deferred rather than reopening a settled one.** §9
chose npm for day one and parked a tap as "can still come later". Its recorded
arguments against — *"Linux users still need another"* — stopped applying when
the supported set narrowed to Apple Silicon alone, and §9 already carries a note
saying that row is no longer live.

## Current state (actual)

**`v0.1.0` is published.** Three assets — `claude-status-darwin-arm64`,
`claude-status-darwin-arm64.tar.gz`, and `SHA256SUMS`. There is a real digest to
pin, which was not true when this plan was first written.

**The tap repository already exists.** `virajp/homebrew-tap`, public, created
2026-08-22, holding a stock `brew tap-new` scaffold: `README.md` and `.github/`,
**no `Formula/` directory**. So step 1 is no longer "create a repo".

**The asset name is `<os>-<cpu>`, not the target triple.** `asset_name()` in
`.config/mise/tasks/_scripts/_rust` formats os and cpu; the triple
(`aarch64-apple-darwin`) names only a cargo directory. This is the single
easiest thing in this cycle to get wrong — see step 4.

**The archive is already reproducible.** `reproducible_tar()` landed with
[plan 3](./03-release.md): fixed mtime, zeroed numeric owner/group, `gzip -n`.
The deterministic-tar work that `release.yml`'s header and
[plan 1](./01-drop-npm.md) both assign to "`distribution/02` by name" is **done
and not part of this cycle** — those two comments are now stale and step 6
corrects them.

**Homebrew 6.0.0 requires explicit trust for non-official taps.** Local brew is
6.0.19. `brew tap virajp/tap` followed by `brew install claude-status` — the
form most READMEs show — now **fails** until `brew trust`. Only the
fully-qualified `brew install virajp/tap/claude-status` is a one-command
install. This post-dates most tutorials.

**One target.** `supported_targets()` has a single row, `aarch64-apple-darwin`,
so there is one formula and no bottle matrix.

**§5 guarantees `--version` prints nothing but the version.** That makes it the
one output shape a formula's `test do` block can safely match on.

**The binary can wire itself.**
[config-and-cli/03](../2026-08-23-config-and-cli/03-cli-surface.md) added
`--configure`, so the caveats have a real command to name.

**The website is deployed, but the vanity domain is dark.**
<https://claude-status-site.pages.dev> serves; `claude-status.virajp.dev` does
**not resolve** — `dig +short` returns nothing, with a control proving DNS works
from here. Both website plans landed; the domain was never pointed.

**The npm placeholder was never published.** `@askviraj/claude-status` returns
404 from the registry. The original step 5 (`npm deprecate`) and criterion 8 had
nothing to act on and are **cut**, not deferred. The folder [index](./index.md)
says a `0.0.1` placeholder holds the name; that is false and this cycle corrects
it.

## Target state (per contract)

§9's channel is Homebrew. A tag produces the release and a formula bump from one
workflow, with the formula's `url` **derived from the same `asset_name()` the
release used** and its digest **read** out of `SHA256SUMS` rather than
recomputed. A user runs one command and has a working bar.

## Delta — ordered steps

### 1. Add `Formula/` to the existing tap

`virajp/homebrew-tap` exists and is public; it needs `Formula/claude-status.rb`.
The repo name is Homebrew's convention, not a choice — it must be
`homebrew-<name>` for `virajp/tap` to resolve.

**Outward-facing: ask before creating or pushing anything to that repo.** It is
a separate public repository, and nothing in this cycle may touch it without the
owner saying so first.

**Not homebrew-core.** Core imposes a notability bar and a release history this
project does not have, and hands release timing to a review queue. Revisit if
the project gets traction; nothing here forecloses it.

### 2. Write the formula

Start from the recon's verified text (`rec-c-report.md` §4) — it passes
`brew style`, `brew audit` and `brew audit --strict` at exit 0 on 6.0.19 — with
the asset name and URLs corrected as below.

**No `version` line.** A `version` beside a version-bearing `url` is a hard
`brew audit` failure. Homebrew parses the version out of the URL. The original
plan specified three fields to rewrite; there are **two**.

`depends_on arch: :arm64` and `depends_on :macos`. `ArchRequirement` is
`fatal true` and produces *"The arm64 architecture is required for this
software."* — the clean refusal criterion 4 wants. `depends_on :macos` handles
the Linux half.

`test do` asserts `#{bin}/claude-status --version` contains the version — the
one thing §5 guarantees will never gain decoration.

### 3. Caveats that point at a URL which resolves

Printed after `brew install` **and** by `brew info --formula`. Homebrew shows
the same `caveats` block in both, so this is one thing to write, not two.

They say: run `claude-status --configure` to wire Claude Code; **that
`--configure` overwrites any existing status line**; then the website for docs
and the config generator.

**Use `https://claude-status-site.pages.dev`, not `claude-status.virajp.dev`.**
The vanity domain does not resolve, and a DNS failure is a worse first
impression than a 404. Same for `homepage`. Swap both when DNS is pointed — the
formula is one file and this is a one-line change, which is cheaper than
shipping a dead link now.

**Do not use `brew audit --online`.** It requires the homepage to resolve, which
drags the site's availability into the release path. Plain `audit` does not.

### 4. Bump the formula from CI

A job in `release.yml` after `publish`. Four things it must get right:

**Derive the URL from `asset_name()`.** Do not hardcode a literal in workflow
YAML. The `publish` job already does this — `release.yml:254` is
`source .config/mise/tasks/_scripts/_rust` — so the bump job follows the
established pattern rather than inventing one. It is a *separate* job, so it
needs its own `actions/checkout` before it can source anything. Reading the name
from the same function that produced the asset leaves one source of truth and no
assertion to keep in sync.

This is the cycle's sharpest hazard, because **a wrong asset name is clean at
every gate that exists.** Plain `brew audit` does not fetch the URL; only
`--online` does. `brew bump-formula-pr` treats `--url` as an opaque string when
`--sha256` is supplied and never fetches it either — proven in recon, a 404 URL
was accepted without complaint. So a wrong name surfaces first as a 404 at real
`brew install`, after the tag is cut, in front of the first user.

**Anchor the `SHA256SUMS` read.** `claude-status-darwin-arm64` is a strict
prefix of `claude-status-darwin-arm64.tar.gz` **and sorts first**, so the
obvious `grep <key> SHA256SUMS | head -1` returns the **raw binary's** digest
for a `url` pointing at the tarball. Match the full line, anchored on the
trailing filename. A well-formed but wrong `sha256` fails every user's install
while the bump job stays green.

**Guard against an empty digest.** Because the lookup is by asset name, a wrong
name makes it return **empty**, not wrong — and an empty `--sha256` lets brew
fall back to a best-effort download instead of failing. Fail the job explicitly
if the digest does not match `^[0-9a-f]{64}$`.

**Use `brew bump-formula-pr --write-only --commit --no-audit`.** Verified in
recon: it does the edit offline, with no GitHub token, no fork and no PR, and it
**deletes the redundant `version` line itself**. The original plan's hand-rolled
rewrite is unnecessary. Gotcha: the tap checkout must have a remote configured.
Commit message is conventionally `claude-status 0.2.0`.

**The digest is read from `SHA256SUMS`, never recomputed.** Recomputing from a
rebuilt binary lets the tap and the release ship different bytes under one
version. Criterion 6 is this.

**Credential: an SSH deploy key on `virajp/homebrew-tap`, not a PAT.** Owner's
decision. A deploy key is scoped to that one repository and cannot reach any
other; a PAT is an account-level credential. Only `git push` needs it — the edit
is offline. Record it in §9, which must stop claiming the repo has no standing
credentials.

### 5. Close the secret-containment asymmetry

`site.yml` has a hardened secret-containment test; `release.yml` has none, and
this cycle is what gives `release.yml` a secret worth containing. Add the
equivalent guard.

**Prove every new guard can fail with a control before trusting it.** Four
guards this project shipped passed vacuously by matching their own explanatory
comments. A guard that scans source must strip comments first, and the control
run is not optional.

### 6. Docs

§9 records Homebrew as the channel, the tap's location, the deploy key, and
marks the options-table Homebrew row resolved. It must show the
**fully-qualified** install form only — `brew install virajp/tap/claude-status`
— and not the two-step `brew tap` + `brew install`, which Homebrew 6 broke.

`CONTRIBUTING.md` gains how to bump the formula by hand if CI cannot.

Correct the two stale "Owned by `distribution/02`" comments in `release.yml`'s
header and `01-drop-npm.md` — deterministic tar landed in
[plan 3](./03-release.md). Correct the folder [index](./index.md)'s claim that a
`0.0.1` npm placeholder exists.

## Acceptance criteria (from contract)

1. Given a clean Apple Silicon Mac with Homebrew 6, when
   `brew install virajp/tap/claude-status` runs, then `claude-status --version`
   prints the bare crate version from any directory.
2. Given that install, when the caveats are read, then they name
   `claude-status --configure`, warn that it overwrites an existing status line,
   and give a website URL **that resolves**.
3. Given `brew info --formula virajp/tap/claude-status`, then the same caveats
   appear without installing.
4. Given an Intel Mac or a Linux host, then `depends_on arch: :arm64` /
   `depends_on :macos` refuse it. **Not testable in this repo** — assert the
   requirement lines are present and that `brew info` renders them; the refusal
   itself is verified by reading `ArchRequirement`, which is `fatal true`.
5. Given a new tag, when the release workflow completes, then the tap's formula
   already names the new version and digest, with no human step.
6. Given the formula's `url`, then its filename equals `asset_name()`'s output
   for the published target — derived from that function, not transcribed — and
   its `sha256` equals the anchored `SHA256SUMS` entry **for that same
   filename**, read rather than computed.
7. Given `brew uninstall`, then the binary leaves the prefix and
   `~/.claude/settings.json` is **unchanged** — the known limitation, asserted
   so it is a documented state rather than a surprise.
8. Given `brew audit` and `brew style` on the committed formula, then both exit
   0 **without `--online`**.

## Risks / drift

**The deploy key is a new long-lived credential.** Plan 1 removed the repo's
last one by deleting OIDC's consumer; this adds one back. It can push to the
tap, which means a compromise can ship a formula pointing anywhere. Scoping it
to that repo is the mitigation, and it is why a deploy key was chosen over a
PAT.

**The digest is the integrity story now.** npm's immutability used to anchor it.
A GitHub Release asset is *mutable* — it can be deleted and re-uploaded at the
same URL — so the formula's `sha256` is what stands between a user and
substituted bytes. That makes step 4's "read, never recompute" a security
property rather than a tidiness one. `reproducible_tar()` is what makes a re-run
of the same tag safe.

**`workflow_dispatch` skips the tag/version gate entirely.** It runs against a
branch, so `ref_type` is never `tag`, and it can clobber a live release's
assets. With a formula pinning those assets, a dispatched re-upload that changed
bytes would break `brew install` silently. Out of scope to fix here, but it is a
sharper hazard once the tap exists — flag it in §9.

**`brew uninstall` cannot un-wire.** Criterion 7 pins it as a known state. A
user who removes the binary keeps three `settings.json` keys pointing at a
command that no longer exists, and Claude Code will show a broken status line.
The website should say how to clear them; nothing in this plan does it for them.

**Gatekeeper is unchanged and still unowned.** §9 deferred signing and
notarisation. A brew-installed binary is downloaded rather than built, same as
npm's was — but "brew installed it" makes people expect it is signed, and it is
not.

**The bump job has never run, and its failure mode is quiet.** The release
succeeds, the tap does not move, and `brew install` keeps serving the previous
version. The `v1.0.0` restart is the first real exercise of it.

**`/opt/homebrew` is shared mutable state across worktrees.** Git worktree
isolation does not cover it. A scratch tap left in `/opt/homebrew/Library/Taps/`
contaminated another agent's `brew style` run during recon. Any `brew tap` done
while testing must be untapped afterwards, and concurrent agents must not both
drive `brew`.

## Out of scope for this cycle

- **Deterministic tar.** Landed in [plan 3](./03-release.md).
- **Cutting a release.** [Plan 3](./03-release.md); the `v1.0.0` restart follows
  this cycle.
- **Pointing DNS at `claude-status.virajp.dev`.** Owner's, deliberately
  deferred. Step 3 works around it rather than waiting.
- **homebrew-core.** See step 1.
- **A cask, or bottles.** A single-binary formula with a prebuilt asset needs
  neither.
- **Linux and Linuxbrew.** Deferred; see the folder [index](./index.md).
- **Code signing and notarisation.** Still deferred, still unowned.
- **Deprecating the npm placeholder.** It was never published — nothing to
  deprecate.
- **An uninstall path for the `settings.json` keys.** Decided against in
  `config-and-cli/03`.
- **Fixing the `workflow_dispatch` gate gap.** Recorded above; its own cycle.

## Gaps surfaced during execution

*(filled in during execution)*
