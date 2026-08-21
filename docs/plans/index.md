---
type: vwf-plan-index
title: Cycle plans — claude-status
description: One row per cycle plan, its target repo, what it requires, what has
  landed, and its status.
status: stable
---

# Cycle plans

The Rust rewrite of the Claude Code statusline, decomposed into five chained
cycles. Each plan is a diff against
[the behaviour contract](../spec/statusline-behaviour.md), and each ends at
something runnable.

| Plan                                                                  | Target repo     | Requires                         | Landed                        | Status |
| --------------------------------------------------------------------- | --------------- | -------------------------------- | ----------------------------- | ------ |
| [2026-08-19-1400-main-bar](./2026-08-19-1400-main-bar.md)             | `claude-status` | —                                | executed, merged in `7cb6247` | active |
| [2026-08-19-1401-subagent-panel](./2026-08-19-1401-subagent-panel.md) | `claude-status` | main-bar                         | executed, merged in `8728158` | active |
| [2026-08-19-1402-spend](./2026-08-19-1402-spend.md)                   | `claude-status` | main-bar                         | executed, merged in `d68baf6` | active |
| [2026-08-19-1404-caps-hook](./2026-08-19-1404-caps-hook.md)           | `claude-status` | main-bar                         | executed, merged in `61a5445` | active |
| [2026-08-19-1403-distribution](./2026-08-19-1403-distribution.md)     | `claude-status` | subagent-panel, spend, caps-hook | executed, merged in `33d6abc` | active |

**`Landed` records execution; `Status` records archival.** A row stays `active`
until `/vwf:archive` retires it, and neither executed cycle can be archived yet
— three active plans still name `main-bar` in their `requires:`, and
`distribution` names `spend`.

**Execution order** — `main-bar`, then `subagent-panel` / `spend` / `caps-hook`
in any order or concurrently in separate worktrees, then `distribution` last.
Filename timestamps record when each plan was written, not when it runs:
`caps-hook` was added after `distribution` and precedes it.

**`distribution` ran out of order, deliberately.** Steps 1, 3, 4 and 9 — the
build tasks, the platform packages, the npx wrapper and the CI release workflow
— landed ahead of its `requires:`, on request, because none of them depends on
the panel, the spend subsystem or the hook. Most of 5–8 followed. The rest (step
2's caps-hook key, `--dry-run`/`--yes`/`--force`, the orphan sweep, the readme)
landed once `caps-hook` did, in order.

**Nothing has shipped.** The repo carries no release tag, and two of
`distribution`'s acceptance criteria stay open until it does: one binary per
platform from a real publish, and a machine with no Node and no Rust running it.
The first release also cannot go through CI —
[npm requires a package to exist before a Trusted Publisher can be configured](https://github.com/npm/cli/issues/8544),
so all seven names need one manual publish first.

## Not planned here

**Phase 5 — the `ai-plugins` cutover.** Removing `tools/statusline/` — the JS
bar and `context-caps.js` both live there — from `virajp/ai-plugins`, retiring
the `cli/src/statusline.ts` install path that deploys them to
`~/.claude/scripts/statusline` and `~/.claude/hooks/context-caps.js`, and
pointing that repo's docs at this one. A change to a different repository, gated
on `2026-08-19-1403-distribution` having shipped.

It also owns the two simplifications this repo cannot make until it lands:
deleting the transitional `.config/statusline.json`, and retiring
`$AI_PLUGINS_USAGE_DIR` together with §8's frozen mirror field names — once the
JS hook is gone, this binary is both the writer and the only reader of that
file.

> **The contract has caught up.** [`caps-hook`](./2026-08-19-1404-caps-hook.md)
> executed, and with it §13's `context-caps.js` bullet is struck through and
> marked reversed, and §10's Phase 5 now has the installer replace the
> `node …context-caps.js` command with `claude-status --caps-hook` rather than
> leaving a Node hook behind.
