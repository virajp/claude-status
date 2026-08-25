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
  docs/plans/2026-08-23-distribution/01-drop-npm.md,
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

## Order — this cycle runs BEFORE `02`

`03` declared `requires: 02`, while `02`'s own out-of-scope list says "Cutting
the release that produces the asset. [Plan 3]". Each declared it needed the
other. **The cycle was in the documents**, not introduced by reordering them —
reversing it is the way out, not a violation.

`02` writes a formula pinning a released asset's digest. No release exists, so
there is nothing to pin. This cycle produces that asset; `02` then reads it.

Two consequences carried here deliberately:

- **The deterministic-archive fix moves into this cycle.** It was assigned to
  `02` by name in `release.yml` and `01-drop-npm.md`, on the assumption `02`
  came first. Both now say `03`. Landed as `reproducible_tar`.
- **Criteria 2 and 3 defer to `02`.** They require a formula that does not exist
  yet. Their *intent* — a clean machine installing and running the binary with
  no toolchain — is provable here from the raw asset, and that is what criterion
  2 now asks.

## Current state (actual)

**No git tag exists today.** No GitHub Release. But one was pushed: run
`32586517321` on 2026-08-22 was a `push` on tag **`v1.0.0`**, which failed and
was deleted. So `v0.1.0` is a version-number *regression* against a tag that was
briefly public. Nothing mechanical cares; it is recorded so nobody rediscovers
it as a surprise. **Decided: ship `v0.1.0` as planned.**

That failure's two causes are both gone at `f6b22c4`: `MISE_ENV=ci` resolved
`core:rust` with `profile = "minimal"` so clippy was absent, and `mise-action`
died installing `pnpm` on `darwin/amd64`. Rust is now `profile = "default"` in
the base config, and neither node nor pnpm nor the Intel runner row remains.
**The npm registry is not part of this.** `@askviraj/claude-status` 404s to an
anonymous fetch while the same account's `@askviraj/ai-plugins` returns 200 —
consistent with a placeholder never published *or* one published and removed,
and an anonymous query cannot tell those apart. §9's fifth amendment already
records it as unverified. Nothing here deprecates anything.

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
with npm. **There is no formula bump job.** `release.yml` has four jobs —
`verify`, `test`, `build`, `publish` — and `02` is what adds a fifth. The
earlier text called the bump's idempotence "untested", which understated it:
there is nothing to test yet.

## Target state (per contract)

§9's distribution decision moves from *resolved* to *shipped*.
`brew install virajp/tap/claude-status && claude-status --configure` works on a
clean Mac, and every subsequent release is a tag push with no manual step.

## Delta — ordered steps

### 0. Make the archive byte-reproducible — **landed**

Done ahead of the rest, because everything after it depends on the asset being
stable. `reproducible_tar` in `_scripts/_rust` pins the member's mtime, zeroes
ownership numerically and drops gzip's header timestamp; the collect step calls
it instead of archiving with `-z`. `tests/release.rs` holds it by running it,
and each assertion was watched failing first.

This is the work `release.yml` and `01-drop-npm.md` assigned to `02` by name.
Both now say `03`. Without it, criterion 4 below cannot pass.

### 1. Confirm the version, and stop

`0.1.0`. Not `1.0.0`. This is a deliberate pause: it is the last point at which
the first published number can be chosen, and a tag that has been pushed and
consumed is not cleanly retractable.

Correct `Cargo.toml`'s comment while here — **done**; it named the npm package,
which `01-drop-npm` deleted.

### 2. Dry-run the whole chain on a throwaway tag first

**Use a fork.** Pushing `v0.1.0-rc.1` against this repo does not work: `verify`
compares the tag to `Cargo.toml`, computes `0.1.0-rc.1` against `0.1.0`, and
exits 1 — *before* `test` and `build`, which are exactly the jobs a dry run
exists to watch. The rehearsal would die in the first job and prove nothing.

The parse is not the problem; the comparison is. Rehearsing on this repo would
mean committing `Cargo.toml` at `0.1.0-rc.1`, tagging, then committing it back —
two commits on `main` and a window where `main` claims to be an rc. A fork
avoids all of it. Note also that `gh release create` is called without
`--prerelease`, so an rc would publish as **Latest** until step 3 replaced it.

Watch the four jobs — `verify`, `test`, `build`, `publish`. **There is no bump
job**; `02` adds it. The likely failures are mechanical: a runner label, an
artifact path, a permission scope.

### 3. Cut `v0.1.0`

`git tag v0.1.0 && git push origin v0.1.0`, after `Cargo.toml` reads `0.1.0` on
`main` — otherwise `verify` fails, which is the job working.

### 4. Verify the release is complete

Per target: a raw binary, a `.tar.gz`, and both in `SHA256SUMS` with digests
that check out.

The formula half — that the tap moved to `0.1.0` and its `sha256` equals the
manifest entry rather than merely changing — **defers to `02`**, which is the
cycle that writes the formula. Record the tarball's digest here; `02` pins it.

### 5. Re-run the tag, deliberately

Re-run the workflow on `v0.1.0` and confirm the re-uploaded assets are
**byte-identical** — same sha256, not merely present. This is now a real test
rather than an aspiration: `reproducible_tar` landed in this cycle, and
`tests/release.rs` proves it by running it. Before that fix this step could not
have passed.

The formula-commits-nothing half defers to `02`.

Caveat worth stating: reproducibility here is proven for the *archive*. The
toolchain is `core:rust = "latest"`, so a re-run weeks later could resolve a
different rustc and change the binary itself. See Risks.

### 6. Install as a user would

On a Mac with no Rust toolchain and no repo checkout:

```sh
# The tap does not exist yet — `02` writes it. Install from the raw asset:
curl -fsSLO https://github.com/virajp/claude-status/releases/download/v0.1.0/claude-status-darwin-arm64
chmod +x claude-status-darwin-arm64 && ./claude-status-darwin-arm64 --configure
```

The `brew install` form is `02`'s to prove. Everything below it is provable now,
and is the part that actually matters — a machine with no toolchain running the
shipped bytes.

Then confirm the bar renders in Claude Code, `--debug` reports the wiring, and —
with no config file written — that it reports defaults in use rather than an
error. That last one is
[config-and-cli/03](../2026-08-23-config-and-cli/03-cli-surface.md)'s criterion
8, proven for the first time on a real install.

### 7. Docs

§10 **Phase 4** is marked complete — its verification text is about assets and
digests and needs no channel. §9 records the **release** as shipped with the
version and date, but **not the channel**: §9's resolved heading is "GitHub
Release assets, Homebrew as the channel", and marking that shipped with no tap
would overclaim. `02` closes it.

`readme.md` and `site/content/install.md` carry the raw-asset route and keep
their "the tap is not published yet" note. `docs/plans/index.md`'s "Nothing has
shipped" section is **rewritten, not deleted** — this cycle falsifies its no-tag
and no-Release claims, but its npm sentence is independently unverified (see
Current state) and its "One target" paragraph is still true and referenced
elsewhere.

## Acceptance criteria (from contract)

Criterion 2 is carried forward from the archived `distribution` plan, where it
was one of two left open, and restated for Homebrew — the other was
`EBADPLATFORM`, which died with npm.

1. Given `v0.1.0`, when the GitHub Release is read, then it carries
   `target_count()` raw binaries and the same number of `.tar.gz` archives, all
   present in `SHA256SUMS` with matching digests.
2. Given a machine with **no Rust toolchain and no checkout**, when the release
   asset is downloaded and `--configure` run, then the bar renders in Claude
   Code. *(Restated from `brew install`, which `02` owns. The property being
   proven — shipped bytes running on a machine that cannot build them — is
   unchanged; only the delivery is.)*
3. **Deferred to `02`.** Given the tap after the release, then its formula names
   `0.1.0` and a `sha256` equal to the release's `SHA256SUMS` entry. No formula
   exists to check until `02` writes one.
4. Given a re-run of `v0.1.0`, then the workflow completes and the assets are
   **byte-identical** — same sha256, not merely present. *(The "tap receives no
   new commit" half defers to `02`. This criterion was unsatisfiable before
   `reproducible_tar` landed in this cycle; `tests/release.rs` now holds it.)*
5. Given the installed binary, when `--version` runs, then stdout is exactly
   `0.1.0`.
6. Given the binary from criterion 2 with no config file, when `--debug` runs,
   then it reports defaults in use and exits 0. *(Independent of the delivery
   channel — the raw asset proves it as well as a formula would.)*
7. Given a pushed tag whose version disagrees with `Cargo.toml`, then `verify`
   fails before any build runs.

## Risks / drift

**`core:rust` was `latest` — fixed.** Pinned to `1.98.0`, the version it already
resolved to, so criterion 4 no longer has a silent horizon. `mise.toml` had
argued exactly this for zola and not for the toolchain. Original finding: There
is no `rust-toolchain.toml`. `reproducible_tar` makes the *archive*
deterministic given the same binary, and the Rust build is itself reproducible
on a fixed toolchain — both verified. But a `workflow_dispatch` re-run weeks
later can resolve a different rustc, producing a different binary and therefore
different digests for all three assets. Criterion 4 is reliably true for a
re-run close in time. Pinning rust for the release path would close it; that is
a decision this cycle records rather than takes.

**`publish` installed a toolchain it never used — fixed.** `install: false`; the
three jobs that genuinely need cargo now pass `install_args: rust`, so nothing
on the release path installs zola. `tests/release.rs` holds both, and each was
proven able to fail. Original finding, kept because it explains why: `publish`
runs `jdx/mise-action@v4` and then touches no mise-provided tool —
`_rust_reassemble` is bash, the collect step is bash plus `shasum` and `tar`,
the release step is `gh`. It nonetheless installs rust with `profile = default`
**and zola** on `ubuntu-24.04`. That is a failure surface added *after* `test`
and `build` have spent their runner minutes and produced an artifact: a death
there means a green build and no release. It is also the exact shape of the
2026-08-22 failure, where `mise-action` died installing `pnpm` before any repo
command ran. `verify` genuinely needs cargo; neither job needs zola.

**Cheapest de-risk, and it costs nothing extra:** prove `mise install` with the
current tool set on `ubuntu-24.04` *before* the release tag depends on it — open
a throwaway PR touching `site/`, or push a `site-v*` tag. The site deploy needs
doing anyway, and it exercises the same install on the same runner image. Doing
that first turns the largest unexercised risk on the release path into a known
quantity.

**`workflow_dispatch` could create a tag out of thin air — fixed.** `publish`
now refuses to run against a non-tag ref, naming the ref it refused. Re-running
a real release still works: dispatch against the tag itself. Original finding:
The tag/crate gate is wrapped in `if [ "$ref_type" = "tag" ]`, and a dispatch
runs against a branch, so it is skipped. `publish` then computes the tag from
`Cargo.toml` and `gh release create` **creates a tag that was never pushed**.
Dispatching from `main` today would publish `v0.1.0` with no human having tagged
it. Not on the intended path; a loaded footgun beside it.

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
- **npm, in any form.** Nothing here publishes, deprecates or touches the
  registry. The earlier wording called the name "a deprecated placeholder",
  which presupposed a `02` step that has not run — and `@askviraj/claude-status`
  404s to an anonymous fetch anyway, which cannot distinguish never-published
  from since-removed. §9's fifth amendment already records it as unverified.
- **The website's own release.** [website/01](../2026-08-23-website/01-site.md)
  ships on a separate `site-v*` tag, deliberately decoupled from this one.

## Gaps surfaced during execution

*(filled in during execution)*
