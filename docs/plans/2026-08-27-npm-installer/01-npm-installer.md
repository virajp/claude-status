---
type: vwf-plan
title: npm-installer — 2026-08-27
description: Cycle plan (a diff) adding npx as a third install channel — a
  dependency-free installer that downloads the release asset against a digest
  pinned in the npm package, places it on PATH, and unwires cleanly.
status: draft
covers: [
  docs/decisions.md,
]
requires: []
timestamp: 2026-08-27T17:22:00Z
tags: [ distribution, npm, npx, installer, receipt, ci, release ]
---

# Plan: npm-installer — 2026-08-27

No dependency chain. One plan, standalone.

## Slice

§11 Distribution. `npx @virajp.dev/claude-status --install` becomes a third
route to a `claude-status` on `PATH`, beside
`brew install virajp/tap/claude-status` and
`mise use --global "github:virajp/claude-status@latest"`.

**The npm package carries no binary.** It is an installer and nothing else,
invoked through `npx` / `pnpx` / `bunx` and never globally installed. Its
version, its tag and its digest all describe an artifact that lives on a GitHub
Release.

## What reverses, and why

Two closed decisions, each quoted rather than paraphrased, because a reversal is
only reviewable against what it reverses.

### `drop-npm` — "npm is retired as a channel"

> "Awkward for a repo with no other JS" turned out to mean **a second language
> in the tree for the life of the project**: a TypeScript installer, its own
> test suite, `tsup`, `pnpm`, a lockfile, a `tsconfig`, and node in the
> toolchain — **all of it to deliver a Rust binary that needs none of it.** —
> [decisions.md:1483](../decisions.md)

**Seven costs are named. Six do not come back:**

| Cost                  | Now                                                                                   |
| --------------------- | ------------------------------------------------------------------------------------- |
| TypeScript installer  | one `.mjs`, no types, no compile step                                                 |
| `tsup`                | none — the published file is the tracked file                                         |
| `pnpm`                | none                                                                                  |
| a lockfile            | none — zero runtime dependencies, so there is nothing to lock                         |
| a `tsconfig`          | none                                                                                  |
| node in the toolchain | node stays a **test-time** tool, exactly as `tests/site.rs` already uses it           |
| its own test suite    | **this one does come back** — as `tests/npm.rs`, inside the Rust suite, not beside it |

**The seventh is the honest residual and is accepted, not dismissed.** There is
JavaScript in the tree again, and it will need maintaining for the life of the
channel. What the plan buys against it is that the file has no build, no
dependency graph and no second `test` command — `mise run code:test` still runs
everything.

**The other half of `drop-npm`'s argument is unaffected.** It said the thing npm
bought was the `pnpx` invocation, and that `--configure` had made it
unnecessary. That is still true — this is not a *replacement* for the tap, and
nothing here argues it should be. It is a route for people who reach for `npx`
before `brew`.

### `release-fix` — "the binary moved out of the npm package, then back in"

> **only its costs remained**: a required network call, air-gapped installs
> broken, `HTTPS_PROXY` unhonoured by Node's `fetch`, a release that had to
> precede the npm publish, and a digest manifest maintained against a mutable
> asset. — [decisions.md:1380](../decisions.md)

**Every one of those costs is real and is being taken.** What changed is the
comparison, not the cost: that passage weighs downloading against *embedding*,
and concludes embedding is strictly better. **This plan is not choosing between
them.** A third channel that embedded the binary would be a fourth copy of the
bytes with a fourth digest to keep true; the user's spec rules it out, and the
plan agrees with the spec.

**The integrity argument is taken up rather than dropped.** The same passage
explains why:

> A release asset is mutable — it can be deleted and re-uploaded at the same URL
> — **and an npm version is not**. — [decisions.md:1401](../decisions.md)

So the digest is pinned **inside the published package**, written at publish
time from the release's own `SHA256SUMS`, and the trust root is npm's
immutability. This is the same shape as the formula's `sha256`, which
[decisions.md:1650](../decisions.md) calls "the only thing standing between a
user and substituted bytes".

**"A release that had to precede the npm publish" is now free.** `publish-npm`
runs *after* the release job in the same workflow, which is where `bump-tap`
already runs for the same reason.

## Current state (actual)

- `supported_targets()` has one row: `aarch64-apple-darwin darwin arm64`
  (`.config/mise/tasks/_scripts/_rust`). Assets are
  `claude-status-darwin-arm64`, `claude-status-darwin-arm64.tar.gz` and
  `SHA256SUMS`.
- `release.yml` runs `verify → test → build → publish → bump-tap`. `bump-tap`
  reads the published release's asset name, URL and digest.
- The binary's CLI (`src/_runtime/cli.rs:158`) accepts nine flags: `--version`,
  `--help`, `--debug`, `--dry-run`, `--statusline`, `--subagent`, `--refresh`,
  `--caps-hook` and `--configure`. **There is no `--uninstall`, and nothing
  unwires `settings.json`.**
- `tests/site.rs:97` (`no_javascript_lockfile_or_node_modules_is_tracked`) fails
  on **any** tracked `package.json`, by name, with a comment saying a manifest
  "is precisely how the npm ecosystem comes back".
- `~/.config/claude-status/` holds config; `~/.cache/claude-status/` holds the
  spend cache. `docs/decisions.md:443` records the split and says explicitly not
  to tidy one into the other.

## Target state

`npm/install.mjs`, published as `@virajp.dev/claude-status`, with three flags.

### `--install`

1. **Platform gate.** `"os": ["darwin"], "cpu": ["arm64"]` in the manifest, so
   npm refuses with `EBADPLATFORM` before a line of the installer runs, plus a
   runtime check that names the host it will not serve. **This restores the gate
   `drop-npm` deleted** — [decisions.md:1507](../decisions.md) records that
   window as knowingly open, and it closes here for this channel.
2. **Classify what is already installed.** `which claude-status`, then resolve
   the symlink to a real path:
   - a `/Cellar/` path segment → Homebrew. Print `brew upgrade claude-status`
     and exit 0.
   - `mise which claude-status` agrees with the resolved path → mise. Print
     `mise upgrade claude-status` and exit 0.
   - neither, and the receipt records this installer placed it **and** the
     digest still matches → upgrade in place, printing `old → new`.
   - neither, and there is no receipt, or the file changed since → **refuse**,
     naming the path. `--force` overrides.
3. **Choose a directory**, first match wins: `~/.local/bin` if on `PATH`; then
   `~/bin` if on `PATH`; then the first `PATH` entry under `$HOME` that is
   user-owned and writable. **Never a directory outside `$HOME`** — not
   `/usr/local/bin`, not `/opt/homebrew/bin`. Nothing qualifies → create
   `~/.local/bin`, install there, print the shell's `PATH` line, **exit 1**.
4. **Download** `claude-status-darwin-arm64.tar.gz` for the pinned tag into a
   temp directory. Verify SHA-256 against the digest in this package. A mismatch
   is **fatal, reported as itself, with an explicit instruction not to retry** —
   the wording [decisions.md:1401](../decisions.md) settled on, because a
   mismatch is not a flaky download.
5. **Extract** (shelling out to `tar`; Node has none), `chmod 755`, then
   `rename` into place — atomic, so a failed install never leaves a partial
   binary on `PATH`.
6. **Execute it.** `claude-status --version` must equal the pinned version
   exactly. [decisions.md:815](../decisions.md) guarantees `--version` prints
   the bare version and nothing else, which is what makes that safe to match on;
   `release.yml`'s "Verify the built binary" step already does this.
7. **Write the receipt.**
8. **`--configure`.** Three states, and the third exists so a script can say
   *no* as explicitly as it can say yes:
   - `--install --configure` → run it. Explicit consent.
   - `--install --no-configure` → skip it, print the one line the user would
     have to run, exit 0. **A decline, not a failure.**
   - neither → prompt on a TTY; skip silently when there is none.

   **`--configure` and `--no-configure` together is an error**, not a precedence
   rule. `--configure` is the one surface in this project that already refuses
   an argument it does not understand ([decisions.md:965](../decisions.md)), and
   a contradiction resolved silently is how a script ends up doing the opposite
   of what it says.

### `--uninstall`

Removes the binary, receipt-guarded exactly as step 2 guards the upgrade, and
unwires `statusLine`, `subagentStatusLine` and the `PostToolUse` entry from
`~/.claude/settings.json`, **keeping another tool's hooks in that array**.
Leaves `~/.config/claude-status/config.json` alone.

### `--help`

`--install`, `--uninstall`, `--help`, and the three modifiers — `--configure`,
`--no-configure`, `--force` — with what each writes.

### The receipt

`~/.local/state/claude-status/install-receipt.json`, holding the version, the
tag, the install path, the binary's SHA-256, and whether `--configure` ran.

**Not `~/.config/claude-status/`.** That directory is the one people commit to a
dotfiles repo — [decisions.md:443](../decisions.md) — and a receipt naming this
machine's install path would arrive on the second machine claiming a binary that
is not there. **Not `~/.cache/`** either: that holds things that are
regenerable, and clearing a cache must not strand the uninstall.

## Delta — ordered steps

Each step names the failing test that defines done. All new tests land in
`tests/npm.rs` unless stated.

1. **Widen the JS ban to exactly one manifest.** Amend
   `no_javascript_lockfile_or_node_modules_is_tracked` (`tests/site.rs:97`) to
   permit `npm/package.json` and nothing else — a `package.json` at any other
   path still fails, and every lockfile still fails. Its doc comment records
   *why* the ban existed and why one manifest is now allowed. → the amended test
   passes with no `package.json` tracked yet, and fails if the allowance is
   written as a name rather than a path.
2. **`npm/package.json`.** Name, `bin: { "claude-status": "install.mjs" }`,
   `os`/`cpu`, `engines.node >= 20`, `files`, no dependencies. →
   `the_package_version_equals_the_crate_version` — reads `crate_version()`
   through the `bash()` helper `tests/release.rs:38` already uses. →
   `the_package_declares_the_only_platform_the_release_carries` — `os`/`cpu`
   derived from `supported_targets()`, not hard-coded twice.
3. **Argument parsing and the platform gate.** →
   `every_flag_the_help_lists_is_a_flag_the_parser_accepts` →
   `an_unsupported_host_is_named_rather_than_attempted`
4. **Channel classification.** → `a_cellar_path_is_classified_as_homebrew` →
   `a_mise_shim_is_classified_as_mise` →
   `an_unknown_binary_is_refused_rather_than_overwritten` →
   `a_receipt_match_is_an_upgrade_rather_than_a_refusal`
5. **Install-directory selection.** →
   `the_install_directory_never_resolves_outside_home` — the one that matters;
   it is the difference between this installer and one that can write to
   `/usr/local/bin`. → `no_writable_path_entry_still_installs_and_exits_nonzero`
6. **Asset naming, download, digest.** →
   `the_asset_name_matches_the_one_the_release_uploads` — sources `_rust` and
   compares against `asset_name darwin arm64`, so the installer and the release
   workflow **cannot drift**. →
   `a_digest_mismatch_is_fatal_and_says_not_to_retry` →
   `a_failed_verification_leaves_nothing_on_path`
7. **Place, chmod, verify by execution.** →
   `a_version_mismatch_after_install_is_a_failure` — using a shim binary, the
   pattern `tests/e2e.rs` already uses.
8. **Receipt.** → `the_receipt_records_the_digest_of_what_was_actually_placed`
9. **`--configure` consent.** →
   `configure_runs_only_on_explicit_consent_or_a_tty` →
   `no_configure_declines_without_prompting_and_names_the_command` →
   `configure_and_no_configure_together_is_refused_rather_than_ranked`
10. **`--uninstall`.** → `the_unwire_is_the_exact_inverse_of_configure` — take
    the settings.json `--configure` actually writes, run the unwire, assert the
    file is byte-equal to what it was before. **This is what makes it safe to
    put settings.json editing in a second language**, and it is the round trip
    [decisions.md:1536](../decisions.md) records as previously unverifiable. →
    `the_unwire_keeps_another_tools_posttooluse_hooks` →
    `the_uninstall_refuses_a_binary_it_did_not_place`
11. **`publish-npm` in `release.yml`**, after `publish`, reading that tag's
    `SHA256SUMS`, injecting version + tag + digest, publishing via Trusted
    Publisher (OIDC). A staging mise task under `.config/mise/tasks/release/`
    does the injection so it is runnable and readable outside CI. →
    `the_publish_npm_job_pins_a_digest_from_the_published_release` →
    `a_manual_dispatch_cannot_publish_to_npm` — the sibling of
    `a_manual_dispatch_cannot_publish_a_release` (`tests/release.rs:323`) →
    `the_publish_npm_job_installs_no_tools_it_does_not_use` — the sibling of
    `the_publish_job_installs_no_tools` (`tests/release.rs:285`)
12. **Docs.** A third route in `site/content/install.md`, giving the command for
    **all three runners** — `npx`, `pnpx` and `bunx` — with arguments, and the
    flags in the same table shape the page already uses for the binary's
    surfaces. `readme.md` if it enumerates channels. Both halves of the reversal
    in `docs/decisions.md` §11. →
    `the_install_page_names_every_runner_the_package_supports` (in
    `tests/site.rs`, beside the existing content guards)

## Prerequisites outside this repository

Neither can be done by a commit here. **Both must land before the next tag**, or
`publish-npm` fails a release that is otherwise fine.

1. **Own the `@virajp.dev` scope on npmjs.com.** The scope is the constraint,
   not the name: npm accepts a scoped publish only under a scope its publisher
   owns. `@askviraj/claude-status`, which [decisions.md:1660](../decisions.md)
   records as unclaimed, **stays** unclaimed — this cycle publishes elsewhere
   rather than taking it. Unscoped `claude-status` was measured taken on
   2026-08-27.
2. **Configure Trusted Publishing** on npmjs.com for this repo and the
   `publish-npm` job. **Resolved 2026-08-27, during execution:** npm will not
   attach a trusted publisher to a package that does not exist (`npm/cli#8544`,
   open), so the first publish of the name is manual or token-authenticated and
   OIDC takes over from the second.

## Risks / drift

- **`publish-npm` can fail a green release.** `bump-tap` already has this shape.
  It must not run before the release exists, and a failure must be legible as
  "the release shipped, the npm publish did not" rather than as a broken
  release.
- **Three channels, one digest source.** The formula and the package both pin a
  digest read from the same `SHA256SUMS`. A re-run of a tag producing different
  bytes now breaks two channels rather than one — which is what
  `reproducible_tar` exists to prevent, and `tests/release.rs:57` pins.
- **The installer hard-codes an asset name.** Mitigated by step 6's test, which
  is the only thing standing between a target rename and a channel that 404s.

## Residuals accepted

Stated rather than implied, and each belongs in `docs/decisions.md`:

- **`HTTPS_PROXY` is not honoured.** Node's `fetch` ignores it, and
  [decisions.md:1421](../decisions.md) lists this among the reasons the download
  path was deleted. It returns with this channel. Users behind a proxy use the
  tap.
- **Air-gapped installs do not work on this channel.** Inherent. The tap does.
- **The binary is unsigned and unnotarised.**
  [decisions.md:1672](../decisions.md) already records this as unowned for the
  tap. `fetch` does not set the `com.apple.quarantine` xattr, so the binary runs
  — but the risk is the same one, now on a second channel.
- **JavaScript is back in the tree**, one file, no build. See the reversal
  section above for what that is and is not.
- **The installer has no linter.** `code:lint` is clippy, actionlint and
  shellcheck; dprint formats `.mjs` already. Precedent:
  `site/public/config-generator.js`.

## Out of scope for this cycle

- **Adding `--uninstall` to the Rust binary.** The unwiring lives in the
  installer, pinned by the round-trip test in step 10. If a second channel ever
  needs it, that is when it moves into the binary.
- **A `curl | sh` installer.** A different channel with a different argument.
- **New targets.** `supported_targets()` is untouched; the bar
  [decisions.md:1340](../decisions.md) sets — a native runner that builds *and
  runs the suite* per target — is unchanged and unmet.
- **Sweeping `ai-plugins` orphans.** `drop-npm` dropped that path knowingly;
  this channel does not pick it back up.

## Gaps surfaced during execution

<!-- Appended by execution. Do not fill at plan time. -->

- ...
