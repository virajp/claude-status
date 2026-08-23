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

The two entries below are one theme — making the config file writable by hand
without guessing — and would likely make one slice rather than two.

### Pin the `$schema` URL, and publish the schema

**Ask.** Editors should resolve the schema that matches the *installed* binary,
and ideally find it without the key being present at all.

**Already true — every generated config carries the key, and a plan should not
redo that.** `schemas/claude-status.schema.json` exists, is a draft-2020-12
schema with `additionalProperties: false`, and declares `$schema` as a permitted
key. All three writers emit it: `assets/claude-status.defaults.json` (what
`--install` seeds the user layer from, byte-for-byte),
`installer/src/_runtime/configure.ts` and `src/modules/config/autoseed.rs` (the
repo layer, seeded and migrated alike — both from a `SCHEMA_URL` constant).

**What is actually left.** The `$id`/`$schema` URL points at the mutable `main`
ref, so an editor resolves whatever `main` says today rather than the schema
matching the installed binary — worth deciding whether to pin to a tag once
[`release`](./2026-08-21-2122-release.md) ships. And whether to publish to
SchemaStore, so editors find it without the key at all.

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

## Closed

Kept as a record of what was raised and where it went, so a later reader does
not re-open any of them.

- **Nothing seeded the repo-level config layer.** Shipped as the installer's
  `--configure` (`d97dc4b`), with the binary growing a matching autoseed on the
  render path (`60f5c0f`) so a repo gets its layer without anyone running a
  command. It answered its own open questions: the **installer** owns the flag,
  not the binary; an existing `claude-status.json` is kept and only gains a
  missing `projectName`; a lone `statusline.json` is rewritten rather than
  renamed (`c42aabc`) with its `$schema` repointed; the name is the directory
  basename, not the git remote; and it honours `--dry-run`. `projectName` became
  repo-level only with `autoConfigureRepo` defaulting on (`03f6773`).

- **An unresolvable `$HOME` had no defined meaning.** Raised by security review
  during `macos-only`, deferred, then fixed in that same cycle. The contract now
  says absent-never-relative and the four callers agree with it.

- **Path-derived segment text reached the row unfiltered.** Same origin, same
  outcome: filtering now sits at the single point every segment's text passes
  through.
