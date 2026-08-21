---
type: vwf-plan-backlog
title: Backlog — claude-status
description: Ideas accepted but not yet planned. Each entry is a candidate
  slice, not a commitment, and carries what is already true so a plan does not
  re-derive it.
status: stable
---

# Backlog

Things worth doing, not yet cut into a cycle. An entry graduates by becoming a
plan doc in [`docs/plans/`](./index.md) — until then nothing here is a promise,
and the "already true" notes are the point: they are what stops a future plan
from re-discovering the same ground.

## Config ergonomics

The three entries below are one theme — making the config file writable by hand
without guessing — and would likely make one slice rather than three.

### `--configure` — seed a repo-level config

**Ask.** A `--configure` argument that runs **only inside a repo**, creates the
repo-level config, and sets `projectName` to the repository's name.

**Why.** `--install` seeds the user-level config at
`~/.config/claude-status.json`, and nothing seeds the repo-level layer at
`<repo-root>/.config/claude-status.json`. Today that file is written by hand or
not at all, so the `project` segment — which is omitted unless `projectName` is
set **in config** — silently never appears for most repos.

**Already true.** The three-layer merge exists and the repo layer already wins
([contract §2](../spec/statusline-behaviour.md), readme "Configuration"). Git
root resolution is already implemented and filesystem-first in
`src/modules/git.rs`, so "am I in a repo, and what is it called" needs no new
mechanism. This repo's own `.config/claude-status.json` is exactly the file the
command would generate, and is a good shape to copy.

**Open questions a plan must answer.** Which binary owns it — this is the
*binary's* flag, not the npm installer's, so it sits beside `--statusline` /
`--caps-hook` rather than beside `--install`. What happens when the file already
exists (refuse? merge? `--force`?). Whether the repo name comes from the
directory basename or the git remote, which disagree for a fork or a renamed
checkout. Whether it honours `--dry-run`, which the *installer* has and the
binary does not.

### `$schema` in every generated config

**Ask.** Both the user-level and repo-level config files should carry a
`$schema` key so editors offer completion and catch mistakes.

**Already true — most of this is done, and a plan should not redo it.**
`schemas/claude-status.schema.json` exists, is a draft-2020-12 schema with
`additionalProperties: false`, and declares `$schema` as a permitted key.
`assets/claude-status.defaults.json` — the single source the installer seeds
from, byte-for-byte — already carries
`"$schema": "https://raw.githubusercontent.com/virajp/claude-status/main/schemas/claude-status.schema.json"`,
and so does this repo's own `.config/claude-status.json`. So a **freshly
installed user config already has it**.

**What is actually left.** Whatever `--configure` writes must carry it too. And
the `$id`/`$schema` URL points at the mutable `main` ref, so an editor resolves
whatever `main` says today rather than the schema matching the installed binary
— worth deciding whether to pin to a tag once
[`release`](./2026-08-21-2122-release.md) ships, and whether the schema should
be published to SchemaStore so editors find it without the key at all.

### Config validation in `--debug`

**Ask.** `--debug` should validate the config and report what is wrong.

**Why.** Every layer is currently *silently* forgiving: a layer that is missing,
malformed, or not a JSON object is ignored rather than fatal
([contract §2](../spec/statusline-behaviour.md)), which is correct for a bar
that redraws every four seconds and must never fail to render — but it means a
typo'd key is indistinguishable from a key that does nothing. The only feedback
today is the unknown-`lines`-id warning on stderr.

**Already true.** `--debug` already reports the three config layers and which
resolved, so the reporting surface exists and this is a new section in it, not a
new command. `additionalProperties: false` in the schema means an unknown key is
already *defined* as an error — the definition just is not enforced anywhere.

**Open questions a plan must answer.** Whether to validate against the JSON
schema (a dependency, and a second source of truth to keep in step with the Rust
types) or to have the deserializer report unknown fields (`serde`'s
`deny_unknown_fields` on a shadow type, no dependency, but duplicates the
schema's job). Whether validation stays `--debug`-only — it must never make a
render fail. Whether it exits non-zero, which would let a setup script check a
config.

## Deferred from review

Both were raised by security review during the `macos-only` cycle, rated `low`,
and declined there as out of scope. Neither is a regression; both are
pre-existing.

### An unresolvable `$HOME` has no defined meaning

`src/_shared/paths.rs`'s `home()` returns `Option`, and its four callers each
invented a different fallback. One of them — `src/modules/spend/cache.rs` —
falls back to the **relative** path `spend.json`, so with no `$HOME` the spend
cache is written into whatever directory Claude Code was launched from. The
contract never says what an unresolvable home means, which is the root cause:
fix the contract first, then the four call sites. Absent is almost certainly the
right answer rather than relative.

### Path-derived segment text is not filtered before it reaches the row

`worktree_subpath` and the branch segment interpolate path- and git-derived
strings into the ANSI row with no C0/ESC filtering, so a directory or branch
named with escape sequences can spoof the status bar. The contract specifies
which escapes the *renderer* emits ([§4](../spec/statusline-behaviour.md)) but
never which bytes a *dynamic value* may carry into one. Affects every
path-derived segment, so the fix belongs in the renderer rather than in each
producer.
