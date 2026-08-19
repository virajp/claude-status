---
type: vwf-plan-index
title: Cycle plans — claude-status
description: One row per cycle plan, its target repo, and its status.
status: stable
---

# Cycle plans

The Rust rewrite of the Claude Code statusline, decomposed into four chained
cycles. Each plan is a diff against
[the behaviour contract](../spec/statusline-behaviour.md); each ends at
something runnable, and none starts until its `requires:` predecessors are
executed and merged.

| Plan                                                                  | Target repo     | Requires              | Status |
| --------------------------------------------------------------------- | --------------- | --------------------- | ------ |
| [2026-08-19-1400-main-bar](./2026-08-19-1400-main-bar.md)             | `claude-status` | —                     | active |
| [2026-08-19-1401-subagent-panel](./2026-08-19-1401-subagent-panel.md) | `claude-status` | main-bar              | active |
| [2026-08-19-1402-spend](./2026-08-19-1402-spend.md)                   | `claude-status` | main-bar              | active |
| [2026-08-19-1403-distribution](./2026-08-19-1403-distribution.md)     | `claude-status` | subagent-panel, spend | active |

`subagent-panel` and `spend` both depend only on `main-bar`, so they may be
executed in either order, or concurrently in separate worktrees.

## Not planned here

**Phase 5 — the `ai-plugins` cutover.** Removing `tools/statusline/` from
`virajp/ai-plugins`, pointing its docs at this repo, and confirming
`context-caps.js` still reads the same usage-mirror file. That is a change to a
different repository and belongs to a plan in *that* repo, gated on
`2026-08-19-1403-distribution` having shipped.
