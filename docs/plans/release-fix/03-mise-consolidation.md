---
type: vwf-plan
title: mise-consolidation — 2026-08-22
description: Move every tool dev and CI share into mise.toml, with one rust
  profile for both, so a toolchain that works locally cannot be missing in CI.
status: active
covers: [
  docs/blueprint/conventions.md,
]
timestamp: 2026-08-22T17:30:00Z
tags: [ mise, ci, tooling ]
---

# Plan: mise-consolidation — 2026-08-22

## Slice

The three-file mise split. Tools dev and CI both need move into the base config;
the per-environment files keep only what is genuinely per-environment.

## Current state (actual)

```toml
# .config/mise.toml          — loaded everywhere
[tools]
"core:rust" = { profile = "minimal", version = "latest" }
node        = { version = "latest" }
pnpm        = { version = "latest" }

# .config/mise.dev.toml      — MISE_ENV unset
[tools]
"core:rust" = { profile = "default", version = "latest" }   # ← silently overrides
dprint, gitleaks, grype, pre-commit, taplo

# .config/mise.ci.toml       — MISE_ENV=ci
[tools]
# "Usually empty — CI reuses the runtime from mise.toml."
```

**This is what broke the first release.** CI runs `mise run code:lint`, which is
`cargo clippy … -- -D warnings`. The `minimal` rust profile carries no clippy.
`mise.dev.toml` overrides the profile to `default`, so a maintainer's machine
has clippy and CI does not — and the failure is invisible until a pipeline runs:

```
error: 'cargo-clippy' is not installed for the toolchain '1.98.0-aarch64-apple-darwin'
```

The override is not documented as an override. Reading `mise.toml` alone tells
you this repo builds with a minimal toolchain, which is true of exactly one of
the two environments that matter.

## Target state (per contract)

`mise.toml` carries **everything dev and CI share**, resolved identically in
both: rust at one profile, node, pnpm. `mise.dev.toml` keeps only tools CI never
runs. `mise.ci.toml` returns to genuinely empty.

**One rust profile, and it is `default`.** The two environments run the same
`code:*` tasks, so they need the same components. `minimal` is right only for a
config whose sole job is producing a binary, and this one's is not — it backs a
task library that lints and formats. The cost is that every consumer downloads
clippy and rustfmt; the benefit is that "works on my machine" and "works in CI"
stop being different questions about the toolchain.

The split's remaining job is unchanged and still worth having: environment-only
tools and environment-specific *values*.

## Delta — ordered steps

### 1. Move the shared tools into the base

`"core:rust" = { profile = "default", version = "latest" }`, `node`, `pnpm` in
`.config/mise.toml`. The rust line gains a comment saying why `default` and not
`minimal` — that CI lints, and clippy is not in `minimal`. A future reader
optimising the toolchain download will otherwise re-introduce exactly this bug.

→ **verify:** `MISE_ENV=ci mise ls --current` lists rust, node and pnpm, all
resolved from `mise.toml`.

### 2. Strip the dev override

`.config/mise.dev.toml` loses its `"core:rust"` line. It keeps `dprint`,
`gitleaks`, `grype`, `pre-commit` and `taplo` — the pre-commit tool chain, which
CI does not run.

→ **verify:** `mise ls --current` locally shows rust resolved from `mise.toml`,
not `mise.dev.toml`. That single line moving is the whole fix.

### 3. Return `mise.ci.toml` to empty

Keep the file and its comment. It documents where CI-only tools and production
env values go, and an empty file that explains itself is worth more than a
deleted one.

→ **verify:** the file declares no tools; CI resolves everything from base.

### 4. Prove the two environments agree

Add a check to `code:precommit` — or a small task — that resolves the tool list
under both `MISE_ENV` settings and fails if the shared set differs. The bug this
plan fixes was a silent divergence; nothing structural currently prevents the
next one.

Keep it narrow: assert the *shared* tools match, not that the lists are
identical, because dev is supposed to have more.

→ **verify:** the check fails if a `"core:rust"` line is put back into
`mise.dev.toml`.

### 5. Docs

`readme.md`'s build section names the mise tasks; plan 4 moves that content out
of the readme entirely, so this step only has to keep whatever survives correct.
Note the profile decision wherever conventions are recorded.

→ **verify:** `/vwf:docs-sync` over the cycle's commit range.

## Acceptance criteria (from contract)

1. `mise.toml` declares rust, node and pnpm; `mise.dev.toml` declares no rust.
2. `MISE_ENV=ci mise run code:lint` succeeds on a clean checkout — clippy is
   present.
3. `mise.ci.toml` declares no tools.
4. A re-introduced dev override fails the check from step 4.

## Risks / drift

**Every environment now downloads a larger rust toolchain.** `default` pulls
clippy and rustfmt where `minimal` did not. Small, one-off per version, and the
alternative is the failure this plan exists to fix.

**`pnpm` is fine only because Intel is gone.** It has no macOS x64 standalone
build at any recent version, so before plan 1 a base-level `pnpm` was
unresolvable on an Intel runner. Consolidating it into the base is safe *because
of* plan 1, and that dependency is worth stating: re-adding an Intel target
means this line stops working, and the fix would be installing pnpm from npm
rather than as a binary.

## Out of scope for this cycle

- **Pinning exact tool versions.** Everything is `latest`; changing that is a
  separate argument about reproducibility.
- **Running `code:sec` or `code:format` in CI.** They are pre-commit's job
  today. If CI ever runs them, `gitleaks`, `grype` and `dprint` move to the base
  by the same rule this plan applies.

## Gaps surfaced during execution

*(filled in during execution)*
