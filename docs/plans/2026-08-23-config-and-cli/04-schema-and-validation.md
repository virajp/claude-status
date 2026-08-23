---
type: vwf-plan
title: schema-and-validation — 2026-08-23
description: Cycle plan (a diff) making the Rust config types generate the JSON
  schema, adding a validation section to --debug, and publishing the schema for
  the website's generator to consume.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: [
  docs/plans/2026-08-23-config-and-cli/03-cli-surface.md,
]
timestamp: 2026-08-23T14:04:00Z
tags: [ config, schema, schemars, validation, debug, schemastore ]
---

# Plan: schema-and-validation — 2026-08-23

## Slice

Contract §3 (Configuration) and §5 (`--debug`). The Rust types become the single
source for three consumers: the renderer (already, from plan 1), the committed
JSON schema, and a `--debug` validation section.

**This is also the plan the website waits on.**
[website/02](../2026-08-23-website/02-config-generator.md) builds its form from
the committed schema, so the schema stops being documentation and becomes an
interface.

## Current state (actual)

**After plans 1–3**, the config is deserialized into Rust types whose `Default`
provably matches the embedded JSON, lives at
`~/.config/claude-status/config.json`, and stores only non-defaults. Unknown
keys are still silently ignored — plan 1 deliberately left `deny_unknown_fields`
off so no behaviour moved while the types landed.

`schemas/claude-status.schema.json` is 301 hand-written lines. Nothing checks it
against the types; they agree only by whoever last edited both.

**The schema's real value is its prose.** `symbols` enumerates the nineteen keys
the script consumes. `spend.show` explains why `auto` renders only for team
seats. `worktreePattern`, `caps` and `lines` each carry a paragraph. This is the
part a generator can lose without anyone noticing until an editor stops being
helpful.

**`$schema` and `$id` point at the mutable `main` ref**, via a `SCHEMA_URL`
constant. After [distribution/01](../2026-08-23-distribution/01-drop-npm.md)
deletes the installer, the only remaining writer is the binary.

**`--debug` has the surface this lands in.** Plan 3 already rewrote
`CONFIG LAYERS` to distinguish absent from broken and to report repo keys that
were ignored.

**Four keys are open maps** — `palette`, `symbols`, `typeSymbols`, `segments` —
whose keys are user-chosen. The rest are closed.

## Target state (per contract)

`schemas/claude-status.schema.json` is generated output, checked for drift on
every commit. `--debug` reports what it could not make sense of, per layer, and
changes nothing about what renders. The schema is listed on SchemaStore so
editors find it by filename.

## Delta — ordered steps

### 1. Add `schemars` as a dev-dependency and a generation task

**Dev-dependency, not a dependency.** The generator runs at author time; nothing
about it belongs in a binary that redraws every four seconds. The derives sit
behind a `schema` feature so the release build does not carry them.

`mise run code:schema` regenerates the file in place, matching the existing
`code/*` task library.

### 2. Carry the descriptions across

Descriptions move into `#[doc]` comments on the types, which `schemars` emits as
`description`. **A generated schema with fewer descriptions than the
hand-written one fails this cycle** — criterion 3.

### 3. Fail the build on schema drift

`code:precommit` and CI regenerate to a temp file and diff against the committed
one. A difference fails, naming `mise run code:schema` as the fix.

Committed rather than generated at build time because editors and the website
fetch it over HTTP from a stable path on `main`.

### 4. Turn on `deny_unknown_fields` — for the validator only

On the closed objects: `powerline`, `spend`, `caps`, `subagent`, the style
objects. **Not** the open maps, where an unknown key is a legal entry.

**The render path keeps the permissive types.** Plan 1 made a deserialize
failure fall back to defaults, so a strict derive on the render path would let
one typo blank a user's entire config — strictly worse than today's silence and
a regression against the never-fail rule. The strict shape is a second
deserialization used only by the validator. One set of types, two derives: the
shapes cannot drift because there is only one shape.

### 5. Report per-layer findings in `--debug`

Findings attach to the layer that caused them, not to the merged result:

```text
CONFIG LAYERS (low to high)
  embedded loaded          <built in>
  user     loaded          ~/.config/claude-status/config.json
           ⚠ unknown key `powerlin` (did you mean `powerline`?)
           · symbols.contxt is not a key this binary reads
           · gauge.width 0 → 10
  repo     using defaults  <no git root>
```

Three kinds:

- **⚠ unknown key** in a closed object — a typo, reportable with confidence.
- **· not a key this binary reads** — a key in an open map outside the consumed
  set. A note, never a warning: it is legal.
- **· coerced** — an empty `gauge.filled`, a `0` width, an uncompilable
  `worktreePattern`. **The most useful of the three, and the one no schema can
  give you**: it reports what the binary *did* with the value, not what the
  value is allowed to be.

### 6. Leave the URL on `main`, and list on SchemaStore

**Not pinned to a tag.** Once generated from the types, the schema tracks the
code by construction. Pinning would freeze an editor on the schema of whatever
version happened to seed the file — worse for the common case, where the user
upgrades the binary and keeps their config.

Submit to SchemaStore's catalog matching `claude-status.json`. `--configure`
keeps seeding the key regardless: a config that carries its own pointer works in
editors that consult no catalog.

**SchemaStore is a PR into a queue outside this repo.** It is the last step and
deliberately **not** an acceptance criterion.

### 7. Docs

§3 records that the schema is generated, that validation is advisory, and that
unknown-key detection covers closed objects only. §5's `--debug` description
gains the validation section. `CONTRIBUTING.md` gains `mise run code:schema`.

## Acceptance criteria (from contract)

1. Given a clean checkout, when `mise run code:schema` runs, then the committed
   schema is unchanged.
2. Given a type with a field added and the schema not regenerated, when
   `code:precommit` runs, then it fails and names the fix.
3. Given the generated schema, when its `description` strings are compared with
   the hand-written schema at this cycle's parent commit, then none is lost.
4. Given a user layer with `powerlin` instead of `powerline`, when `--debug`
   runs, then it is reported **under the user layer** — and when the bar
   renders, then it renders exactly as it did before the typo was introduced.
5. Given `gauge.width` of `0`, when `--debug` runs, then it reports the coercion
   to 10.
6. Given an unknown key in `palette`, when `--debug` runs, then it is **not**
   reported as an error.
7. Given any config at all, when the bar renders, then `--debug`'s findings
   change nothing on stdout and the exit code is 0.
8. Given the release binary, when its dependency tree is inspected, then
   `schemars` is absent.
9. Given the committed schema, when it is fetched and parsed as draft-2020-12 by
   a third-party validator, then it is valid — the website consumes this file,
   so being well-formed is now an interface obligation.

## Risks / drift

**The strict/permissive split is the subtle part.** Getting step 4 wrong in the
direction of strictness means a typo blanks a config — worse than today.
Criterion 4 tests exactly that: reported *and* harmless.

**A generated schema can be worse than a hand-written one.** The existing file
carries prose written for a human editing JSON. `schemars` emits structure
faithfully and prose only if it is in `#[doc]`. Criterion 3 should be checked by
diffing descriptions, not by eyeballing the file.

**The schema is now load-bearing for the website.** A malformed or
under-described schema produces a form with unlabelled fields. Criterion 9 makes
that a gate rather than something discovered later in
[website/02](../2026-08-23-website/02-config-generator.md).

**SchemaStore is not this repo's to merge.** It can sit, be rejected, or ask for
changes. If it has not merged when everything else has, record it in Gaps and
finish; do not hold a cycle open on someone else's queue.

**Leaving the URL on `main` is a real trade, taken deliberately.** A user on an
old binary gets an editor validating against a newer schema, which can offer a
key their binary ignores. Pinning gets the reverse and worse case: an editor
rejecting a key their upgraded binary supports, forever, because the URL was
frozen at install time. Generation makes the first case rare and
self-correcting; nothing makes the second self-correct.

**The open-map limit undercuts the original ask.** The goal was that a typo'd
key be distinguishable from a key that does nothing. For `symbols`, `segments`,
`palette` and `typeSymbols` — a large share of what people actually edit — it
still is not, and cannot be. Step 5's note is the mitigation, and the website
should say so plainly rather than leave a user to discover it.

## Out of scope for this cycle

- **Making validation fail a render, or exit non-zero.** Both answered *no*.
  §3's never-fail rule is not up for renegotiation in a diagnostics cycle, and a
  non-zero `--debug` would break the pipelines people run it in.
- **Typo detection in open maps.** Not possible; see Risks.
- **Pinning the schema URL to a tag.** Decided against; see step 6.
- **The website's form.**
  [website/02](../2026-08-23-website/02-config-generator.md) consumes this
  schema but is planned separately.

## Gaps surfaced during execution

*(filled in during execution)*
