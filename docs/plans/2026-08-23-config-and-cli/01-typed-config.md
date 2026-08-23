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
timestamp: 2026-08-23T14:01:00Z
tags: [ config, serde, refactor, render ]
---

# Plan: typed-config — 2026-08-23

## Slice

Contract §3 (Configuration). The merged config becomes a set of deserialized
Rust types instead of a `serde_json::Value` read through dotted string paths.

**This cycle changes no behaviour.** Every rendered byte, every fallback, every
stderr warning is identical before and after. It exists because three later
plans need something this codebase cannot currently do: name a default.

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

**Four config keys are open maps, not fixed shapes.** `palette`, `symbols`,
`typeSymbols` and `segments` all declare `additionalProperties: <type>` in the
schema — their keys are user-chosen colour names, data types and segment ids.
`powerline`, `spend`, `caps` and `subagent` are closed
(`additionalProperties: false`).

**Nothing can currently answer "is this the default?"** — there is no
representation of the defaults beyond the embedded JSON blob merged underneath
everything else. That is the gap the next three plans need closed.

## Target state (per contract)

The merged `Value` is deserialized **once** into a `Config` whose fields are
typed. Readers keep the same method names and signatures wherever possible, so
call sites move rather than change.

Every type implements `Default`, and that `Default` is the same value the
embedded JSON carries — proven by a test, not by inspection. This is what
[plan 2](./02-config-relocation.md) needs in order to write a file containing
only what differs from it.

Contract §3's forgiveness rule is unchanged and gains a case: a merged config
that will not deserialize is ignored too, and the embedded defaults render.

## Delta — ordered steps

### 1. Define the types, mirroring the schema's open/closed split

Closed objects become structs. Open maps become `BTreeMap<String, T>`:
`palette`, `symbols`, `typeSymbols`, `segments`. `BTreeMap` rather than
`HashMap` so `--debug`'s output is ordered and diffable.

`lines` is `Vec<Vec<SegmentEntry>>`, where `SegmentEntry` is an untagged enum of
a bare id string or a style-override object — the one place the schema is
genuinely heterogeneous.

**No `deny_unknown_fields` in this cycle.** An unknown key stays silently
ignored, so behaviour does not move while the types land.
[Plan 4](./04-schema-and-validation.md) turns strictness on, and does it in a
way that cannot blank a config.

### 2. Put `#[serde(default)]` on everything, without exception

Every field is optional at every layer, because a user layer is a partial
override. A missing field must produce the type's default, never an error.

### 3. Make `Default` agree with the embedded JSON, and prove it

Each type's `Default` returns what `DEFAULTS_JSON` carries for that key. A test
deserializes `DEFAULTS_JSON` and asserts the result equals `Config::default()` —
**field by field, not by serialising both to strings**, so a mismatch names the
field.

This test is the contract plan 2 depends on. If it can be made to pass only by
weakening it, plan 2's "store only non-defaults" is unsound and that is a Gaps
entry, not a workaround.

### 4. Move each coercion from its accessor into the type

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

### 5. Deserialize after the merge, and never before

`layers::load` keeps merging `Value` internally, then calls
`serde_json::from_value` once at the end. The merge, the object-only filter and
the `FORBIDDEN_KEYS` strip all happen first and all stay untyped.

### 6. Make a deserialize failure fall back, not fail

If `from_value` errors on the merged tree, use `Config::default()` and emit one
stderr diagnostic naming the error. Step 3 is what makes this safe: the fallback
is a value in code, not a second parse that could also fail.

### 7. Replace the accessors and delete `get`

Call sites move from `config.get("gauge.width")` to `config.gauge.width`.
`Config::get` and its dotted-path fold are deleted — leaving them would keep a
second, unchecked way to read the config alive.

`Config::color` and `Config::worktree_matcher` survive as methods; they compute
rather than fetch.

### 8. Docs

Contract §3 gains a note that the config is deserialized into types after the
merge, and that a merged config which will not deserialize falls back to the
embedded defaults.

## Acceptance criteria (from contract)

1. Given `tests/golden/`, when `cargo test --test golden` runs, then every
   golden matches **without** being regenerated — this is the real gate, and
   `UPDATE_GOLDEN=1` must not be used in this cycle.
2. Given `DEFAULTS_JSON`, when it is deserialized, then it equals
   `Config::default()` field by field.
3. Given `{}` as the merged config, when it is deserialized, then the result
   equals `Config::default()`.
4. Given a merged config that will not deserialize, when the bar renders, then
   the defaults render, one diagnostic goes to **stderr**, and stdout carries a
   full bar.
5. Given `gauge.width` of `0`, then the width is 10; given `100000`, then it is
   `MAX_GAUGE_WIDTH`.
6. Given an empty `gauge.filled`, an empty `projectName` and an empty
   `worktreePattern`, then each falls back exactly as it does today — the
   empty-string cases tested separately from the missing-key cases.
7. Given an uncompilable `worktreePattern`, then the default matcher is used and
   the warning goes to stderr, with stdout unchanged.
8. Given the repo after this cycle, when `grep -rn 'config.get("' src/` runs,
   then there are no matches.

## Risks / drift

**The forgiving semantics are the whole risk.** `serde`'s defaults are about
*absence*; most of these coercions are about a *present but useless* value — an
empty glyph, a zero width, an empty pattern. `#[serde(default)]` does not fire
for those. Step 4 splits them deliberately, and criterion 6 tests the empty case
separately from the missing case because they take different paths.

**Criterion 1 is the one that matters.** The goldens are exact ANSI strings
containing Nerd Font private-use codepoints; `tests/golden.rs` says outright
that they are generated and never hand-written. Regenerating them during this
cycle would convert a rendering regression into a passing test. If a golden
genuinely must change, that is a Gaps entry with the reason.

**Criterion 2 may not hold on the first attempt, and that is informative.** The
embedded defaults and the accessors' fallbacks have never been checked against
each other. Where they disagree, the *embedded JSON* wins — it is what renders
today — and the disagreement is recorded rather than quietly resolved in favour
of whichever was easier to write.

**Typing may find the schema and the code already disagree.** They have never
been compared. A key the accessors read but the schema omits, or vice versa, is
a **contract question**: record it and route it, do not pick a side.

**Open maps cannot be typo-checked, and never will be.** A misspelled key in
`palette`, `symbols`, `typeSymbols` or `segments` is by definition a legal
entry. Plan 4 inherits this limit; it is named here because it is a property of
the config's shape, not of the validator.

## Out of scope for this cycle

- **Anything a user can see.** Criterion 1 is the bar. If this cycle changes
  output, it has failed regardless of how good the change is.
- **`deny_unknown_fields`, schema generation, the `--debug` validation
  section.** [Plan 4](./04-schema-and-validation.md).
- **Moving files, changing paths, or storing only non-defaults.**
  [Plan 2](./02-config-relocation.md).
- **Deleting `autoseed.rs` or `autoConfigureRepo`.** Also plan 2 — they are
  typed here like everything else, then removed there.
- **Changing any default, key name or coercion.** A coercion that looks wrong is
  a Gaps entry, not an edit.

## Gaps surfaced during execution

*(filled in during execution)*
