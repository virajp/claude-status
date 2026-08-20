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

| Plan                                                                  | Target repo     | Requires                         | Landed                                  | Status |
| --------------------------------------------------------------------- | --------------- | -------------------------------- | --------------------------------------- | ------ |
| [2026-08-19-1400-main-bar](./2026-08-19-1400-main-bar.md)             | `claude-status` | —                                | executed, merged in `7cb6247`           | active |
| [2026-08-19-1401-subagent-panel](./2026-08-19-1401-subagent-panel.md) | `claude-status` | main-bar                         | —                                       | active |
| [2026-08-19-1402-spend](./2026-08-19-1402-spend.md)                   | `claude-status` | main-bar                         | executed, merged in `d68baf6`           | active |
| [2026-08-19-1404-caps-hook](./2026-08-19-1404-caps-hook.md)           | `claude-status` | main-bar                         | —                                       | active |
| [2026-08-19-1403-distribution](./2026-08-19-1403-distribution.md)     | `claude-status` | subagent-panel, spend, caps-hook | steps 1, 3, 4 and 9 only — see its plan | active |

**`Landed` records execution; `Status` records archival.** A row stays `active`
until `/vwf:archive` retires it, and neither executed cycle can be archived yet
— three active plans still name `main-bar` in their `requires:`, and
`distribution` names `spend`.

**Execution order** — `main-bar`, then `subagent-panel` / `spend` / `caps-hook`
in any order or concurrently in separate worktrees, then `distribution` last.
Filename timestamps record when each plan was written, not when it runs:
`caps-hook` was added after `distribution` and precedes it.

**One deliberate exception to that order.** `distribution`'s steps 1, 3, 4 and 9
— the build tasks, the platform packages, the npx wrapper and the CI release
workflow — were executed ahead of its `requires:`, on request, because none of
them depends on the panel, the spend subsystem or the hook. Its remaining steps
(2 and 5–8 — the installer, its receipts, `--uninstall` and the two migrations —
plus step 10's readme) stay gated on all three predecessors. Nothing has shipped
from it: the repo carries no release tag.

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

> **The contract has not caught up with that last paragraph.** §13 still says
> `context-caps.js` "stays in `ai-plugins`", and §10's Phase 5 still has it
> reading the usage mirror after the cutover.
> [`caps-hook`](./2026-08-19-1404-caps-hook.md) reverses §13 and amends it when
> that cycle executes.
