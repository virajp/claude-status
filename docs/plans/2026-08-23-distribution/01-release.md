---
type: vwf-plan
title: release — 2026-08-23
description: Cycle plan (a diff) for the first publish — the one manual npm
  publish that bootstraps Trusted Publishing, then OIDC registration and a real
  tag driven through CI end to end.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: []
timestamp: 2026-08-23T10:11:00Z
tags: [ distribution, npm, release, ci, oidc ]
---

# Plan: release — 2026-08-23

## Slice

Contract §9 (Distribution). Publish `@askviraj/claude-status@0.1.0` for real.

**This replaces `2026-08-21-2122-release.md`**, which was never executed and was
written against a world with three npm packages and two targets. Its *substance*
survives — npm still requires a package to exist before a Trusted Publisher can
be configured — and its three open acceptance criteria are carried forward
below. Its steps do not survive; there is one package and one target now.

**This is the only cycle in this folder whose steps are not reversible by
editing a file.** A published version number cannot be republished. It gets its
own plan and its own gate for that reason alone.

## Current state (actual)

**On the registry:** `@askviraj/claude-status@0.0.1`, described as
`"Placeholder to reserve the name — see 1.0.0"`, with `latest` pointing at it.
Nothing else. No scoped platform packages — `release-fix` collapsed those.

**In the repo:** no git tag at all. No GitHub Release.

**The pipeline is complete and has never run on a tag.**
`.github/workflows/release.yml`:

| Job       | Does                                                                       |
| --------- | -------------------------------------------------------------------------- |
| `verify`  | parses `cargo pkgid`, fails if the tag disagrees with `Cargo.toml`         |
| `test`    | the full suite, one runner per row of `supported_targets()` — one today    |
| `build`   | the release binary, uploaded as an artifact                                |
| `publish` | reassembles, stages the npm package, creates the GitHub Release, publishes |

`publish` is idempotent in both halves: `gh release upload --clobber` for an
existing tag, and an `npm view` check that **skips** an already-published
version rather than failing. `permissions:` already declares `contents: write`
and `id-token: write`, and the job installs `npm@latest` because OIDC trusted
publishing needs npm ≥ 11.5.1.

**`Cargo.toml` is at `0.1.0`**, with the comment *"one line for the binary and
the npm package; 1.0.0 ships once tested"*. `build:installer` substitutes it
into `npm/claude-status/package.json`, whose committed version is the
`0.0.0-managed-by-cargo` placeholder.

**The manifest refuses the wrong host:** `"os": ["darwin"]`, `"cpu": ["arm64"]`.

## Target state (per contract)

§9's distribution decision moves from *resolved* to *shipped*.
`npx @askviraj/claude-status --install` works on a clean Apple Silicon Mac,
`v0.1.0` exists as both a git tag and a GitHub Release carrying the binary and
`SHA256SUMS`, and every subsequent release is a tag push with no manual step and
no long-lived token in the repo.

## Delta — ordered steps

### 1. Confirm the version, and stop

`0.1.0`. Not `1.0.0` — see the folder [index](./index.md). This step is a
deliberate pause, not a no-op: it is the last point at which the first published
number can be chosen, and everything after it is irreversible.

### 2. Build and inspect the package before anything is published

`mise run build:installer`, then `npm pack --dry-run target/npm/claude-status`.

Check by eye: the version is `0.1.0` and not the placeholder; `bin/` carries the
binary; `os`/`cpu` are `darwin`/`arm64`; the readme is the one written for npm
by `release-fix/04`. **A published tarball cannot be corrected**, only
superseded by a new version — this inspection is the last cheap check.

### 3. The one manual publish

`npm publish --access public target/npm/claude-status`, authenticated with a
token, from the maintainer's machine.

This exists solely because
[npm requires a package to exist before a Trusted Publisher can be configured](https://github.com/npm/cli/issues/8544).
It is the single manual publish in this project's life.

The name already exists at `0.0.1`, so this is a new version on an existing
package rather than a first publish of a name — which means the Trusted
Publisher settings page is reachable already. **Try step 4 before step 3**: if
OIDC can be registered against the placeholder, the manual publish is
unnecessary and this step is skipped entirely. Record which way it went.

### 4. Register Trusted Publishing, and revoke the token

On npmjs.com, set this repo and `release.yml` as the package's Trusted
Publisher. Then delete the token used in step 3, if one was used.

Leaving it is how a repo ends up with a long-lived credential nobody remembers
granting. The point of OIDC is that there is nothing to leak.

### 5. Tag, and watch the whole pipeline

`git tag v0.1.0 && git push origin v0.1.0`, then watch all four jobs.

The tag must be pushed **after** `Cargo.toml` reads `0.1.0` on `main`, or
`verify` fails in seconds — which is the job doing its job.

### 6. Re-run the tag, deliberately

Re-run the workflow on the same tag and confirm every publish step **skips**
rather than fails. The idempotence is written into the job and has never been
exercised; a release pipeline you cannot safely retry is one you will be afraid
to use the first time it half-fails.

### 7. Install it like a user would

On a Mac with no Rust toolchain: `npx @askviraj/claude-status --install`, then
confirm the bar renders in Claude Code. Then `--uninstall` and confirm the tree
is as it was.

### 8. Docs

§9's decision is marked **shipped**, with the version and date. `readme.md`
gains the real install line if it differs from what is written. The `index.md`
"Nothing has shipped" section is deleted, because it will no longer be true.

## Acceptance criteria (from contract)

The first two are carried forward verbatim from the archived `distribution`
plan, where they were the only two left open, and through
`2026-08-21-2122-release.md`, which never ran.

1. Given a published release, when the package is inspected, then there is
   exactly one binary for the one supported platform, built by CI from the
   tagged commit.
2. Given a machine with **no Node toolchain beyond npx and no Rust**, when
   `npx @askviraj/claude-status --install` runs, then the bar renders.
3. Given the package's settings page, when it is read, then this repo and
   `release.yml` are its Trusted Publisher and no long-lived token remains.
4. Given a pushed `v*` tag, when the release workflow runs, then it publishes
   with provenance and no manual step.
5. Given a re-run of an already-published tag, when the workflow runs, then
   every publish step is skipped rather than failed.
6. Given `v0.1.0`, when the GitHub Release is read, then it carries
   `target_count()` assets and a `SHA256SUMS` whose digests match them.
7. Given the published package on a non-Apple-Silicon host, when `npm i` runs,
   then it fails `EBADPLATFORM`.

## Risks / drift

**Criterion 7 could not be proven in the `macos-only` cycle, and this is where
it can be.** npm 11 has no way to simulate a foreign host — `--os`/`--cpu` are
accepted and ignored, `npm_config_platform` warns as unknown — so a local
simulation succeeds and proves nothing. Against the **real registry** an Intel
Mac, a Linux box or a CI runner closes it properly. If no such host is
available, record that in Gaps rather than marking it passing; it has been
carried forward once already by being quietly deferred.

**Step 3 is the irreversible one and it is a human step.** A wrong `os`/`cpu`, a
placeholder version, or a missing binary in `bin/` becomes a permanent published
artifact. Step 2 exists only to catch that, and it should not be rushed because
the pipeline looks ready.

**The pipeline has never run.** Four jobs, all plausible, none exercised on a
tag. The likely failures are unglamorous: a runner label, an artifact path, a
permissions scope. Budget for the first tag failing on something mechanical, and
prefer fixing forward with `0.1.1` over deleting and re-pushing a tag — a
re-pushed tag with different content is the one thing the idempotence check
cannot protect against.

**`latest` currently points at the `0.0.1` placeholder.** Publishing `0.1.0`
moves it, which is the intent. Worth confirming after the publish rather than
assuming: a `dist-tags` left on a placeholder is exactly the kind of thing
nobody notices until a user reports installing an empty package.

## Out of scope for this cycle

- **The Homebrew tap.** [Plan 2](./02-homebrew-formula.md), which needs this
  release to exist before a formula can point at one.
- **`1.0.0`.** A later decision, taken on evidence from this release.
- **Code signing and notarisation.** §9 deferred it and this cycle does not
  change the argument — a binary from npm meets Gatekeeper the same way it did
  before it was published.
- **The `ai-plugins` cutover (Phase 5).** A different repository, gated on this.
- **`cargo install` / crates.io.** Unchanged exclusion from `distribution`.

## Gaps surfaced during execution

*(filled in during execution)*
