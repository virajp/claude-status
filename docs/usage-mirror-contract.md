# The usage mirror — a contract with `ai-plugins`

**This is not internal.** It is the reason vwf's context-cap hook can work at
all.

Context-window and rate-limit figures arrive **only on the statusline payload**
— never on hook stdin. So every main-bar render mirrors them to a session-keyed
file, which a `PostToolUse` hook then reads.

This document exists separately from the rest of the project's documentation
because **it governs a consumer in another repository and cannot be verified by
this repo's tests alone.** Everything else this project does is pinned by a test
that fails when it breaks; this is pinned by a test that only proves *this* side
still holds up its end.

## Status of this document

| Side                    | Who                                                    | Verified by                                                                    |
| ----------------------- | ------------------------------------------------------ | ------------------------------------------------------------------------------ |
| **Writer**              | `src/modules/usage.rs`, on every `--statusline` render | This repo's suite. Authoritative.                                              |
| **Reader (in-repo)**    | `--caps-hook`, via `caps::Usage::from_mirror`          | This repo's suite. Authoritative.                                              |
| **Reader (cross-repo)** | `context-caps.js` in [`virajp/ai-plugins`][ai-plugins] | **Nothing here.** Every claim about it below is **unverified from this side.** |

[ai-plugins]: https://github.com/virajp/ai-plugins

**Read the third row as a warning, not a formality.** The corrections in
[What the old contract omitted](#what-the-old-contract-omitted) were made by
reading this repository's code, which tells you what is *written*. Whether the
consumer copes with each shape is a claim about a file this repository cannot
run. Where that distinction matters it is called out inline.

## Enabling it

The mirror is **inert** unless a usage directory is named. Two variables, new
name first:

1. `CLAUDE_STATUS_USAGE_DIR`
2. `AI_PLUGINS_USAGE_DIR` — the name `ai-plugins` exports and `context-caps.js`
   reads

**Both names are honoured on both sides** — the writer and the caps hook resolve
the directory through the same function, so the two can never disagree about
which variable won. The fallback exists so the variable can migrate without
breaking a machine still running the JS hook, which only knows the old name.
Once that hook is gone this binary is both the writer and the only reader, and
this document stops describing a cross-repo contract at all.

**An empty value falls back rather than masking.** `CLAUDE_STATUS_USAGE_DIR=""`
resolves to the legacy name, not to "off with the fallback skipped". This is a
per-arm filter rather than one at the end of the chain, and the difference is
load-bearing: `var()` yields `Ok("")` for an empty variable, so a chain that
filtered emptiness only at the end would see `Some("")` from the new name, never
try the legacy one, and only then discard it — **letting an emptied new variable
silently take the caps hook's only data with it.**

The mirror is off only when **neither** variable carries a non-empty path.

## The path

`<dir>/<session_id>.json`.

A leading `~`, `$HOME` or `${HOME}` in the directory is expanded, because Claude
Code may or may not have expanded it before exporting. The expansion is
**deliberately loose, matching the original**: `${HOME` and `$HOME}` expand too,
because every spelling arrives in the wild.

**A directory that names `$HOME` when there is none resolves to nothing, and no
mirror is written.** It must never degrade to the unexpanded text: `~/usage`
taken literally is a *relative* path, so the file would land in whatever
directory Claude Code was launched from. The caps hook applies the identical
rule, **so it never reads from a directory the bar would not have written to.**

The directory is created if it does not exist. The write is atomic —
`<session_id>.json.<pid>.tmp` in the same directory, then renamed — so a
concurrent reader never sees a half-written file. The JSON is **compact**, not
pretty-printed: nobody opens this file by hand.

## The document

Nine keys, in this order, and the order is preserved deliberately.

```jsonc
{
  "sessionId": "abc123",
  "ts": 1787037452146,
  "ctxPct": 26,
  "ctxUsed": 259000,
  "ctxSize": 1000000,
  "fiveHourPct": 7,
  "fiveHourResetsAt": 1774200000,
  "sevenDayPct": 1.0,
  "sevenDayResetsAt": 1774600000,
}
```

| Key                | Source                       | When the payload does not carry it |
| ------------------ | ---------------------------- | ---------------------------------- |
| `sessionId`        | the payload's `session_id`   | no file is written at all          |
| `ts`               | the render's own clock       | always present                     |
| `ctxPct`           | context used, percent        | `null`                             |
| `ctxUsed`          | context tokens used          | `null`                             |
| `ctxSize`          | the context window's size    | **the key is absent**              |
| `fiveHourPct`      | 5-hour window used, percent  | `null`                             |
| `fiveHourResetsAt` | 5-hour window reset, **raw** | `null`                             |
| `sevenDayPct`      | 7-day window used, percent   | `null`                             |
| `sevenDayResetsAt` | 7-day window reset, **raw**  | `null`                             |

**Numbers are written whole where they are whole.** `26` mirrors as `26`, not
`26.0`, because the consumer compares these against thresholds. A fractional
value survives as one (`1.5`), and a non-finite value is `null`.

**No file is written** when the usage directory or the session id is missing or
empty.

### Which keys the in-repo reader actually uses

`--caps-hook` reads **five** of the nine: `ctxPct`, `fiveHourPct`,
`sevenDayPct`, `fiveHourResetsAt` and `sevenDayResetsAt`. A missing percentage
reads as `0.0`, which never breaches.

`sessionId`, `ts`, `ctxUsed` and `ctxSize` are written for the cross-repo
consumer and are **not** read by anything in this repository. That is worth
knowing before assuming this repo's suite protects them: **it does not protect
their meaning, only that they are still emitted.**

## What the old contract omitted

Three shapes the shipped writer has and the retired §8 did not describe. Each
was found by reading the code, and each is called out here rather than quietly
folded into the layout above, **because a consumer written against §8 was
written against a document that did not mention them.**

### 1. `ctxSize` is absent when unknown; the other six are `null`

`ctxSize` is inserted **only when the payload carried a context window size**.
Every other value field — `ctxPct`, `ctxUsed`, and the four rate-limit fields —
is present as `null` when unknown.

So the document has **nine keys or eight**, and the missing one is always
`ctxSize`.

This asymmetry is not a tidy design; it is what the consumer was written
against, and it is pinned by a test on this side so it cannot drift. **A
consumer that reads `ctxSize` with a "present but null" assumption gets
`undefined` rather than `null`** — which in JavaScript compares differently
against a numeric threshold than `null` does.

> **Unverified.** Whether `context-caps.js` distinguishes the two is a fact
> about a file in another repository. This document records the shape that is
> written, not an agreement that the consumer handles it.

### 2. `resets_at` is mirrored raw

The reset timestamps are **not normalised**. Whatever the payload carried
arrives unchanged: epoch seconds stay seconds, epoch millis stay millis, **and
an ISO-8601 string stays a string.**

The main bar normalises these for its own rendering — discriminating seconds
from millis on `> 1e12`, and parsing ISO — but the mirror deliberately does not.
**The consumer does its own discrimination**, and normalising here would mean
two implementations of the same three-way guess, drifting independently.

The in-repo reader takes both values as raw JSON for the same reason.

### 3. `<session>.state.json` is a neighbour in the same directory

The usage directory holds **two** files per session, written by **two different
things**:

| File                      | Written by                                                                     | Purpose                                  |
| ------------------------- | ------------------------------------------------------------------------------ | ---------------------------------------- |
| `<session_id>.json`       | the main-bar render                                                            | the mirror this document describes       |
| `<session_id>.state.json` | the caps hook — `--caps-hook`, and during the transition `context-caps.js` too | the escalation debounce: `{ level, ts }` |

§8 never mentioned the second file at all.

**The debounce file's name is not this project's to choose.** It sits beside the
mirror, and during the transition the JS hook writes it too — a machine running
both must not double-fire, which it would if the two used different names.

The writer asserts the two names cannot collide: a session id would have to
contain a literal `.state` for `<session_id>.json` to end in `.state.json`.
**That check is a `debug_assert!`** — it holds the shape in the test build and
costs nothing in the released one, because the collision needs a hostile session
id that Claude Code does not produce.

> **Correction to how this was previously framed.** The retirement plan
> described `<session>.state.json` as a file `context-caps.js` writes. That was
> true when the plan was written and is now only half true: **`--caps-hook`
> writes it as well**, and after the JS hook is gone it will be the only writer.
> The name is still not unilaterally ours to change while both exist.

**A failed write of the debounce file never suppresses the directive** — a
read-only usage directory should cost the debounce, not the cap.

## Best-effort, always

**A failure here must never affect the rendered line.** The mirror runs before
anything that can fail, and is gated on neither the layout nor git — a broken
config or a slow repo must not cost the caps hook its data. An unwritable
directory is silently survived.

This is the render-never-fails rule applied to a side effect: the mirror is a
thing the bar does *for someone else*, and no amount of failing at it may reach
the user's status line.

## Changing this format

**Keep these field names and this file layout byte-compatible.** A consumer
lives in another repo. Changing the format is a **coordinated change across
both**, and this document is where the old shape is written down.

**The env var name is part of the contract too** — renaming it silently disables
vwf's caps, with nothing on any stream to say so.

The one safe unilateral change is the one already in flight: honouring a new
name while continuing to honour the old, on both sides, until the old reader is
gone.

## Provenance

Lifted from §8 of `docs/spec/statusline-behaviour.md` on 2026-08-27, as step 3
of the `retire-the-spec` cycle, and corrected against `src/modules/usage.rs`,
`src/_runtime/app.rs` and `src/modules/caps/mod.rs` as it moved. The three
corrections above are the audit's findings; the spec was deleted three steps
later. The reasoning behind the decisions this contract encodes lives in
[decisions.md](./decisions.md).
