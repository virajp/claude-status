---
type: vwf-plan
title: typed-config — 2026-08-23
description: Cycle plan (a diff) replacing the untyped Value-and-dotted-paths
  config with deserialized Rust types, preserving every forgiving coercion and
  changing no rendered byte.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: []
timestamp: 2026-08-23T10:01:00Z
tags: [ config, serde, refactor, render ]
---

# Plan: typed-config — 2026-08-23

## Slice

Contract §3 (Configuration). The merged config becomes a set of deserialized
Rust types instead of a `serde_json::Value` read through dotted string paths.

**This cycle changes no behaviour.** Every rendered byte, every fallback, every
stderr warning is identical before and after. It exists so that
[plan 2](./02-schema-and-validation.md) has something to generate a schema from
and validate against.

## Current state (actual)

`src/modules/config/mod.rs:27`:

```rust
pub struct Config {
    root: Value,
}
```

Everything else in the module is an accessor over that one field. `get` splits a
dotted path at runtime and folds it through the tree:

```rust
pub fn get(&self, path: &str) -> Option<&Value> {
    path.split('.').try_fold(&self.root, |cur, key| cur.get(key))
}
```

**Eleven accessors, and each one encodes a deliberate coercion.** These are not
incidental — most were written against a specific bug, and the comments in
`mod.rs` say which:

| Accessor              | Coercion that must survive typing                                          |
| --------------------- | -------------------------------------------------------------------------- |
| `symbol`              | missing key → `""`. A guard, not a behaviour; the embedded layer covers it |
| `powerline`           | missing → `""`                                                             |
| `gauge_width`         | `0` **or** missing → `10`; capped at `MAX_GAUGE_WIDTH` (1000)              |
| `gauge_glyph`         | missing **or empty** → `▰`/`▱`. An empty glyph would erase the bar         |
| `project_name`        | empty string → `None`, not `Some("")`                                      |
| `auto_configure_repo` | non-boolean or missing → `true`. Opt-**out**, must match the defaults      |
| `worktree_matcher`    | empty **or** uncompilable pattern → `"worktree"`, warning on **stderr**    |
| `lines`               | a non-array row degrades to an empty row, not an error                     |
| `palette`             | a non-object → `None`                                                      |
| `default_fg`          | raw `Option<&Value>`, resolved later by `color::resolve`                   |
| `color`               | delegates to `color::resolve` against the palette                          |

`gauge_width`'s cap is load-bearing beyond tidiness: the width feeds
`str::repeat`, and an enormous one aborts on allocation failure. An abort cannot
be caught by `catch_unwind`, so the fallback line would never print and the bar
would go blank.

**The merge is untyped and must stay that way.** `layers::load`
(`src/modules/config/layers.rs:50`) deep-merges embedded → user → repo, and
`deep_merge` (`src/_shared/json.rs:87`) strips `FORBIDDEN_KEYS` as it goes. A
layer that parses but is not an object is skipped entirely, because replacing
the merged config with a number would blank the bar.

**The schema is hand-written and unrelated to the code.**
`schemas/claude-status.schema.json`, 301 lines, `additionalProperties: false` at
the top level. Nothing checks it against the accessors.

**Four config keys are open maps, not fixed shapes.** `palette`, `symbols`,
`typeSymbols` and `segments` all declare `additionalProperties: <type>` — their
keys are user-chosen colour names, data types and segment ids. `powerline`,
`spend`, `caps` and `subagent` are closed (`additionalProperties: false`).

## Target state (per contract)

The merged `Value` is deserialized **once** into a `Config` whose fields are
typed. Readers keep the same method names and signatures wherever possible, so
call sites move rather than change.

Contract §3's forgiveness rule is unchanged and now has a stronger guarantee
behind it: a layer that is missing, malformed, or not a JSON object is ignored
rather than fatal, and **a merged config that will not deserialize is ignored
too** — the embedded defaults render instead.

## Delta — ordered steps

### 1. Define the types, mirroring the schema's open/closed split

Closed objects become structs with `#[serde(deny_unknown_fields)]` deferred to
plan 2 — this cycle derives `Deserialize` only, so an unknown key is still
silently ignored and behaviour does not move.

Open maps become `BTreeMap<String, T>`: `palette`, `symbols`, `typeSymbols`,
`segments`. `BTreeMap` and not `HashMap` so `--debug`'s output is ordered and
diffable.

`lines` is `Vec<Vec<SegmentEntry>>`, where `SegmentEntry` is an untagged enum of
a bare id string or a style-override object — the one place the schema is
genuinely heterogeneous.

### 2. Put `#[serde(default)]` on everything, without exception

Every field is optional at every layer, because a user layer is a partial
override. A missing field must produce the type's default, never an error. This
is the single largest source of regression risk in the cycle and is worth a test
that deserializes `{}` and asserts the result equals the embedded defaults.

### 3. Move each coercion from its accessor into the type

The table above is the checklist. Three shapes:

- **Plain defaults** (`symbol`, `powerline`, `auto_configure_repo`) — a
  `#[serde(default = "...")]` function.
- **Value-dependent coercions** (`gauge_width`'s `0 → 10` and its cap,
  `gauge_glyph`'s empty → fallback, `project_name`'s empty → `None`) — a custom
  `deserialize_with`, because the coercion is on the *parsed* value, not on
  absence.
- **Fallible with a side effect** (`worktree_matcher`) — stays an accessor. It
  compiles a regex and writes to stderr on failure; that belongs in a method,
  not in a `Deserialize` impl. The typed field holds the raw pattern string.

### 4. Deserialize after the merge, and never before

`layers::load` keeps returning the merged `Value` internally, then calls
`serde_json::from_value` once at the end. The merge, the object-only filter and
the `FORBIDDEN_KEYS` strip all happen first and all stay untyped.

### 5. Make a deserialize failure fall back, not fail

If `from_value` errors on the merged tree, fall back to deserializing the
embedded defaults alone and emit one stderr diagnostic naming the error.

**The embedded defaults must be infallible.** Deserializing them is the fallback
path, so if *they* fail there is nothing left. A test asserts `DEFAULTS_JSON`
deserializes cleanly; that test is what makes the fallback safe to rely on.

### 6. Replace the accessors and delete `get`

Call sites move from `config.get("gauge.width")` to `config.gauge.width`.
`Config::get` and its dotted-path fold are deleted — leaving them would keep a
second, unchecked way to read the config alive.

`Config::color` and `Config::worktree_matcher` survive as methods; they compute
rather than fetch.

### 7. Docs

Contract §3 gains a note that the config is deserialized into types after the
merge, and that a merged config which will not deserialize falls back to the
embedded layer. The forgiveness rule itself is unchanged — this names a case it
already covered in spirit.

## Acceptance criteria (from contract)

1. Given every reference fixture in `docs/spec` §12, when the bar is rendered
   before and after this cycle, then the output is **byte-identical**.
2. Given `{}` as the merged config, when it is deserialized, then the result
   equals the embedded defaults.
3. Given `DEFAULTS_JSON`, when it is deserialized, then it succeeds — the
   fallback path is proven, not assumed.
4. Given a merged config that will not deserialize, when the bar renders, then
   the embedded defaults render, one diagnostic is written to **stderr**, and
   stdout carries a full bar.
5. Given `gauge.width` of `0`, then the width is 10; given `100000`, then it is
   `MAX_GAUGE_WIDTH`.
6. Given an empty `gauge.filled`, an empty `projectName` and an empty
   `worktreePattern`, then each falls back exactly as it does today — the
   empty-string cases are separately tested from the missing-key cases.
7. Given an uncompilable `worktreePattern`, then the default matcher is used and
   the warning goes to stderr, with stdout unchanged.
8. Given the repo after this cycle, when `grep -rn 'config.get("' src/` runs,
   then there are no matches.

## Risks / drift

**The forgiving semantics are the whole risk.** `serde`'s defaults are about
*absence*; most of these coercions are about a *present but useless* value — an
empty glyph, a zero width, an empty pattern. A `#[serde(default)]` does not fire
for those. Step 3 splits them deliberately for that reason, and criterion 6
tests the empty case separately from the missing case because they take
different paths.

**A hot path is being rewritten for a diagnostic feature.** This binary redraws
every four seconds and the third invariant is that it never fails to render.
Nothing in this cycle is user-visible, so criterion 1 — byte-identical output
across the whole fixture set — is the real gate, not the unit tests.

**Typing may find that the schema and the code already disagree.** They have
never been checked against each other. If a key the accessors read is absent
from the schema, or vice versa, this cycle surfaces it. That is a good outcome
but it is a **contract question**, not a refactor decision: record it in Gaps
and route it rather than silently picking whichever side looks right.

**Open maps cannot be typo-checked, and never will be.** A misspelled key in
`palette`, `symbols`, `typeSymbols` or `segments` is by definition a legal
entry. Plan 2 inherits this limit; it is named here because it is a property of
the config's shape, not of the validator.

## Out of scope for this cycle

- **`deny_unknown_fields`, schema generation and the `--debug` validation
  section.** All of it is [plan 2](./02-schema-and-validation.md). This cycle
  deliberately keeps unknown keys silently ignored so that behaviour does not
  move while the types land.
- **Changing any default, key name or coercion.** A coercion that looks wrong is
  a Gaps entry, not an edit.
- **The `$schema` URL and SchemaStore.** Plan 2.

## Gaps surfaced during execution

*(filled in during execution)*
