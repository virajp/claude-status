---
type: vwf-plan
title: drop-npm — 2026-08-23
description: Cycle plan (a diff) deleting the npm installer and its publish
  path, and adding the tarball release asset a Homebrew formula consumes.
status: done
covers: [
  docs/decisions.md,
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

1. **Deferred to [distribution/03](./03-release.md).** Given a `v*` tag, when
   the release workflow runs, then the GitHub Release carries a `.tar.gz`
   **and** a raw binary per target, all entered in `SHA256SUMS`, and nothing is
   published to npm.
2. **Deferred to [distribution/03](./03-release.md)**, with local evidence in
   Gaps. Given the `.tar.gz`, when it is extracted, then `claude-status` is at
   the archive root and is executable.
3. **Deferred to [distribution/03](./03-release.md)**, with local evidence in
   Gaps. Given `SHA256SUMS`, when each digest is checked against its asset, then
   all agree.
4. **Met.** Given the repo after this cycle, when it is searched, then
   `installer/`, `npm/`, `tsup`, `pnpm` and `build:installer` are all gone.
5. **Met.** Given `.config/mise.toml`, then Node is absent unless something
   still needs it — and if it is present, the reason is recorded.
6. **Met.** Given `release.yml`, then `id-token: write` is gone and
   `contents: write` remains.
7. **Met.** Given `mise run code:all`, then it passes with no Node toolchain
   **reachable** — see Gaps for why "installed" could not be the test.
8. **Demoted, and met as reworded:** no step in this cycle contacts the
   registry. The original — "given the npm registry, then
   `@askviraj/claude-status@0.0.1` is still present and unmodified" — asserted
   something outside this cycle against an external mutable service. See Gaps.

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

### Criteria 1–3 are deferred to [distribution/03](./03-release.md)

They describe a **real tagged release** — "given a `v*` tag, when the workflow
runs…" — and no tag is cut here. Precedent exists:
[the plans index](../index.md) already carries three criteria into
`distribution/03` for the same reason.

What was produced instead is local evidence, which is weaker than a real run but
is not nothing. The collect step's body was executed verbatim in a scratch dir
against a locally built binary:

- `tar tzvf` shows `claude-status` at the **archive root**, mode `-rwxr-xr-x`
  (criterion 2).
- `shasum -a 256 -c SHA256SUMS` reports `OK` for both assets, and the manifest
  does not list itself (criterion 3).
- With a second row added to `supported_targets()` **in a `/tmp` copy** of
  `_rust`, the same loop produced `claude-status-linux-x64.tar.gz` with no edit
  to the workflow — the tarball name is table-derived, not hard-coded.

**What local evidence cannot reach** is the `upload-artifact` round trip. The
executable bit inside the tarball comes from `_rust_reassemble`'s `chmod 755`,
and the scratch run reproduced that chmod rather than exercising it. Only a real
tag proves the bit survives the artifact round trip.

### The plan's steps 2 and 3 were wrong in two ways that would have broken the release

Recorded because the plan is the record, and both were caught before landing.

1. **`_rust_reassemble` must stay.** Step 2 said to remove "the
   reassemble-and-stage steps" and step 3 said to delete the script "if nothing
   else calls it". The collect step's `cp target/$target/release/…` reads a path
   that **only** `_rust_reassemble` creates, and its `chmod 755` is the only
   thing restoring the bit `upload-artifact` drops. Deleting it would have
   failed the collect step under `set -e`, so `gh release create` would never
   run and **the tag would produce no release at all**. Only its comments were
   edited.
2. **Step 1's tarball trips the collect step's own guard.** The guard compared
   the asset count to `target_count()` (=1); two assets per target makes that
   `::error::collected 2 assets, expected 1`. The expected count is now
   `$(( $(target_count) * 2 ))`. Verified by mutation: the pre-fix guard fails
   on a correct collection, the fixed guard fails when a tarball is missing, and
   passes when the collection is complete.

Step 3's deletion list was also short by roughly eighteen items. The one that
would have broken outright is `.config/mise/tasks/build/all`, which the plan
never mentions: it called `mise run build:installer` and ran `node -p` against
`target/npm/…/package.json`.

### Criterion 8 is demoted

As written — "given the npm registry, then `@askviraj/claude-status@0.0.1` is
still present and unmodified" — it asserts something **outside this cycle**,
against an external mutable service, and no step here can make it true or false.
It is reworded to **"no step in this cycle contacts the registry"**, which is
locally checkable and was checked: no `npm publish`, `npm view`, `npm deprecate`
or `npm install` survives anywhere in the tree outside plan prose.

**The registry state is UNVERIFIED.** `@askviraj/claude-status` 404s to an
anonymous fetch while the same account's `@askviraj/ai-plugins` returns 200.
That is consistent with either a placeholder that was never published or one
published and since removed. This cycle does not assert either way, and the
["still reserved" line in the plans index](../index.md) is left as it stands
rather than "corrected" on a guess.

### Criterion 7: "no Node toolchain installed" is not testable on this machine

The intended demonstration was that `mise x -- command -v node` comes back empty
once node leaves `[tools]`. **It does not**, and the premise behind that check
was wrong: node and pnpm are also declared in the maintainer's **global**
`~/.config/mise/config.toml`, which this repo neither owns nor should touch. So
a mise shell in this worktree still resolves
`~/.local/share/mise/installs/node/latest/bin/node`, and no edit here can change
that.

What *is* provable, and is stronger than the absence check, is that nothing in
the repo **invokes** a Node toolchain. `node`, `npm`, `pnpm`, `npx`, `tsc` and
`tsup` were each shadowed by a shim that prints a loud message and exits 127,
placed first on `PATH`; the shims were confirmed live (all six exit 127), and
then:

- `mise run code:all` — exit 0, **zero** shims tripped, 541 tests passed.
- `mise run build:all` — exit 0, **zero** shims tripped.

`mise config ls` independently confirms this worktree's `.config/mise.toml`
declares `rust` and nothing else, and `mise run code:toolchain` passes — the
`[tools]` block is non-empty, so the `mise-toolchain` pre-commit hook is
satisfied.

### G1 — the ai-plugins orphan sweep is never run again

Nothing now removes `~/.claude/scripts/statusline`,
`~/.config/ai-plugins/receipts/statusline.json` or
`~/.claude/hooks/context-caps.js`. Three deleted tests covered this.

The one consequence that changes what *renders* is handled: `--debug` still
reports a stale `node …/context-caps.js` hook and `--configure` still replaces
it (`a_legacy_node_caps_hook_is_replaced_rather_than_joined`). The rest is
litter on machines that ran the old installer.

### G2 — the persisted decline is gone with the receipt

`receipt.declinedOrphans` remembered that a user had said no to the orphan
sweep, so it was offered once rather than every run. There is no receipt, so
there is no consent memory. Moot while G1 stands — nothing asks — but it becomes
live again the moment anything does.

### G3 — nothing gates the platform

The two gates were `"os"`/`"cpu"` in `npm/claude-status/package.json` and the
installer's unsupported-platform message. Both were deleted here; the Homebrew
formula that replaces them is **[distribution/02](./02-homebrew-formula.md)'s,
which owns closing this**. Between the two cycles a non-`darwin:arm64` user is
told nothing.

Accepted deliberately. A runtime host check was considered and declined: it
would outlive the window, and the binary would then refuse to run on a platform
the formula had already refused to install. Recorded in the contract's §9 sixth
amendment and in `supported_targets`' own comment.

### G4 — an orphaned receipt on machines that ran the old install

`~/.config/claude-status/receipt.json` is written by nothing and read by nothing
after this cycle, but it is not removed from machines that ran
`npx @askviraj/claude-status --install`. Harmless — it is an inert JSON file —
but it is litter this cycle knowingly leaves.

### G5 — the strongest assertion in the deleted suite has nothing to apply to

"Install then uninstall is a whole-`$HOME` no-op" — a full recursive digest
snapshot of the home before and after — was the suite's best test, because it
caught anything the installer touched *and forgot to record*, including things
nobody thought to assert.

The harness survives: `tests/e2e.rs` still walks a home and digests every path
(`snapshot`, used by `no_mode_writes_outside_the_cache_directory`). What it has
no counterpart for is the round trip, because `config-and-cli/03` shipped
`--configure` with **no `--unconfigure`**. This is not recoverable by writing a
test; it needs an undo to exist.

The contract's §10 Phase 4 verification asked for exactly this round trip. That
is now unverifiable rather than merely unperformed, and Phase 4 says so.

### G6 — shallow top-up semantics are unexercised

"Add whole top-level keys from the template without reaching inside one the user
owns" had two tests. `--configure` seeds a `$schema` pointer and nothing else,
and never tops up, so there is no top-up to exercise. Deliberate — the
full-asset seed is exactly what `config-relocation` set out to remove — but the
*shape* rule is unasserted anywhere now.

### The contract's receipt discipline is contradicted by landed code

§9 closed with "the installer must keep the receipt discipline… an uninstall
*restores* the `settings.json` keys it overwrote". `config-and-cli/03`
deliberately shipped no receipt and no `--unconfigure`, so the spec was already
contradicted **before** this cycle. The paragraph is struck through and the
fifth amendment records why, rather than being quietly deleted.

### What the 57 deleted tests covered

`installer/test/installer.test.mjs` was 1,226 lines and **57 `it(` blocks** (61
assertions once the five-platform loop expands). Read before deletion, as the
plan requires. The split, counted per block:

| Verdict                                                                                 | Count | What                                                                                                                                                                                                                                                |
| --------------------------------------------------------------------------------------- | ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Covered** by a Rust test asserting the same thing                                     | 11    | the three wired keys, the node-hook replacement, foreign-hook survival, `--dry-run`, `--version`, settings merge, stale-command rewrite, idempotence, an existing user config left alone                                                            |
| **Covered by inversion** — a Rust test asserts the *new*, deliberately different answer | 8     | help/no-args (see below), foreign status line (refuse → replace-and-report), the seeded config (byte-copy of the asset → `$schema` pointer alone), `--configure`'s meaning, a non-object config (refuse → ignore), verb-pairing (help → refuse)     |
| **Dropped outright**, no counterpart                                                    | 38    | the whole `--uninstall`/receipt surface (5), the user-level `statusline.json` migration (10), the repo-layer *writing* surface (9), the binary-placement surface (4), the ai-plugins sweep (3), platform gating (1 block / 5 cases), and six others |

11 + 8 + 38 = 57.

**The recon pass reported this as "20 covered, 31 dropped", which does not sum
to 57.** The numbers above were recounted block by block against the surviving
Rust test names. The covered figure agrees within one (12 + 7 = 19 ≈ 20); the
dropped figure was undercounted by seven.

The largest single loss is the repo-layer writing surface — nine tests for a
command that no longer exists, because the per-repo layer is now written by
hand. That is a documented decision (`--help` carries the file's shape), not an
oversight, but nine tests' worth of edge cases went with it.

### Out of scope, found and left alone

Flagged rather than fixed, because none of it traces to this cycle's request:

- **`dprint.json`'s TypeScript plugin** and its ~90 lines of `typescript`
  config. dprint is a mise binary and needs no Node, so this is dead config
  rather than a broken toolchain — but it is dead.
- **`.vscode/settings.json` and `launch.json`** — pnpm/node/tsconfig entries,
  including a `pnpm run dev` launch task for a project that does not exist here.
- **`.config/mise/tasks/setup/all:14`** and **`setup/ai:10`** —
  already-commented `pnpm dlx` / `pnpx` lines.
- **`jdx/mise-action@v4` in the `publish` job.** After the strip, no `mise run`
  remains in that job, so the action is dead weight. Left in place — removing it
  is a workflow change this cycle was not asked for.
- **`release.yml:136`'s comment** ("…now that the npm installer no longer reads
  it") is past tense and remains true, so it was left.
- **`app.rs:544` and `app.rs:742`** were on the brief's fix list but describe
  the **`ai-plugins`** installer — a different, external tool this cycle does
  not touch. Both remain accurate and were left unedited.

### Second fix round — what adversarial review found

Two reviewers ran in **separate worktrees** (cycle 04 shipped a defect where two
mutate-then-revert reviewers shared one and corrupted each other). Between them
they found eleven issues; nine were acted on.

**The chronic defect recurred, and worse than before.** Cycles 02, 03 and 04
each shipped one comment claiming coverage it lacked or citing something that
moved. This cycle shipped **four dangling or falsified references, three of them
written by the same commit that deleted what they pointed at**:

- **The contract passage that named this cycle as its own owner.** §4 carried
  live prose — *"The installer still seeds… `distribution/01` deletes the
  installer outright and owns retiring both the seeding and the test that pins
  it"* — with a citation into `installer/src/modules/config.ts` and a closing
  instruction telling the reader the paragraph above did not describe the whole
  system. The cycle retired the mechanism and left the sentence assigning it the
  job. Now past tense, citation dropped, instruction inverted.
- **"Sixth amendment" is the fifth.** §9 has five `> **Amended` blocks; the
  label was repeated across six sites in four files, and
  `docs/spec/DRIFT-2026-08-23.md:163` ("Four amendments below never touched the
  heading") settles the count. Corrected everywhere.
- **`hostKey()` cited by a line this cycle added.** The symbol existed only in
  `installer/src/modules/binary.ts`, deleted in the same commit — and the same
  file, seventy lines below, *removed* that exact citation from another comment.
  Half the stated rationale was void; the reason stands on `asset_name` alone.
- **`PACKAGES` never existed.** The identifier is `SUPPORTED`, and
  `DRIFT-2026-08-23.md:165` had already recorded the name as false. The cycle
  rewrote that very sentence, carried the known-wrong identifier through, and
  then deleted the only file that could disprove it.
- **§9's new preamble contradicted its own commit**, claiming nothing under the
  heading changed while the same diff rewrote the numbered list. Now states what
  was edited and why.
- **Present-tense npm prose** three lines above a paragraph the cycle *did* fix
  — the tense pass was applied inconsistently within one document.

**The gitignored-tree scan was fixed structurally rather than patched a fifth
time.** `node_modules/` was outside the SKIP list, and because it is ignored by
the maintainer's *global* ignore file, a violation there turned the suite red
with `git status` completely clean — the same signature that turned `main` red
in cycles 04 and 05. Each previous fix added one more path. The scan now
enumerates **tracked files via `git ls-files`**, so every untracked or ignored
tree is out of scope by construction and there is no list to maintain. Two
vacuity assertions were added, because a scan of nothing passes. Verified by
mutation across six cases: clean tree passes; `node_modules/`, the three
gitignored doc trees and `docs/plans/` are ignored; a tracked violation in
`readme.md` and a **newly `git add -N`'d file** are both caught. `walk_files`
became dead and was removed.

**The release could ship a non-executable binary, green.** The collect step's
guard counts assets and cannot see a *mode*. Deleting the whole reassemble step
aborts loudly — that blocker is caught — but deleting only its `chmod` line
passes with exit 0 and publishes a 644 binary inside a 644 tarball. Two comments
already called that `chmod` load-bearing; nothing enforced it. An `-x` check now
does, proven both ways: 755 passes, 644 fails with the reason.

**Corrected in this round:** the covered/inverted split is **11 / 8 / 38**, not
12 / 7 / 38 — the deleted "prints help and mutates nothing when given no
arguments" case is an *inversion* (the binary prints a one-line diagnostic, and
`tests/e2e.rs` asserts exactly that), and its mutates-nothing half is asserted
nowhere: `no_mode_writes_outside_the_cache_directory` has no empty-args row.

### Left for later, deliberately

- **The `.tar.gz` is not byte-reproducible.** `tar -czf` embeds mtime, so a
  `workflow_dispatch` retry produces identical contents with a different sha256.
  Harmless while nothing pins the digest — but
  [distribution/02](./02-homebrew-formula.md)'s formula will pin it, and then a
  retry silently breaks every `brew install` with no signal. The header comment
  no longer claims byte-idempotence, and names the fix (`--sort=name`,
  `--mtime`, fixed owner/group — GNU tar only, which `publish` has and a macOS
  reviewer does not). **Owned by `distribution/03`**, which now runs first —
  `02` needs a published asset to pin, and only a release produces one. Landed
  there as `reproducible_tar`, held by `tests/release.rs`.
- **`jdx/mise-action@v4` is dead weight in `publish`** now that no `mise run`
  survives there. Installing a full Rust toolchain is a failure surface between
  a green build and the actual release. Safe follow-up, not this cycle's.
- **`launch.json` was never live** — a suspicion raised during review and
  disproved: it targets `projects/service` and `projects/web`, which
  `git log
  --all -- projects/` shows never existed, and the deleted
  `package.json` had no `scripts` block at all. `pnpm run dev` was unrunnable
  before this cycle. Not broken by it, not fixed here.
