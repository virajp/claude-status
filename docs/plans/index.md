---
type: vwf-plan-index
title: Cycle plans — claude-status
description: One row per cycle plan, its target repo, what it requires, what has
  landed, and its status.
status: stable
---

# Cycle plans

The Rust rewrite of the Claude Code statusline. Each plan is a diff against
[the behaviour contract](../spec/statusline-behaviour.md), and each ends at
something runnable.

## Active

| Plan                                                                      | Target repo     | Requires     | Landed | Status     |
| ------------------------------------------------------------------------- | --------------- | ------------ | ------ | ---------- |
| [2026-08-21-2121-macos-only](./2026-08-21-2121-macos-only.md)             | `claude-status` | —            | —      | active     |
| [2026-08-21-2122-release](./2026-08-21-2122-release.md)                   | `claude-status` | `macos-only` | —      | superseded |
| [2026-08-22-2147-github-artifacts](./2026-08-22-2147-github-artifacts.md) | `claude-status` | —            | —      | active     |

**Execution order** — `macos-only`, then `release`. They are separated because
`release` is the only cycle here whose steps are **not** reversible by editing a
file: a published version cannot be republished, and a wrapper published while
the target table is still six-wide would reserve four names that will never be
built again.

Accepted but not yet cut into a cycle: [`backlog.md`](./backlog.md).

## Archived

The five cycles that built the binary. All executed and merged; retired
2026-08-21 into [`archived/`](./archived/).

| Plan                                                                           | Target repo     | Requires                         | Landed                        | Status   |
| ------------------------------------------------------------------------------ | --------------- | -------------------------------- | ----------------------------- | -------- |
| [2026-08-19-1400-main-bar](./archived/2026-08-19-1400-main-bar.md)             | `claude-status` | —                                | executed, merged in `7cb6247` | archived |
| [2026-08-19-1401-subagent-panel](./archived/2026-08-19-1401-subagent-panel.md) | `claude-status` | main-bar                         | executed, merged in `8728158` | archived |
| [2026-08-19-1402-spend](./archived/2026-08-19-1402-spend.md)                   | `claude-status` | main-bar                         | executed, merged in `d68baf6` | archived |
| [2026-08-19-1404-caps-hook](./archived/2026-08-19-1404-caps-hook.md)           | `claude-status` | main-bar                         | executed, merged in `61a5445` | archived |
| [2026-08-19-1403-distribution](./archived/2026-08-19-1403-distribution.md)     | `claude-status` | subagent-panel, spend, caps-hook | executed, merged in `33d6abc` | archived |

**`distribution` was archived with two acceptance criteria open**, deliberately.
Everything it owned has landed except the publish itself, and both open criteria
— one binary per platform from a real publish, and a machine with no Node and no
Rust running it — are **carried forward verbatim** into
[`release`](./2026-08-21-2122-release.md), which is where they can actually be
closed. Nothing was retired unfinished and unrecorded.

Filename timestamps record when each plan was written, not when it ran:
`caps-hook` was added after `distribution` and preceded it, and `distribution`
ran partly ahead of its `requires:` on request. Its own
[gaps section](./archived/2026-08-19-1403-distribution.md#gaps-surfaced-during-execution)
records that history.

## Nothing has shipped

The repo carries no release tag and no package is on the registry. The first
release also cannot go through CI —
[npm requires a package to exist before a Trusted Publisher can be configured](https://github.com/npm/cli/issues/8544),
so the names need one manual publish first.
[`release`](./2026-08-21-2122-release.md) owns that.

**Three names, not seven.** `macos-only` narrows the supported set from six
targets to two, so a complete release is the wrapper plus two darwin platform
packages.

## Not planned here

**Phase 5 — the `ai-plugins` cutover.** Removing `tools/statusline/` — the JS
bar and `context-caps.js` both live there — from `virajp/ai-plugins`, retiring
the `cli/src/statusline.ts` install path that deploys them to
`~/.claude/scripts/statusline` and `~/.claude/hooks/context-caps.js`, and
pointing that repo's docs at this one. A change to a different repository, gated
on [`release`](./2026-08-21-2122-release.md) having shipped.

It also owns the two simplifications this repo cannot make until it lands:
deleting the transitional `.config/statusline.json`, and retiring
`$AI_PLUGINS_USAGE_DIR` together with §8's frozen mirror field names — once the
JS hook is gone, this binary is both the writer and the only reader of that
file.

> **The contract has caught up.**
> [`caps-hook`](./archived/2026-08-19-1404-caps-hook.md) executed, and with it
> §13's `context-caps.js` bullet is struck through and marked reversed, and
> §10's Phase 5 now has the installer replace the `node …context-caps.js`
> command with `claude-status --caps-hook` rather than leaving a Node hook
> behind.
