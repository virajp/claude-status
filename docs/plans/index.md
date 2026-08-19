---
type: vwf-plan-index
title: Cycle plans — claude-status
description: One row per cycle plan, its target repo, and its status.
status: stable
---

# Cycle plans

The Rust rewrite of the Claude Code statusline, decomposed into five chained
cycles. Each plan is a diff against
[the behaviour contract](../spec/statusline-behaviour.md); each ends at
something runnable, and none starts until its `requires:` predecessors are
executed and merged.

| Plan                                                                  | Target repo     | Requires                         | Status |
| --------------------------------------------------------------------- | --------------- | -------------------------------- | ------ |
| [2026-08-19-1400-main-bar](./2026-08-19-1400-main-bar.md)             | `claude-status` | —                                | active |
| [2026-08-19-1401-subagent-panel](./2026-08-19-1401-subagent-panel.md) | `claude-status` | main-bar                         | active |
| [2026-08-19-1402-spend](./2026-08-19-1402-spend.md)                   | `claude-status` | main-bar                         | active |
| [2026-08-19-1404-caps-hook](./2026-08-19-1404-caps-hook.md)           | `claude-status` | main-bar                         | active |
| [2026-08-19-1403-distribution](./2026-08-19-1403-distribution.md)     | `claude-status` | subagent-panel, spend, caps-hook | active |

**Execution order** — `main-bar`, then `subagent-panel` / `spend` / `caps-hook`
in any order or concurrently in separate worktrees, then `distribution` last.
Filename timestamps record when each plan was written, not when it runs:
`caps-hook` was added after `distribution` and precedes it.

## Not planned here

**Phase 5 — the `ai-plugins` cutover.** Removing `tools/statusline/` and
`~/.claude/hooks/context-caps.js` from `virajp/ai-plugins` and pointing its docs
at this repo. A change to a different repository, gated on
`2026-08-19-1403-distribution` having shipped.

It also owns the two simplifications this repo cannot make until it lands:
deleting the transitional `.config/statusline.json`, and retiring
`$AI_PLUGINS_USAGE_DIR` together with §8's frozen mirror field names — once the
JS hook is gone, this binary is both the writer and the only reader of that
file.
