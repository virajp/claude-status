---
type: vwf-plan
title: schema-and-validation — 2026-08-23
description: Cycle plan (a diff) making the Rust config types generate the JSON
  schema, adding a validation section to --debug, and listing the schema on
  SchemaStore.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: [
  docs/plans/2026-08-23-config-ergonomics/01-typed-config.md,
]
timestamp: 2026-08-23T10:02:00Z
tags: [ config, schema, schemars, validation, debug, schemastore ]
---

# Plan: schema-and-validation — 2026-08-23

## Slice

Contract §3 (Configuration) and §5 (`--debug`). Three things, all downstream of
[plan 1](./01-typed-config.md)'s types:

1. `schemas/claude-status.schema.json` becomes **generated output** from the
   Rust types, with a drift check that fails the build if it is stale.
2. `--debug` grows a validation section that names what is wrong with the config
   instead of silently ignoring it.
3. The schema is listed on SchemaStore, so editors find it by filename.

## Current state (actual)

**After plan 1**, the config is deserialized into Rust types. Unknown keys are
still silently ignored — plan 1 deliberately left `deny_unknown_fields` off so
no behaviour moved while the types landed.

`schemas/claude-status.schema.json` is 301 hand-written lines. Nothing checks it
against the types; the two agree only by whoever last edited both.

The `$schema` and `$id` both point at
`https://raw.githubusercontent.com/virajp/claude-status/main/schemas/claude-status.schema.json`
— the mutable `main` ref. The URL is a `SCHEMA_URL` constant in two places,
`installer/src/modules/config.ts:48` and `src/modules/config/autoseed.rs:30`,
and every config either writer seeds carries it.

`--debug` already has the surface this lands in. `debug_report_with`
(`src/_runtime/app.rs:321`) prints a `CONFIG LAYERS (low to high)` section
listing each layer, whether it loaded, and its path. Today "loaded" is the only
verdict a layer gets. The only other config feedback anywhere is the
unknown-`lines`-id warning on stderr.

Contract §3's forgiveness rule stands: a layer that is missing, malformed, or
not a JSON object is ignored rather than fatal, because the bar must never fail
to render.

## Target state (per contract)

One source of truth — the Rust types — with three consumers: the renderer (plan
1), the committed schema, and the `--debug` check.

`--debug` reports per layer what it could not make sense of. It stays a
**report**: nothing it finds changes what renders, and nothing it finds makes
the binary exit non-zero on a render.

## Delta — ordered steps

### 1. Add `schemars` as a dev-dependency and a generation task

**Dev-dependency, not a dependency.** The generator runs at author time; nothing
about it belongs in a binary that redraws every four seconds. The derives sit
behind `#[cfg_attr(feature = "schema", derive(JsonSchema))]` so the release
build does not carry them.

A `mise run code:schema` task regenerates `schemas/claude-status.schema.json` in
place, matching the existing `code/*` task library.

### 2. Carry the descriptions across

The hand-written schema's value is not its shape — it is the prose. `symbols`
enumerates the nineteen keys the script consumes; `spend.show` explains why
`auto` renders only for team seats. Losing that would make this a downgrade
dressed as an improvement.

Descriptions move into `#[doc]` comments on the Rust types, which `schemars`
emits as `description`. **A generated schema that has fewer descriptions than
the hand-written one fails this cycle** — see criterion 3.

### 3. Fail the build on schema drift

`code:precommit` and CI regenerate the schema to a temp file and diff it against
the committed one. A difference fails, with the message naming
`mise run code:schema` as the fix.

Committed rather than generated-at-build because it is fetched over HTTP by
editors from the repo — it has to exist at a stable path on `main`.

### 4. Turn on `deny_unknown_fields` for the closed objects

`powerline`, `spend`, `caps`, `subagent` and the style objects. **Not** the open
maps — `palette`, `symbols`, `typeSymbols`, `segments` take user-chosen keys, so
an unknown key there is a legal entry, not a typo. This asymmetry is inherent to
the config's shape and is the honest limit of the whole feature.

Because plan 1 made a deserialize failure fall back to the embedded defaults, an
unknown key in a closed object would now blank a user's whole config rather than
be ignored. **That is a behaviour change this cycle must not ship.** So the
strict types are a *second* deserialization used only by the validator; the
render path keeps the permissive one. One set of types, two derives — the shapes
cannot drift because there is only one shape.

### 5. Report per-layer findings in `--debug`

Each layer in `CONFIG LAYERS` gains its findings underneath it, so a finding is
attributed to the file that caused it rather than to the merged result:

```text
CONFIG LAYERS (low to high)
  embedded loaded     <embedded>
  user     loaded     /Users/x/.config/claude-status.json
           ⚠ unknown key `powerlin` (did you mean `powerline`?)
           ⚠ symbols.contxt is not a key this binary reads
  repo     not found  <no git root>
```

Three finding kinds: an unknown key in a closed object; a key in an open map
outside the set this binary actually consumes (a **note**, not an error, since
it is legal); and a value that was coerced — an empty `gauge.filled`, a `0`
width, an uncompilable `worktreePattern`. The third is the most useful of the
three and the one no schema can give you: it reports what the binary *did* with
your value, not what the value is allowed to be.

### 6. Leave the URL on `main`, and list on SchemaStore

The `$id`/`$schema` URL is **not** pinned to a tag. Once the schema is generated
from the types it tracks the code by construction, and pinning would freeze an
editor on the schema of whatever version happened to seed the file — worse for
the common case where the user upgrades the binary and keeps their config.

Submit to SchemaStore's catalog matching `claude-status.json`, so editors
resolve it with no `$schema` key present at all. The key keeps being seeded
regardless: SchemaStore is a convenience, and a config that carries its own
pointer works in editors that do not consult a catalog.

**SchemaStore is an external PR into a queue outside this repo's control.** It
is the last step for that reason, and the cycle is complete without it — see
Risks.

### 7. Docs

Contract §3 records that the schema is generated from the types, that the
`--debug` validation section exists and is advisory, and that unknown-key
detection covers closed objects only. §5's `--debug` description gains the
validation section. `readme.md` gains a line under Diagnosing; `CONTRIBUTING.md`
gains `mise run code:schema`.

## Acceptance criteria (from contract)

1. Given a clean checkout, when `mise run code:schema` runs, then
   `schemas/claude-status.schema.json` is unchanged — the committed file is the
   generated file.
2. Given a Rust config type with a field added and the schema not regenerated,
   when `code:precommit` runs, then it fails and names `mise run code:schema`.
3. Given the generated schema, when its `description` strings are compared with
   the hand-written schema at this cycle's parent commit, then none has been
   lost.
4. Given a user layer with `powerlin` instead of `powerline`, when `--debug`
   runs, then the finding is reported **under the user layer**, and when the bar
   renders, then it renders exactly as it did before the typo was introduced.
5. Given a config with `gauge.width` of `0`, when `--debug` runs, then it
   reports the coercion to 10.
6. Given a config with an unknown key in `palette`, when `--debug` runs, then it
   is **not** reported as an error.
7. Given any config at all, when the bar renders, then `--debug`'s findings
   change nothing on stdout and the exit code is 0.
8. Given the release binary, when its dependency tree is inspected, then
   `schemars` is absent.

## Risks / drift

**The strict/permissive split is the subtle part.** Step 4 introduces a second
deserialization whose only job is to be intolerant. Getting this wrong in the
direction of strictness means a typo blanks a user's config — strictly worse
than today's silence, and a regression against the third invariant. Criterion 4
tests exactly that: the typo is reported *and* the bar is unaffected.

**A generated schema can be worse than a hand-written one.** The hand-written
file carries prose written for a human editing JSON. `schemars` emits structure
faithfully and prose only if it is in `#[doc]`. Criterion 3 is a real gate and
should be checked by diffing descriptions, not by eyeballing the file.

**SchemaStore is not this repo's to merge.** The catalog PR can sit, be
rejected, or ask for changes. The cycle must be able to complete without it — so
it is a step, but not an acceptance criterion. If it has not merged when
everything else has, record it in Gaps and finish; do not hold the cycle open on
someone else's queue.

**Leaving the URL on `main` is a real trade, taken deliberately.** A user on an
old binary gets an editor validating against a newer schema, which can offer a
key their binary ignores. The alternative — pinning to the seeding version —
gets the reverse and worse case: an editor rejecting a key their upgraded binary
supports, forever, because the URL was frozen at install time. Generation makes
the first case rare and self-correcting; nothing makes the second self-correct.

**The open-map limit undercuts the original ask.** The backlog wanted "a typo'd
key is distinguishable from a key that does nothing". For `symbols`, `segments`,
`palette` and `typeSymbols` — a large share of what people actually edit — it
still is not, and cannot be. Step 5's "outside the consumed set" note is the
mitigation, and it is a note rather than a warning because those keys are legal.
This should be said plainly in the readme rather than left for a user to
discover.

## Out of scope for this cycle

- **Making validation fail a render, or exit non-zero.** The backlog raised both
  as open questions; both are answered *no*. Contract §3's never-fail rule is
  not up for renegotiation in a diagnostics cycle, and a non-zero exit from
  `--debug` would make it unusable in the shell pipelines people actually run it
  in. Revisit only with a concrete need.
- **Typo detection in open maps.** Not possible; see Risks.
- **Validating the JS bar's `statusline.json`.** A different tool's config,
  retired at the Phase 5 cutover.
- **Any change to a default or a coercion.** Plan 1's exclusion still holds.

## Gaps surfaced during execution

*(filled in during execution)*
