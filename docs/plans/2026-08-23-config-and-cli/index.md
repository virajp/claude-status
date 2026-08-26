---
type: vwf-plan-index
title: config-and-cli — 2026-08-23
description: Four ordered plans that give the config real types, move it to
  ~/.config/claude-status/ storing only non-defaults, reshape the CLI around
  --configure, and make the types generate the schema.
status: done
covers: [
  docs/spec/statusline-behaviour.md,
]
timestamp: 2026-08-23T14:00:00Z
tags: [ config, cli, serde, schema, validation, xdg ]
---

# config-and-cli — 2026-08-23

| # | Plan                                                   | What it changes                                                                                        |
| - | ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------ |
| 1 | [typed-config](./01-typed-config.md)                   | the config becomes deserialized Rust types — and a default becomes a thing the code can name           |
| 2 | [config-relocation](./02-config-relocation.md)         | `~/.config/claude-status/`, non-defaults only, repo layer narrowed to `projectName`, zero-config works |
| 3 | [cli-surface](./03-cli-surface.md)                     | `--refresh` rename, `--configure`, `--help`, and a `--debug` that says "no config"                     |
| 4 | [schema-and-validation](./04-schema-and-validation.md) | the types generate the schema; `--debug` validates against them                                        |

## The change these four serve

**The config file should hold only what the user changed.** Today the installer
seeds `~/.config/claude-status.json` with the shipped defaults byte-for-byte, so
every install freezes a copy of them. A default that later turns out to be wrong
cannot be improved for anyone who has already installed — their file overrides
it forever, with a value they never chose.

Storing only non-defaults inverts that: an unset key follows the binary. It also
makes the file small enough to read, which is what the website's generator
produces and what a human can reasonably hand-edit.

**This requires knowing what a default is**, in code, at runtime — which is why
plan 1 comes first and why it is not optional. `Config` is currently a
`serde_json::Value` behind dotted-path getters; nothing in it can answer "is
this value the default?". Typing it is the enabling change for everything else
in this folder.

## Order

**1 → 2 → 3 → 4**, and the dependency is real at every step.

Plan 2 cannot write a non-defaults-only file without plan 1's notion of a
default. Plan 3's `--debug` must report "no config file, using defaults", which
is a state plan 2 creates. Plan 4 generates the schema from plan 1's types, and
the website's config generator
([website/02](../2026-08-23-website/02-config-generator.md)) consumes that
schema — so plan 4 is the one thing outside this folder that another folder
waits on.

## Decisions this folder rests on

**A render never writes to disk.** `src/modules/config/autoseed.rs` currently
creates the repo config layer from the render path, gated on an
`autoConfigureRepo` key. Both are deleted in plan 2. A status line that redraws
every four seconds and also creates files is a surprising thing to have built,
and the reason it existed — nothing else seeded the repo layer — stops applying
when the repo layer becomes a hand-written, documented, one-key file.

**The repo layer holds `projectName` and nothing else.** It is authored by the
user, documented in `--help` and on the website, and never generated. This is a
deliberate narrowing of contract §2's three-layer merge, taken because the repo
layer's only real use is naming the project.

**Config and cache are separate directories.** Config at
`~/.config/claude-status/`, regenerable state at `~/.cache/claude-status/`
(where the spend cache already lives). A cache under `~/.config` gets committed
to dotfile repos and synced between machines, which is exactly wrong for a
machine-local token-derived figure.

**Nothing needs migrating.** `claude-status` has never been released. There are
no users, no installed configs, and no old paths to honour — so every move here
is a rename rather than a migration, and the JS bar's `statusline.json`
migration is deleted rather than carried.
