---
type: vwf-plan
title: config-relocation — 2026-08-23
description: Cycle plan (a diff) moving the user config into
  ~/.config/claude-status/, storing only non-defaults, narrowing the repo layer
  to projectName, and deleting the render path's ability to write files.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: [
  docs/plans/2026-08-23-config-and-cli/01-typed-config.md,
]
timestamp: 2026-08-23T14:02:00Z
tags: [ config, xdg, paths, autoseed, defaults ]
---

# Plan: config-relocation — 2026-08-23

## Slice

Contract §2 and §3. Four changes to where config lives and what it contains:

1. The user config moves to `~/.config/claude-status/config.json`.
2. A written config contains **only non-defaults**.
3. The repo layer is narrowed to `projectName` alone, and is never generated.
4. The render path loses its ability to write files at all.

## Current state (actual)

**The user config is a bare file**, `~/.config/claude-status.json`
(`layers.rs:20`, `CONFIG_FILE_NAME`). The comment beside it notes it is *not*
the JS bar's `statusline.json`, and that `--install` migrates the old file.

**Home is resolved from `$HOME` and nothing else** (`paths.rs:15`), deliberately
not a platform config directory, because `dirs::config_dir()` on macOS gives
`~/Library/Application Support`.

**The spend cache is already correctly placed** at
`~/.cache/claude-status/spend.json` (`spend/cache.rs:94`), with a
`CLAUDE_STATUS_SPEND_CACHE` override used by the tests.

**The installer seeds the defaults byte-for-byte.**
`installer/src/modules/config.ts` writes `assets/claude-status.defaults.json`
verbatim as the user's config. So every install freezes a full copy of the
shipped defaults.

**A render creates the repo config.** `src/modules/config/autoseed.rs` writes
`<repo-root>/.config/claude-status.json` from the render path, gated on
`Config::auto_configure_repo` — an opt-**out** key whose shipped value is
`true`. It seeds `$schema` and `projectName`, and it also *migrates* a
`statusline.json` it finds, rewriting the `$schema` URL.

**The repo layer is a full config layer.** It participates in the same deep
merge as the user layer and can override anything.

**There is nothing to migrate.** `claude-status` has never been released. No
user has a config at any of these paths.

## Target state (per contract)

| Thing              | Path                                     |
| ------------------ | ---------------------------------------- |
| User config        | `~/.config/claude-status/config.json`    |
| Repo config        | `<repo-root>/.config/claude-status.json` |
| Spend cache & lock | `~/.cache/claude-status/`                |

The user config holds only keys whose value differs from the binary's default,
so an unset key follows the binary forward. The repo config holds `projectName`
and nothing else, is written by a human, and is documented in `--help` and on
the website.

**A render reads. It never writes.** With no config file anywhere, the bar
renders from the embedded defaults, and that is a supported, tested state rather
than a degraded one.

## Delta — ordered steps

### 1. Move the user config into a directory

`~/.config/claude-status/config.json`. A directory rather than a bare file
because the tool will accumulate more than one thing to store, and a directory
is also one thing to delete.

**No fallback to the old path.** Nothing was ever released, so a fallback would
be compatibility with a state that never existed.

### 2. Keep the cache where it is, and say why

`~/.cache/claude-status/` is already correct and does not move. Record the
reason in §3 so it is not "tidied" into the config directory later: a spend
figure derived from an account token is machine-local and regenerable, and a
config directory is the thing people commit to a dotfiles repo and sync between
machines.

### 3. Teach the config to serialise only its non-defaults

A `Config` can already be compared with `Config::default()` after
[plan 1](./01-typed-config.md). Serialisation walks the tree and emits a key
only where the two differ.

**`#[serde(skip_serializing_if)]` is not sufficient on its own** for the open
maps: `palette` and `segments` are `BTreeMap`s whose *defaults are non-empty*,
so "skip if empty" would emit the whole shipped palette the moment a user
changes one colour. A map is diffed entry by entry.

The output always carries `$schema`, which is not a default — it is a pointer
that makes the file editable.

### 4. Narrow the repo layer to `projectName`

The repo layer deserializes into a one-field type. Any other key present is
**ignored, and reported by `--debug`** — not merged, and not an error, because
§3's never-fail rule still holds.

This is a deliberate narrowing of §2's three-layer merge. The repo layer existed
to name the project; letting it override styling made every repo a place where
the bar could look different for reasons nobody could find.

### 5. Delete `autoseed.rs` and `autoConfigureRepo`

Both go. The module, the config key, the schema entry, and the tests.

**The invariant this buys is worth naming**: a status line that redraws every
four seconds now provably touches nothing on disk during a render. That is
easier to reason about than any amount of care about *when* it writes.

### 6. Delete the `statusline.json` migration

`autoseed.rs`'s migration arm and the installer's. The JS bar's config is
another tool's file, and with nothing released there is no user holding one that
this binary was ever going to read.

### 7. Make zero-config a tested state, not an accident

`layers::load(None, None)` already returns the embedded defaults, and a golden
covers it. Extend that to the real paths: with `$HOME` pointing at an empty
directory and no git root, a full bar renders and nothing is created.

### 8. Docs

§2 records the three-layer merge with the repo layer restricted to
`projectName`. §3 records the new paths, the config/cache split and its reason,
the non-defaults-only rule, and that a render never writes. §9's `--install`
description is left alone —
[distribution/01](../2026-08-23-distribution/01-drop-npm.md) owns it.

## Acceptance criteria (from contract)

1. Given `$HOME` pointing at an empty directory and no git root, when the bar
   renders, then it renders in full and **nothing is created anywhere** under
   that `$HOME`.
2. Given a config differing from the defaults in exactly one key, when it is
   serialised, then the output holds `$schema` and that one key.
3. Given a config that changes one entry of `palette`, when it is serialised,
   then only that entry is emitted, not the whole shipped palette.
4. Given a user config at `~/.config/claude-status/config.json`, when the bar
   renders, then it is applied; given one at the old
   `~/.config/claude-status.json`, then it is **ignored**.
5. Given a repo config carrying `projectName` and `gauge`, when the bar renders,
   then the name applies and the gauge does not, and `--debug` reports the
   ignored key.
6. Given any render at all, when the process is traced, then it opens no file
   for writing outside `~/.cache/claude-status/`.
7. Given the repo after this cycle, when it is searched, then `autoseed.rs`,
   `autoConfigureRepo` and every `statusline.json` reference are gone.
8. Given `tests/golden/`, when the suite runs, then every golden matches without
   regeneration.

## Risks / drift

**Non-defaults-only serialisation is only as good as plan 1's `Default`.** If
`Config::default()` disagrees with `DEFAULTS_JSON` anywhere, this cycle writes a
file that either omits a key the user set or emits one they did not. Plan 1's
criterion 2 is the guard; if it was weakened to pass, this cycle inherits the
weakness and criterion 2 here will not catch it.

**The open-map diff is the subtle part.** Emitting a whole `palette` because one
colour changed would defeat the purpose entirely and would look like it worked —
the file is valid, the bar renders, and the user has silently frozen every other
colour. Criterion 3 exists for exactly that, and it is worth extending to
`segments` and `symbols` by hand.

**Narrowing the repo layer removes a capability.** Anyone using a repo layer for
styling loses it. There is nobody in that position — nothing has shipped — but
the *contract* said the layer could override anything, so §2 is being reduced,
not clarified. It is recorded as a reversal rather than a tidy-up.

**Deleting autoseed removes the only thing that created the repo layer.** After
this cycle a repo config exists only if a human writes one, which means
discoverability moves entirely to `--help` ([plan 3](./03-cli-surface.md)) and
the website. If both are vague, the feature is effectively gone — the file being
supported is not the same as anyone knowing it exists.

**Criterion 6 is worth actually running, not reasoning about.** "A render writes
nothing" is easy to believe and easy to be wrong about — a cache touch, a lock
file, a log. Trace it.

## Out of scope for this cycle

- **`--configure`, `--refresh`, `--help` and `--debug`'s no-config report.**
  [Plan 3](./03-cli-surface.md). This cycle creates the zero-config state; plan
  3 makes the CLI speak about it.
- **The schema.** [Plan 4](./04-schema-and-validation.md) regenerates it; this
  cycle edits it by hand only where a key was deleted.
- **Deleting the installer.**
  [distribution/01](../2026-08-23-distribution/01-drop-npm.md).
- **The usage mirror and `$AI_PLUGINS_USAGE_DIR`.** §8 is a contract with
  another repo and is explicitly unchanged.

## Gaps surfaced during execution

*(filled in during execution)*
