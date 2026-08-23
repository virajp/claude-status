---
type: vwf-plan-index
title: config-ergonomics — 2026-08-23
description: Two ordered plans that give the config real Rust types, then make
  those types the single source for the JSON schema and for a validation
  section in --debug.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
timestamp: 2026-08-23T10:00:00Z
tags: [ config, schema, validation, serde, debug ]
---

# config-ergonomics — 2026-08-23

| # | Plan                                                   | What it changes                                                         |
| - | ------------------------------------------------------ | ----------------------------------------------------------------------- |
| 1 | [typed-config](./01-typed-config.md)                   | the config becomes deserialized Rust types, not `Value` + string paths  |
| 2 | [schema-and-validation](./02-schema-and-validation.md) | those types generate the schema and back a `--debug` validation section |

Both graduate the **Config ergonomics** entries in
[`backlog.md`](../backlog.md), which asked for two things — a `$schema` URL that
is not a moving target, and a `--debug` that says what is wrong with your
config.

## Why this is two plans and not one

The backlog judged these "one slice rather than two", and that judgement was
made against an assumption that turned out to be false: that there were Rust
config types to hang a schema and a validator off.

**There are not.** `Config` is a struct with one field — `root: Value` — and
every reader goes through `config.get("gauge.width")`, a dotted path split at
runtime (`src/modules/config/mod.rs:27`). The 301-line schema at
`schemas/claude-status.schema.json` is hand-written and has no counterpart in
the code at all. So "generate the schema from the types" needs types first, and
introducing them is a change to the render path rather than a documentation
change.

Plan 1 is therefore a refactor with **no user-visible behaviour change** — that
is its whole acceptance bar. Plan 2 is the feature. Landing them together would
mean a cycle where a behavioural regression and a new warning surface are
indistinguishable in review.

## The decision this rests on

**The types become the source of truth, not a mirror of it.** The alternative
considered and rejected was a validation-only shadow type: structs that exist
solely for `schemars` and a `deny_unknown_fields` check, with the renderer left
on `Value`. That kills schema-vs-types drift but invents a new drift — the
shadow can disagree with the getters that actually read the config, and nothing
would catch it. One set of types that reads, validates and generates has no
second thing to disagree with.

The cost is honest: plan 1 touches a hot path that redraws every four seconds,
and it has to preserve a set of deliberately forgiving coercions that a naive
`serde::Deserialize` would turn into hard errors. Those coercions are enumerated
in plan 1 and each one has a test.

## What does not change

**The deep merge stays on `Value`.** Typing happens once, after `layers::load`
has merged embedded → user → repo. It cannot happen before: a user layer setting
only `gauge.width` must not replace the whole embedded `gauge` object, and
`deep_merge` also strips `FORBIDDEN_KEYS` on the way through. Both are
properties of merging untyped trees.

**The bar never fails to render.** A config that will not deserialize falls back
to the embedded defaults and says so on stderr. It does not abort, and it does
not render blank — the third invariant outranks every diagnostic in these two
plans.
