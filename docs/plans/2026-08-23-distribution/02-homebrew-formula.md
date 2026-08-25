---
type: vwf-plan
title: homebrew-formula — 2026-08-23
description: Cycle plan (a diff) adding the formula to the existing
  virajp/homebrew-tap, pinning the published tarball by a digest read out of
  SHA256SUMS, and bumping it from CI on every tag.
status: done
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

> **This is the pre-execution snapshot, kept as written.** Two of its statements
> are now false by design: `v0.1.0` was deleted for the `v1.0.0` restart, and
> the tap now has a `Formula/` that CI created. See **Shipped — `v1.0.0`** at
> the end for what is true today.

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
[plan 1](./01-drop-npm.md) once assigned to "`distribution/02` by name" is
**done and not part of this cycle**. Both comments were re-read during execution
and both had already been updated by the cycle that moved the work —
`release.yml` now says "this cycle owns that fix, not `distribution/02`" and
plan 1 says "Owned by `distribution/03`". Nothing to correct.

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

**Nothing is on the npm registry under the placeholder's name.**
`@askviraj/claude-status` 404s to an authenticated fetch. The original step 5
(`npm deprecate`) and criterion 8 have nothing to act on and are **cut**, not
deferred. The folder [index](./index.md) says a `0.0.1` placeholder holds the
name; that is not true today and this cycle corrects it.

Whether it was *ever* published is not decidable from outside — an unpublished
package 404s exactly like one that never existed — so this records what was
measured, not the stronger claim. Nothing depends on which it was.

## Target state (per contract)

§9's channel is Homebrew. A tag produces the release and a formula bump from one
workflow, with the formula's `url` **read back out of the release GitHub
published** and its digest **read** out of that release's `SHA256SUMS` rather
than recomputed. Nothing about the asset is reconstructed, so there is nothing
to drift. A user runs one command and has a working bar.

## Delta — ordered steps

### 1. The tap is generated, not seeded

`virajp/homebrew-tap` exists and is public. It needs `Formula/claude-status.rb`
— and **CI creates it**. There is no hand-seeding step.

**This corrects an earlier revision of this plan**, which had the first formula
written by hand and CI bumping two fields in it thereafter. That splits the
formula's source across two repositories: `desc`, `homepage`, `caveats` and the
`depends_on` pair would live only in the tap, where nothing in this repo's suite
can see them and they drift silently from the project they describe. It also
makes the first release depend on a one-time manual step somebody has to get
right.

Instead `.config/homebrew/claude-status.rb` is the source, and `render_formula`
emits the **whole file** with `url` and `sha256` substituted for the release
just published. The tap is a generated artefact, overwritten in full every
release, so it cannot drift; and because the render creates its output, the
first release creates the formula.

The template is a real formula carrying a real released `url`/`sha256` pair
rather than placeholders, so `brew style` and `brew audit` can check it as it
sits in this repo.

**Outward-facing: nothing pushes to that repo until the App exists.** The repo
name is Homebrew's convention, not a choice — it must be `homebrew-<name>` for
`virajp/tap` to resolve.

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

**Read the URL back out of the published release.** Not from `asset_name()`, and
certainly not from a literal in workflow YAML. The job runs after `publish` and
asks GitHub what the release actually carries —
`gh release view "$tag" --json assets` — taking both the asset's name and its
`url` from the response.

**This supersedes an earlier revision of this step, which said to derive the URL
from `asset_name()`.** That would have been one source of truth for the *name*,
but still a reconstruction: it assumes the asset that got uploaded is the one
that function describes. Reading the release removes the assumption instead of
relocating it — you cannot pin a URL that 404s if the URL came from the thing
serving it. The `count != 1` guard is what makes it safe when a second target is
added.

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

**Rewrite the two fields directly; do not put Homebrew on the runner.** Recon
verified `brew bump-formula-pr --write-only --commit --no-audit` works offline
and deletes a redundant `version` line itself, and an earlier revision of this
plan chose it for that reason. **That reason is spent** — the template carries
no `version` line, so there is nothing for it to delete; and the tool patches an
existing formula, which is the wrong shape entirely now that CI renders the
whole file. It would also drag a Homebrew installation onto a runner.

That cuts against `publish`'s installs-nothing doctrine, which exists because a
tool download failing *after* the build has spent its minutes is a green release
with no artifact — the exact shape of the 2026-08-22 failure. A bump job has the
same shape one step later: a green release whose tap silently did not move.

So `rewrite_formula` in `_scripts/_rust` does it, and the suite runs it. It
refuses a formula where `url` or `sha256` is absent or doubled, because `awk`
exits 0 whether or not a pattern matched — without that check a formula that
changed shape would sail through a rewrite that did nothing. Commit message
stays `claude-status <version>`, matching Homebrew's own convention.

**The digest is read from `SHA256SUMS`, never recomputed.** Recomputing from a
rebuilt binary lets the tap and the release ship different bytes under one
version. Criterion 6 is this.

**Credential: a GitHub App.** Owner's decision, after research, and it
supersedes the SSH deploy key an earlier revision of this plan recorded.
`GITHUB_TOKEN` cannot reach another repository, so the push needs a credential
of its own, and the three candidates differ in what sits in this repo's secrets
between releases:

| Option     | What is at rest                        | Blast radius               |
| ---------- | -------------------------------------- | -------------------------- |
| PAT        | an account-level token                 | every repo the account has |
| Deploy key | a private key that can push to the tap | the tap, forever           |
| **App**    | credentials that only *mint* a token   | one hour, one repo         |

The App wins because nothing at rest can push anything. `APP_ID` and `APP_KEY`
authorise minting; the token itself is installation-scoped, expires in an hour,
and `actions/create-github-app-token` revokes it when the job ends.

Two details that are easy to get wrong:

- **`client-id`, not `app-id`.** The action deprecates `app-id`. These are
  different values on the App's settings page — `APP_ID` holds the **Client ID**
  (`Iv23…`), not the numeric App ID. Same secret name, different value.
- **`permission-contents: write`** narrows the minted token below whatever the
  installation was granted, so a later broadening of the App does not silently
  widen this job.

Record it in §9, which must stop claiming the repo has no standing credentials —
though "standing" is now weaker than it was going to be.

### 5. Close the secret-containment asymmetry

`site.yml` has a hardened secret-containment test; `release.yml` has none, and
this cycle is what gives `release.yml` a secret worth containing. Add the
equivalent guard.

**Prove every new guard can fail with a control before trusting it.** Four
guards this project shipped passed vacuously by matching their own explanatory
comments. A guard that scans source must strip comments first, and the control
run is not optional.

### 6. Docs

§9 records Homebrew as the channel, the tap's location, the App credential, and
marks the options-table Homebrew row resolved. It must show the
**fully-qualified** install form only — `brew install virajp/tap/claude-status`
— and not the two-step `brew tap` + `brew install`, which Homebrew 6 broke.

`CONTRIBUTING.md` gains how to bump the formula by hand if CI cannot.

Correct the folder [index](./index.md)'s claim that a `0.0.1` npm placeholder
exists.

~~Correct the two stale "Owned by `distribution/02`" comments in `release.yml`'s
header and `01-drop-npm.md` — deterministic tar landed in
[plan 3](./03-release.md).~~ **Already done.** Checked during execution:
`release.yml`'s header reads "**This cycle owns that fix, not
`distribution/02`**" and `01-drop-npm.md` reads "**Owned by
`distribution/03`**". The cycle that moved the work moved the comments with it.
The instruction was written from the recon's snapshot at `f6b22c4` and was stale
by the time this plan was revised — the same failure mode the revision existed
to fix, one level up.

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

**The App can still ship a formula pointing anywhere.** Plan 1 removed the
repo's last standing credential by deleting OIDC's consumer; this adds something
back. The App is the mildest of the three options — a compromise of
`APP_ID`/`APP_KEY` yields tokens scoped to the tap and expiring hourly, not a
key that pushes forever — but "scoped to the tap" is precisely the scope needed
to publish a malicious formula, so the reduction is in *duration and breadth*,
not in what a live compromise could do to a user running `brew install`.

**The App's private key is the thing to protect, and it does not expire.**
Short-lived tokens do not make the key short-lived. Rotating it is a manual step
nobody is currently prompted to take, which is worth a note in §9 rather than a
false sense that the App made this a solved problem.

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
- **Deprecating the npm placeholder.** Nothing is on the registry under that
  name — nothing to deprecate.
- **An uninstall path for the `settings.json` keys.** Decided against in
  `config-and-cli/03`.
- **Fixing the `workflow_dispatch` gate gap.** Recorded above; its own cycle.

## Gaps surfaced during execution

**Blocker 6 was confirmed, not inherited.** A scratch tap was built and
`brew audit --strict` run against a formula whose url named
`claude-status-aarch64-apple-darwin.tar.gz`. It **exited 0**; the url returns
HTTP 404. That is the whole justification for reading the asset from the
release, and it is now measured rather than quoted. Blocker 5 was confirmed the
same way — adding a `version` line took `brew audit` from exit 0 to exit 1,
"redundant with version scanned from URL" — which doubles as proof the audit
gate is live rather than passing vacuously. The scratch tap was removed and
`/opt/homebrew/Library/Taps/` restored to its baseline of `macpaw` and
`stablyai`.

**Blocker 3 reproduced against the real published manifest.** v0.1.0's
`SHA256SUMS`: the naive `grep … | head -1` returns `9d088dc5…`, the raw binary's
digest, for a url pointing at the tarball, whose digest is `af64e2a6…`.

**`brew style` on a bare file path is misleading.** It applies generic RuboCop
cops — Sorbet sigils, `Style/Documentation`, frozen string literals — that do
not apply to a formula in a tap, and it reported 5 offences on text that is
clean inside a tap. Only `FormulaAudit/DependencyOrder` was real (`arch` before
`macos`). Verify formulae in a tap, not as loose files.

**The step-6 instruction to fix two stale comments was itself stale.** Both had
already been corrected by the cycles that moved the work. Recorded in step 6
rather than silently dropped.

**The npm claim was weakened.** An earlier draft of this revision said the
placeholder "was never published". An authenticated fetch proves only that
nothing is there **now** — an unpublished package 404s identically. Corrected
here and in the folder index.

**A guard's first draft had a false positive that the suite caught.** The
credential test substring-matched `"pat"` and failed against `path: tap`. It now
enumerates the job's `secrets.` references and requires exactly `APP_ID` and
`APP_KEY`, which is both precise and catches a credential nobody thought to ban.

**The hand-seeding step was removed rather than completed.** An earlier draft of
this record listed "the tap has no `Formula/`" as work blocked on the owner. It
was not owner-blocked; it was a design flaw. CI renders the whole formula, so
the first release creates the tap's file and there is nothing to seed. The
lesson generalises: *a manual step that exists because the automation was scoped
too narrowly is not a blocker, it is the automation's missing half.*

**Criterion 3 is verified.** `brew info --formula` on the rendered formula, in a
real scratch tap, prints the caveats without installing — naming `--configure`,
warning about the overwrite, and giving a URL that resolves. It also renders
`Required: arm64 architecture, macOS` (criterion 4's mechanism) and reports
`stable 0.1.0`, which confirms Homebrew scans the version out of the url and no
`version` line is wanted. `brew style`, `brew audit` and `brew audit --strict`
are all exit 0 against the rendered output.

## Shipped — `v1.0.0`, 2026-08-25

The owner created the GitHub App and set `APP_ID` / `APP_KEY`, which was the
last external blocker. `v1.0.0` was then tagged and the whole chain ran.

**Run `32878826168` passed end to end, `bump-tap` included on its first ever
execution.** The App minted a token, checked out the tap and pushed
`claude-status 1.0.0` as `github-actions[bot]`. `Formula/claude-status.rb` did
not exist beforehand — CI created it, which is the render design working as
intended rather than a step somebody performed.

**Criterion 6 held on real data, and this is the one that mattered.** The tap
pins `7d91e4bf…`, the **tarball's** digest. The raw binary's is `990856ec…`, and
the naive `grep … | head -1` returns that one — so the anchored lookup is the
difference between a working tap and a checksum failure for every user, with the
release run green either way.

**Ordering was reversed from the recorded plan and that was right.** `v0.1.0`
was deleted only after `v1.0.0` was confirmed good. Had `bump-tap` failed — a
live possibility for a job that had never run — the project would still have had
a published release to fix forward from.

### Acceptance criteria

| # | Status                 | Evidence                                                                                                                                                           |
| - | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1 | **met**                | `brew install virajp/tap/claude-status`, then `claude-status --version` prints `1.0.0` from `/tmp` via `PATH`                                                      |
| 2 | **met**                | caveats name `--configure`, warn `OVERWRITES`, and give a URL that resolves                                                                                        |
| 3 | **met**                | `brew info --formula` prints those caveats; also verified pre-release in a scratch tap                                                                             |
| 4 | **mechanism verified** | `brew info` renders `Required: arm64 architecture, macOS`; `ArchRequirement` is `fatal true`. The refusal itself is still untestable here — no Intel or Linux host |
| 5 | **met**                | the tag's run moved the tap with no human step                                                                                                                     |
| 6 | **met**                | tap `sha256` = the published `SHA256SUMS` entry for the tarball, read not computed                                                                                 |
| 7 | **not verified**       | the binary was left installed; no `brew uninstall` was run                                                                                                         |
| 8 | **cut**                | nothing on the npm registry to deprecate                                                                                                                           |

Two notes on what is *not* closed. **Criterion 7** stayed unverified because the
install is in use rather than a test fixture — uninstalling to prove
`settings.json` survives would have taken the owner's status line down.
**`--configure` was run** by the owner and rewrote `statusLine` from
`${HOME}/.claude/bin/claude-status` to a bare `claude-status`, so the bar is now
served by the brew-managed binary and `brew upgrade` keeps it current.
