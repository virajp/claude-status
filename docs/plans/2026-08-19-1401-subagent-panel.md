---
type: vwf-plan
title: subagent-panel — 2026-08-19
description: Cycle plan (a diff) for the subagent panel — status matching,
  type glyphs, the description budget, and NDJSON output.
status: draft
covers: [
  docs/spec/statusline-behaviour.md,
]
requires: [
  docs/plans/2026-08-19-1400-main-bar.md,
]
timestamp: 2026-08-19T14:01:00Z
tags: [ rust, statusline, phase-2 ]
---

# Plan: subagent-panel — 2026-08-19

## Slice

The subagent panel — the second rendering surface, selected by the
**`--subagent` flag**, not by the payload's shape. Contract §2 (subagent
payload) and §3 (subagent styling and status matching), plus the §10 Phase-2
verification clause of
[the behaviour contract](../spec/statusline-behaviour.md).

Plan 2 of 5 — requires [`main-bar`](./2026-08-19-1400-main-bar.md); required by
[`distribution`](./2026-08-19-1403-distribution.md).

Independent of [`spend`](./2026-08-19-1402-spend.md): the two share only what
plan 1 landed, so they may be executed in either order or concurrently in
separate worktrees. The panel path never reads the spend cache and never writes
the usage mirror.

## Current state (actual)

After [`main-bar`](./2026-08-19-1400-main-bar.md):

- `render/powerline.rs`, `config/`, `fmt.rs`, `time.rs`, `git.rs` and `json.rs`
  all exist and are tested. This cycle adds one module and reuses the rest
  unchanged.
- `cli.rs` already recognises `--subagent`, but returns empty output for it.
- `assets/claude-status.defaults.json` already carries the full `subagent` block
  — `descBudgetFraction`, the six `segments`, and the four `statuses` with their
  match regexes — and `typeSymbols` with its nine glyphs plus `_default`.
- `config/matcher.rs` already compiles literal-alternation patterns to
  `contains`; every shipped status regex is one.
- `serde_json`'s `preserve_order` is already enabled, which this cycle depends
  on.

## Target state (per blueprint)

`--subagent` with a payload carrying a `tasks` array emits **NDJSON** — one
`{"id": …, "content": …}` object per line, `content` being a fully rendered ANSI
row. Not a single blob, and no trailing newline.

The flag decides the surface; the payload no longer does. A `--subagent`
invocation whose payload carries **no** `tasks` array renders empty output and
exits 0 — it does not fall back to the main bar, because a silent surface swap
is exactly the failure the flags exist to prevent.

Two traps the contract calls out and one it gets wrong:

- `type` is almost always the generic `"local_agent"` regardless of the real
  subagent type, so it renders as a **glyph, never as text**.
- Neither `model` nor `effort` is a documented per-task field. Read a per-task
  value if present, else the panel-wide value, else omit the segment.
- **The contract's §3 table is wrong** where it says `name` falls back to the
  type. It does not: `name` is simply omitted when absent. The upstream
  `statusline.md` carries the same stale wording.

## Delta — ordered steps

1. **`src/modules/render/subagent.rs` — status matching.** `task_mark(status)`
   lowercases `status` (so patterns only ever see lowercase), then walks
   `subagent.statuses` **in config insertion order** — which is why plan 1 took
   `preserve_order`; a `BTreeMap` or `HashMap` would silently reorder a
   user-authored config. First entry whose `match` hits wins. An entry with an
   empty `match` is recorded as the fallback and the loop **does not break**, so
   with two empty-`match` entries the **last** one wins. A pattern that fails to
   compile is skipped silently, not fatal. *Test-first:* the four shipped
   statuses resolve to their symbol and colour; `"not_ok"` matches `done` (the
   regexes are unanchored substring matches, and `ok` is inside `not_ok`) —
   faithful, not a bug to fix; an unknown status takes the `pending` fallback; a
   config with two empty-`match` entries takes the last.

2. **Type glyphs.** `typeSymbols[type]`, falling back to `typeSymbols._default`.
   Never rendered as text. *Test-first:* `"local_agent"` → U+F109; an unlisted
   type → U+F544.

3. **The description budget.** `cols` resolves `payload.columns` → `$COLUMNS` →
   `80` — the contract mentions neither the env var nor the default.
   `budget = max(12, floor(cols × descBudgetFraction))`, so 120 columns gives 54
   and the absent-`columns` case gives 36.

   Normalisation the contract omits entirely: `description` else `label` else
   `""`, then **all whitespace runs collapse to a single space** and the result
   is trimmed. An empty result omits the segment.

   Truncation is to `budget - 1` **UTF-16 code units** plus U+2026 HORIZONTAL
   ELLIPSIS (one character, not three dots), so a truncated description is
   exactly `budget` units long. Take `encode_utf16().take(n)` and lossy-decode:
   a naive `&desc[..budget-1]` **panics** on a non-char boundary, and JS `slice`
   would split a surrogate pair there. *Test-first:* `columns: 120` → budget 54;
   no `columns` and no `$COLUMNS` → budget 36; `columns: 0` falls through to
   `$COLUMNS` then 80; `descBudgetFraction: 0` clamps to 12; a
   newline-and-tab-laden description collapses to single spaces; an emoji
   straddling the cut does not panic; a truncated result is exactly `budget`
   UTF-16 units.

4. **The row.** Segments in order: `head`, `name`, `model`, `desc`, `tokens`,
   `duration`, each conditional.
   - `head` is `"{status symbol} {type glyph}"` — a space between them
     **always**, even when the status symbol is empty. Its background **always
     comes from the matched status**; `subagent.segments.head.bg` is ignored,
     and only `bold` and `fg` are read from it, once before the task loop rather
     than per task.
   - `name` is `{agent} {name}`, omitted when absent. **No fallback to `type`.**
   - `model` is `{model} <label>` where the label joins the model name and
     `[effort]`, skipping empties — so effort with no model yields just
     `[high]`. Per-task `model`/`effort` win over panel-wide.
   - `tokens` renders when `tokenCount` is present, so `0` renders `0`.
   - `duration` is skipped when `startTime` does not parse **or is falsy**, and
     may go negative for a future `startTime` — reproduce rather than clamp.

   The panel's cwd chain differs from the main bar: `payload.cwd` → the first
   task's `cwd` → the process cwd. `workspace` is **never** consulted. The repo
   config layer still comes from the resolved git root, so a panel does pick up
   per-repo config. *Test-first:* golden rows for the full six-segment case and
   for each omission.

5. **NDJSON output.** One `{"id": …, "content": …}` per task, joined with `\n`,
   **no trailing newline**. `id` keeps its original JSON type — a number stays a
   number. ESC serialises as the six-character escape `\u001b` while the Nerd
   Font glyphs stay raw UTF-8; `serde_json` matches both, but assert it. A task
   that is not an object, or whose `id` is absent or `null`, is skipped — but
   `id: 0` and `id: ""` are **kept**. Zero tasks yields empty stdout.
   *Test-first:* the §12 subagent fixture round-trips through `jq -r .content`;
   a task without an `id` is skipped rather than crashing; `id: 0` survives as
   the number `0`.

## Acceptance criteria (from blueprint)

Derived from [the behaviour contract](../spec/statusline-behaviour.md) §§2–3 and
its §10 Phase-2 verification clause; it carries no `Acceptance` blocks.

- [x] Given `--subagent` and a payload with **no** `tasks` array, when the
      binary runs, then stdout is empty and the exit code is 0 — it never falls
      back to the main bar — this plan's decision
- [x] Given `--subagent` and a payload with a `tasks` array, when the binary
      renders, then stdout is NDJSON — one JSON object per line, each with `id`
      and `content` — and not a single blob — from
      [contract §2](../spec/statusline-behaviour.md)
- [x] Given the §12 subagent fixture, when the output is piped through
      `jq -r .content`, then a sane single powerline row is printed — from
      [contract §10, Phase 2](../spec/statusline-behaviour.md)
- [x] Given a task with no `id`, when the binary renders, then that task is
      skipped and the remaining tasks still render — from
      [contract §10, Phase 2](../spec/statusline-behaviour.md)
- [x] Given a task whose `type` is `"local_agent"`, when the binary renders,
      then the type appears as a glyph and the string `local_agent` appears
      nowhere in the output — from
      [contract §2](../spec/statusline-behaviour.md)
- [x] Given a task with no per-task `model` and a panel-wide `model`, when the
      binary renders, then the panel-wide value is used; and given neither, the
      model segment is omitted — from
      [contract §2](../spec/statusline-behaviour.md)
- [x] Given `columns: 120` and a description longer than 54 characters, when the
      binary renders, then the description is truncated to 53 units plus `…` —
      from [contract §3](../spec/statusline-behaviour.md)
- [x] Given statuses declared in a non-alphabetical order in a user config, when
      the binary matches, then the first matching entry in **config order** wins
      — from [contract §3](../spec/statusline-behaviour.md)
- [x] Given a subagent payload, when the binary renders, then no spend cache is
      read, no refresh child is spawned, and no usage mirror is written — from
      [contract §§7–8](../spec/statusline-behaviour.md)

## Risks / drift

- **The contract and the upstream docs both misstate the `name` fallback.** Both
  say `name` falls back to the task's `type`; the code does not. Step 4 follows
  the code. **Resolving step:** amend `docs/spec/statusline-behaviour.md` §3 in
  this cycle.
- **The contract omits the description-budget details entirely** — the
  `$COLUMNS` fallback, the 80 default, the whitespace collapse, the UTF-16
  measurement and the U+2026 ellipsis. All are observable. **Resolving step:**
  fold step 3's specifics back into the contract.
- **UTF-16 truncation is a deliberate fidelity choice, not a good one.** It
  measures code units, so an emoji costs 2 and a CJK character costs 1 despite
  occupying 2 terminal columns. A CJK-heavy description will therefore overrun
  its visual budget and may wrap. Shipping without `unicode-width` because the
  failure is cosmetic and the JS has the same flaw; if it looks bad in practice,
  swapping in `unicode-width` is a change confined to this one module — but it
  breaks byte-fidelity with the JS, so it needs a decision, not a quiet fix.
- **The panel is untested against a real Claude Code subagent payload.** Every
  fixture here is hand-written from the contract. The `type`-is-always-generic
  trap was learned the hard way once; there may be more. Capture a real payload
  during execution and add it as a golden.

## Out of scope for this cycle

- The `spend` segment — [plan 3](./2026-08-19-1402-spend.md). The panel never
  renders it.
- Per-task `model`/`effort` becoming real upstream fields — the fallback chain
  is built to accept them, but nothing here depends on them existing.
- Terminal-width-aware truncation (`unicode-width`) — see Risks / drift.
- Packaging — [plan 5](./2026-08-19-1403-distribution.md).

## Gaps surfaced during execution

- **The §3 amendment this plan promised was not owed.** The risk list says the
  contract's §3 table claims `name` falls back to the task's `type`. It does
  not: §2 already says "Do not fall back to showing it as the name", and §3
  carries only the styling. The stale wording is in the **upstream**
  `statusline.md` in `ai-plugins`, which is not this repo's to fix — it goes
  with the Phase 5 cutover. Amended the two things that *were* missing instead:
  the description budget, which §3 omitted entirely, and §12's fixture command.
- **§12's subagent recipe was stale in a second way.** It piped the fixture into
  a bare `claude-status`, which since the `main-bar` cycle made the flag choose
  the surface renders the **main bar** — so the documented way to look at the
  panel never showed the panel. Now spelled with `--subagent`, with a note
  saying why the flag is not optional.
- **A panic on this surface yields empty output, not `⚡ Claude`.** The JS entry
  point printed the fallback line whichever surface was rendering. Here that
  would be a line of NDJSON that is not JSON, and the consumer parses every
  line. Deliberate divergence, recorded because it is the one place the panel
  breaks the third invariant's *letter* while keeping its intent.
- **The "no refresh child" criterion is proven structurally, not by assertion.**
  `tests/e2e.rs` compares the spend cache bytes immediately after the process
  exits, which a detached child could in principle beat. The real guarantee is
  that `build_panel` never calls the spend resolver, so there is no gate to pass
  and no child to spawn — but that is an argument about the code, not a test.
  **Resolving step:** if the panel ever grows a reason to touch the cache, this
  needs a real probe (a sentinel the child would overwrite), not a tightened
  race.
- **Two tests stopped meaning what their names said, and this cycle is why.**
  `app::the_unbuilt_surfaces_are_silent` asserted `dispatch(Subagent) == ""`;
  once the surface renders, that unit test inherits the real process's stdin,
  `$HOME` and git root — the same hazard the `spend` cycle recorded when two of
  its tests quietly became live-fetch tests. Removed, with the coverage moved to
  `tests/e2e.rs`. `e2e::the_non_rendering_surfaces_are_recognised_and_silent`
  was narrowed to `--refresh-spend` and renamed. **Worth generalising:** a cycle
  that makes an inert surface live owes a pass over every test that asserted it
  was inert — those tests keep passing and stop testing anything.
- **The panel is still untested against a real Claude Code subagent payload.**
  The plan's own risk, unchanged: every fixture here is hand-written from the
  contract, including the three goldens. The `type`-is-always-generic trap was
  learned the hard way once. **Resolving step:** capture a real payload the next
  time subagents run and add it as a fourth golden.
- **UTF-16 truncation shipped as planned, and the flaw is still there.** The
  budget counts code units, so a CJK-heavy description occupies twice its
  measured width and may wrap. Unchanged from the JS, and `unicode-width` would
  break byte-fidelity with it, so it stays a decision rather than a quiet fix.
- **One divergence not in the plan: a non-numeric `descBudgetFraction`.** In the
  JS it produced a `NaN` budget, and `length > NaN` is false, so a hand-broken
  config silently disabled truncation altogether. Here it falls back to `0.45`.
