---
type: vwf-plan
title: embed-the-binary — 2026-08-22
description: Put the binary back inside the one npm package and unwind the
  download-and-verify path, which bought nothing once the target set became one.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: [
  docs/plans/release-fix/01-apple-silicon-only.md,
]
timestamp: 2026-08-22T17:30:00Z
tags: [ distribution, npm, installer ]
---

# Plan: embed-the-binary — 2026-08-22

## Slice

Contract §9. `@askviraj/claude-status` carries the binary again. `--install`
copies it out of the package instead of downloading it, and the entire
fetch-and-verify mechanism is deleted rather than maintained.

**This reverses the previous cycle deliberately, not accidentally.** See the
folder index for the comparison table; the short version is that every benefit
the download delivered came from *not having per-platform packages*, and plan 1
delivers all of them by leaving exactly one target.

## Current state (actual)

- `installer/src/modules/binary.ts` — reads `bin/checksums.json`, resolves a
  release URL, `download()`s it with a 120 s `AbortSignal.timeout`, hashes it,
  compares, and `place()`s it via temp-then-rename. Three distinguishable
  failure modes (`offline`, `missing`, `corrupt`), a
  `$CLAUDE_STATUS_RELEASE_BASE` test seam, and a `proxyHint()` for Node's fetch
  ignoring `HTTPS_PROXY`.
- `.config/mise/tasks/build/installer` — generates `checksums.json` from
  `supported_targets()` and fails if it pins fewer than `target_count()`.
- `.github/workflows/release.yml` — a `Collect the release assets` step that
  cross-checks every binary against the manifest, and a
  `Create the GitHub
  Release` step that must run **before** the npm publish.
- `installer/test/installer.test.mjs` — a stand-in release server in **its own
  process**, because `run()` uses `execFileSync` and an in-process server would
  deadlock against it. Six tests: success, 404, digest mismatch,
  dry-run-does-no-network, the recorded digest,
  uninstall-keeps-an-edited-binary.
- The staged package is 15.5 KB and fetches ~1.0 MB at install time.

## Target state (per contract)

`build:installer` stages `target/aarch64-apple-darwin/release/claude-status`
into the package's `bin/`. `--install` copies it to `~/.claude/bin/` and records
its digest. No manifest, no network, no verification — because there is no
second artifact to distrust. npm's own immutability is the whole integrity
story, which is where it sat before last cycle and where it belongs with one
target.

The package goes to ~1.0 MB packed. `--install` becomes offline again:
air-gapped installs work, and `HTTPS_PROXY` stops being anything this tool has
an opinion about.

## Delta — ordered steps

### 1. Stage the binary into the package

`build:installer` installs the built binary at `bin/claude-status`, mode 755,
and fails if it is not there — a package staged without its binary is not
releasable and should not stage quietly.

Delete the `checksums.json` generation and its completeness check. Add
`preferUnplugged: true` to `npm/claude-status/package.json`: the package now
carries an executable, and Yarn PnP must not leave it inside a zip.

→ **verify:** `npm pack --dry-run` lists `bin/claude-status` and no
`checksums.json`; the tarball is ~1 MB; the staged binary answers `--version`
with the crate version.

### 2. Reduce `binary.ts` to resolving a local path

What goes: `Manifest`, `readManifest`, `manifestPath`, `resolve`, `download`,
`place`, `BinaryFetchError`, `DownloadError`, `releaseBase`, `TIMEOUT_MS`,
`describe`, `proxyHint`, and the `$CLAUDE_STATUS_RELEASE_BASE` seam.

What remains: `hostKey()`, a `supportedPlatforms()` that no longer reads a
manifest, and a copy-and-chmod. The host check stays — plan 1 makes npm the
first gate, but a forced install still deserves a real message rather than a
crash.

`install()` in `install.ts` stops `await`ing a download and stops catching fetch
errors. It still records the digest, now `sha256()` of the staged file.

→ **verify:** `tsc --noEmit` clean; no reference to `checksums`, `fetch` or
`RELEASE_BASE` survives anywhere under `installer/src`.

### 3. Delete the download tests and the server harness

The six download tests go, along with `SERVER_SOURCE`, the spawned server, the
mode file and `serving()`. The fixtures return to a binary written beside the
bundle.

**Two of the six do not go**, because they test things that still exist and were
never about downloading:

- the receipt records the binary's digest;
- `--uninstall` keeps a binary edited since install.

Both are rewritten against the staged binary rather than a downloaded one.

→ **verify:** the suite passes with the server harness deleted; the two
surviving tests still fail if the digest is not recorded.

### 4. Simplify the release workflow

The `Collect the release assets` step and the manifest cross-check go. The
ordering constraint — release before npm publish — goes with them, because
nothing in the published package points at a release any more.

**Keep creating the GitHub Release.** It is still the artifact a user grabs to
install by hand, and a Homebrew tap would want it. It is simply no longer load
bearing for npm.

→ **verify:** a tag push produces a release with the binary and `SHA256SUMS`,
and publishes an npm package that works whether or not that release exists.

### 5. Rejoin the version lines at `0.1.0`

`Cargo.toml` goes to `0.1.0`. `npm/claude-status/package.json` reverts to the
`0.0.0-managed-by-cargo` placeholder, and `build:installer` substitutes the
crate version into it again and re-asserts they agree — the stamp check that was
removed when the lines were split.

The reason for splitting them was to iterate on the *fetch path* without burning
binary versions; step 2 deletes that path. Keeping the split would leave one
artifact making two version claims about itself.

→ **verify:** `--version` reports `0.1.0`; the staged binary reports `0.1.0`;
`build:installer` fails if the manifest is hand-edited away from the crate
version.

### 6. Delete the `v1.0.0` tag and release

Going `1.0.0` → `0.1.0` → `1.0.0` with a release parked at the first reads as a
mistake in the tag history. Nothing consumes it — npm was never published — so
both the release and the tag go, leaving one clean `v1.0.0` for the real one.

Safe **only** because nothing installed from it. If it had been consumed the
answer would be to leave it and move forward instead.

→ **verify:** `gh release list` shows no `v1.0.0`; `git ls-remote --tags`
agrees.

### 7. Docs

- `readme.md` — the download paragraph, the network requirement and the proxy
  note all go. Plan 4 rewrites this file wholesale for npm; this step only has
  to not leave it lying. Sequence 4 after this one and the two do not fight.
- `--help` — `WHAT --install DOES` loses "downloaded from its GitHub release".
- Contract §9 — a further amendment. It should say what changed **and** that the
  previous amendment's reasoning was sound for two targets, so a future reader
  does not read a reversal as a mistake.

→ **verify:** `/vwf:docs-sync` over the cycle's commit range.

## Acceptance criteria (from contract)

1. `npx @askviraj/claude-status --install` works with **no network access**.
2. The published tarball contains the binary; nothing is fetched at install.
3. `--uninstall` still leaves the tree byte-identical, and still keeps a binary
   edited since install.
4. `--dry-run` still reports the digest and changes nothing.
5. No `fetch`, no `checksums.json`, no release-base seam under `installer/src`.
6. A tag push still produces a GitHub Release carrying the binary.
7. `--version`, the staged binary and `Cargo.toml` all report `0.1.0`, and
   `build:installer` fails if they disagree.

## Risks / drift

**The package is ~65× larger** — 15.5 KB to ~1.0 MB. That is the honest price of
an offline install, and it is what the package was before last cycle.

**Reverting a cycle that shipped one commit ago looks like churn** unless the
reason is written down. It is written down in the folder index; the contract
amendment must carry it too, or the repo's own history argues against itself.

**The `--install` output stops naming a version it fetched.** `(fetched 1.0.0)`
was a genuinely useful line while the package and binary versions differed. Keep
something equivalent — the binary's version is knowable from the staged file, so
the line can survive as `(1.0.0)`.

## Out of scope for this cycle

- **Releasing `1.0.0`.** That happens once the installer has been tested, with
  the crate and the package bumped together. This cycle only rejoins the two
  lines at `0.1.0`.
- **Signing and notarization.** Unchanged by where the binary travels.

## Gaps surfaced during execution

*(filled in during execution)*
