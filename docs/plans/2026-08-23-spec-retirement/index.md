---
type: vwf-plan-index
title: spec-retirement — 2026-08-23
description: One plan that retires the behaviour contract by rehoming the
  decision record, the ai-plugins contract and the reference payloads, then
  deleting the document.
status: active
covers: [
  docs/decisions.md,
  docs/usage-mirror-contract.md,
  tests/fixtures/README.md,
]
timestamp: 2026-08-23T15:00:00Z
tags: [ docs, spec, retirement, decisions ]
---

# spec-retirement — 2026-08-23

| # | Plan                                       | What it changes                                                 |
| - | ------------------------------------------ | --------------------------------------------------------------- |
| 1 | [retire-the-spec](./01-retire-the-spec.md) | deletes the spec; adds decisions, the mirror contract, fixtures |

## Why the contract is being retired rather than repaired

`docs/spec/statusline-behaviour.md` was validated section by section against the
code on 2026-08-23. The audit is in `DRIFT-2026-08-23.md`, deleted together with
the spec and kept in git history.

**Five claims are actively wrong** — a §12 fixture that prints an error instead
of a bar, a §3 precedence order that contradicts the config order it names as
its own authority, a §3 preservation rule the code inverts, a §10 isolation
recipe that isolates nothing, and a §7 kill-switch superseded by `46ab142`.

**One failure mode produced nearly all of them.** The document is maintained by
strike-through amendments; an amendment updates its own section and never sweeps
for the text it falsified. `8f4efe0` added a CLI surface and a config block,
amended §9, and left §1, §3, §4a and §5 describing a binary with one fewer
surface than it has.

**It recurs.** The archived `subagent-panel` cycle already found and fixed a
stale §12 fixture; the other §12 fixture is now broken the same way.

**The code has already stopped trusting it.** Three comments cite contract
sections to record that they are behind. Where spec and code comment disagreed,
the audit found the comment right every time.

Repairing it fixes five lines and leaves the process that generates them.

## Why it is last, not first

Every other plan **writes to this file** — six of the nine have a Docs step
whose content is "§N records X". Deleting it first would strand those steps and
force six plans to be rewritten to avoid fixing five lines.

Three plans also **read it as authority**, most load-bearingly
[distribution/02](../2026-08-23-distribution/02-homebrew-formula.md), whose
Homebrew `test do` block is derived from a §5 guarantee. Those claims need a
home before the file goes.

## What replaces it

Not one document — three, split by what each part is *for*:

| Part                           | Goes to                         | Why there                                                                                                              |
| ------------------------------ | ------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| User-facing behaviour (§1–§7)  | the site                        | [website/01](../2026-08-23-website/01-site.md) already specifies exactly this content, aimed at the people who need it |
| The decision record            | `docs/decisions.md`             | reasoning, not behaviour — no test can hold it, and it is the only part that cannot be reconstructed                   |
| The `ai-plugins` contract (§8) | `docs/usage-mirror-contract.md` | governs a consumer in another repo, so this repo's tests cannot verify it                                              |
| Reference payloads (§12)       | `tests/fixtures/`               | executable instead of transcribed — which is how they went stale twice                                                 |

Behaviour itself moves to where it already lives: **380 tests and 8 exact-ANSI
goldens.** The discipline that replaces the spec is one line — no document
restates behaviour a test already holds.
