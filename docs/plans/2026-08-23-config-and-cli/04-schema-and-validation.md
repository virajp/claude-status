---
type: vwf-plan
title: schema-and-validation — 2026-08-23
description: Cycle plan (a diff) making the Rust config types generate the JSON
  schema, adding a validation section to --debug, and publishing the schema for
  the website's generator to consume.
status: done
covers: [
  docs/decisions.md,
]
requires: [
  docs/plans/2026-08-23-config-and-cli/03-cli-surface.md,
]
timestamp: 2026-08-23T14:04:00Z
tags: [ config, schema, schemars, validation, debug, schemastore ]
---

# Plan: schema-and-validation — 2026-08-23

## Slice

Contract §3 (Configuration) and §5 (`--debug`). The Rust types become the single
source for three consumers: the renderer (already, from plan 1), the committed
JSON schema, and a `--debug` validation section.

**This is also the plan the website waits on.**
[website/02](../2026-08-23-website/02-config-generator.md) builds its form from
the committed schema, so the schema stops being documentation and becomes an
interface.

## Current state (actual)

**After plans 1–3**, the config is deserialized into Rust types whose `Default`
provably matches the embedded JSON, lives at
`~/.config/claude-status/config.json`, and stores only non-defaults. Unknown
keys are still silently ignored — plan 1 deliberately left `deny_unknown_fields`
off so no behaviour moved while the types landed.

`schemas/claude-status.schema.json` is 301 hand-written lines. Nothing checks it
against the types; they agree only by whoever last edited both.

**The schema's real value is its prose.** `symbols` enumerates the nineteen keys
the script consumes. `spend.show` explains why `auto` renders only for team
seats. `worktreePattern`, `caps` and `lines` each carry a paragraph. This is the
part a generator can lose without anyone noticing until an editor stops being
helpful.

**`$schema` and `$id` point at the mutable `main` ref**, via a `SCHEMA_URL`
constant. After [distribution/01](../2026-08-23-distribution/01-drop-npm.md)
deletes the installer, the only remaining writer is the binary.

**`--debug` has the surface this lands in.** Plan 3 already rewrote
`CONFIG LAYERS` to distinguish absent from broken and to report repo keys that
were ignored.

**Four keys are open maps** — `palette`, `symbols`, `typeSymbols`, `segments` —
whose keys are user-chosen. The rest are closed.

## Target state (per contract)

`schemas/claude-status.schema.json` is generated output, checked for drift on
every commit. `--debug` reports what it could not make sense of, per layer, and
changes nothing about what renders. The schema is listed on SchemaStore so
editors find it by filename.

## Delta — ordered steps

### 1. Add `schemars` as a dev-dependency and a generation task

**Dev-dependency, not a dependency.** The generator runs at author time; nothing
about it belongs in a binary that redraws every four seconds. The derives sit
behind a `schema` feature so the release build does not carry them.

`mise run code:schema` regenerates the file in place, matching the existing
`code/*` task library.

### 2. Carry the descriptions across

Descriptions move into `#[doc]` comments on the types, which `schemars` emits as
`description`. **A generated schema with fewer descriptions than the
hand-written one fails this cycle** — criterion 3.

### 3. Fail the build on schema drift

`code:precommit` and CI regenerate to a temp file and diff against the committed
one. A difference fails, naming `mise run code:schema` as the fix.

Committed rather than generated at build time because editors and the website
fetch it over HTTP from a stable path on `main`.

### 4. Turn on `deny_unknown_fields` — for the validator only

On the closed objects: `powerline`, `spend`, `caps`, `subagent`, the style
objects. **Not** the open maps, where an unknown key is a legal entry.

**The render path keeps the permissive types.** Plan 1 made a deserialize
failure fall back to defaults, so a strict derive on the render path would let
one typo blank a user's entire config — strictly worse than today's silence and
a regression against the never-fail rule. The strict shape is a second
deserialization used only by the validator. One set of types, two derives: the
shapes cannot drift because there is only one shape.

### 5. Report per-layer findings in `--debug`

Findings attach to the layer that caused them, not to the merged result:

```text
CONFIG LAYERS (low to high)
  embedded loaded          <built in>
  user     loaded          ~/.config/claude-status/config.json
           ⚠ unknown key `powerlin` (did you mean `powerline`?)
           · symbols.contxt is not a key this binary reads
           · gauge.width 0 → 10
  repo     using defaults  <no git root>
```

Three kinds:

- **⚠ unknown key** in a closed object — a typo, reportable with confidence.
- **· not a key this binary reads** — a key in an open map outside the consumed
  set. A note, never a warning: it is legal.
- **· coerced** — an empty `gauge.filled`, a `0` width, an uncompilable
  `worktreePattern`. **The most useful of the three, and the one no schema can
  give you**: it reports what the binary *did* with the value, not what the
  value is allowed to be.

### 6. Leave the URL on `main`, and list on SchemaStore

**Not pinned to a tag.** Once generated from the types, the schema tracks the
code by construction. Pinning would freeze an editor on the schema of whatever
version happened to seed the file — worse for the common case, where the user
upgrades the binary and keeps their config.

Submit to SchemaStore's catalog matching `claude-status.json`. `--configure`
keeps seeding the key regardless: a config that carries its own pointer works in
editors that consult no catalog.

**SchemaStore is a PR into a queue outside this repo.** It is the last step and
deliberately **not** an acceptance criterion.

### 7. Docs

§3 records that the schema is generated, that validation is advisory, and that
unknown-key detection covers closed objects only. §5's `--debug` description
gains the validation section. `CONTRIBUTING.md` gains `mise run code:schema`.

## Acceptance criteria (from contract)

1. Given a clean checkout, when `mise run code:schema` runs, then the committed
   schema is unchanged.
2. Given a type with a field added and the schema not regenerated, when
   `code:precommit` runs, then it fails and names the fix.
3. Given the generated schema, when its `description` strings are compared with
   the hand-written schema at this cycle's parent commit, then none is lost.
4. Given a user layer with `powerlin` instead of `powerline`, when `--debug`
   runs, then it is reported **under the user layer** — and when the bar
   renders, then it renders exactly as it did before the typo was introduced.
5. Given `gauge.width` of `0`, when `--debug` runs, then it reports the coercion
   to 10.
6. Given an unknown key in `palette`, when `--debug` runs, then it is **not**
   reported as an error.
7. Given any config at all, when the bar renders, then `--debug`'s findings
   change nothing on stdout and the exit code is 0.
8. Given the release binary, when its dependency tree is inspected, then
   `schemars` is absent.
9. Given the committed schema, when it is fetched and parsed as draft-2020-12 by
   a third-party validator, then it is valid — the website consumes this file,
   so being well-formed is now an interface obligation.

## Risks / drift

**The strict/permissive split is the subtle part.** Getting step 4 wrong in the
direction of strictness means a typo blanks a config — worse than today.
Criterion 4 tests exactly that: reported *and* harmless.

**A generated schema can be worse than a hand-written one.** The existing file
carries prose written for a human editing JSON. `schemars` emits structure
faithfully and prose only if it is in `#[doc]`. Criterion 3 should be checked by
diffing descriptions, not by eyeballing the file.

**The schema is now load-bearing for the website.** A malformed or
under-described schema produces a form with unlabelled fields. Criterion 9 makes
that a gate rather than something discovered later in
[website/02](../2026-08-23-website/02-config-generator.md).

**SchemaStore is not this repo's to merge.** It can sit, be rejected, or ask for
changes. If it has not merged when everything else has, record it in Gaps and
finish; do not hold a cycle open on someone else's queue.

**Leaving the URL on `main` is a real trade, taken deliberately.** A user on an
old binary gets an editor validating against a newer schema, which can offer a
key their binary ignores. Pinning gets the reverse and worse case: an editor
rejecting a key their upgraded binary supports, forever, because the URL was
frozen at install time. Generation makes the first case rare and
self-correcting; nothing makes the second self-correct.

**The open-map limit undercuts the original ask.** The goal was that a typo'd
key be distinguishable from a key that does nothing. For `symbols`, `segments`,
`palette` and `typeSymbols` — a large share of what people actually edit — it
still is not, and cannot be. Step 5's note is the mitigation, and the website
should say so plainly rather than leave a user to discover it.

## Out of scope for this cycle

- **Making validation fail a render, or exit non-zero.** Both answered *no*.
  §3's never-fail rule is not up for renegotiation in a diagnostics cycle, and a
  non-zero `--debug` would break the pipelines people run it in.
- **Typo detection in open maps.** Not possible; see Risks.
- **Pinning the schema URL to a tag.** Decided against; see step 6.
- **The website's form.**
  [website/02](../2026-08-23-website/02-config-generator.md) consumes this
  schema but is planned separately.

## Gaps surfaced during execution

**Step 1 was impossible as written, and step 4 was wrong.** Both are recorded
here rather than silently corrected, because the reasoning that produced them is
the reasoning a reader will apply again.

- **A dev-dependency cannot back a derive on a library type.** The derive
  expands inside the lib crate, where a dev-dependency is not in scope
  (`E0433`), and Cargo rejects `optional = true` on a dev-dependency outright.
  `schemars` is therefore a genuine `[dependencies]` entry made optional behind
  a `schema` feature that nothing enables by default. The step's motive is
  intact and measured: `cargo tree -e normal` reports zero `schemars`.
- **`deny_unknown_fields` on the render types would have been a regression, not
  a strictness knob.** Four separate reasons, each verified: `Caps` has a
  hand-written `Deserialize`, so the attribute there compiles clean and does
  nothing; on the five derived blocks the error is swallowed by `block()`, so a
  typo inside `powerline` would blank the block and draw a bar with **no
  separators**; on `Config` it would reject `$schema`, which the binary writes
  into every config itself; and it cannot attribute a finding to a layer,
  because the merge has already happened. The validator is a `serde_json::Value`
  walk against the generated schema's key set instead. It also reports **every**
  finding rather than serde's first, and it can report coercions, which no
  deserialization can.

**Descriptions live in `#[schemars(description = …)]`, not in `#[doc]`.** Step 2
said doc comments. These types' doc comments explain *deserializers* to a Rust
reader — `PowerlineConfig::cap`'s is three sentences about
`as_str().unwrap_or_default()` — and putting that in an editor's hover for
someone writing JSON would be worse than the hand-written schema it replaced.
Rewriting them for the schema would have destroyed the more valuable text. So
each user-facing string is an explicit attribute, and `description = ""`
suppresses the doc-comment fallback on the containers.

**The schema now accepts `null` where a colour is expected, and the hand-written
one was wrong not to.** An explicit `null` clears a colour the defaults set so
the segment falls through to `defaultFg`, and `write::non_defaults` writes that
key back verbatim — meaning `--configure` produced a file that failed to
validate against the schema it stamps into it. Found by the round-trip test
rather than by reading. `$defs/color`'s description gained a sentence saying so;
it is the only one of the 39 whose text changed.

**Criterion 9 says "fetched" and that cannot gate a pull request.** `$id` serves
`main`, so the document at that URL is the *previous* commit's until this one
merges. `tests/schema.rs` compiles the committed file from disk with `boon`
(draft-2020-12), which is the closest thing that can fail before a merge rather
than after one.

**Two guards were nearly vacuous and were fixed, not shipped.**

- A `title` strip was written on the belief that schemars titles every block
  from the first line of its doc comment. It does not —
  `get_title_and_description` returns a title only when the comment's first
  character is `#`. The strip was removing nothing and would have made its own
  test unable to fail, so it was deleted; the test now goes red for a
  heading-style doc comment, which was confirmed by writing one.
- Criterion 6 ("an unknown key in `palette` is not an error") passes with zero
  lines of code, because `palette` is an open map and no key in it *can* be
  unknown. It is asserted positively instead: the key must appear, under its
  layer, as a `·` note.

**The `symbols` consumed set is the shipped keys, not the lookup sites.** The
binary's `config.symbol("…")` calls are scattered across the segment builders
and a grep of them is not a contract. The embedded layer's `symbols` keys are
this binary's own statement of which glyphs it draws, so that is what an unread
note is measured against. It errs towards silence — the defaults ship `repo`,
which no builder asks for, so a `repo` key is not reported.

**`typeSymbols` and `subagent.statuses` can never produce an unread note**, and
that is a limit rather than an omission: their keys are a subagent's `type` and
the user's own bucket labels, so every key in both is read and there is no list
to check one against. The plan's Risks section says the open-map limit undercuts
the original ask; this is where it bites hardest.

**One unexplained e2e failure, not reproduced.** A single run of the full suite
reported one e2e test failing; the name was not captured, and nine consecutive
full-suite runs since have been clean. Most likely the pre-existing timing
assertion in `a_hanging_git_costs_one_shared_budget_not_one_per_subprocess`
under parallel load, but that is a guess and is recorded as unresolved.

**Not done: SchemaStore.** Step 6's catalog submission is a PR into another
repository's queue and the plan explicitly excludes it from the acceptance
criteria. Nothing here blocks it.

### Found in review, fixed in the consolidated round

**Two guards could not tell a derived value from a copy that matched.** Both
were found by mutation — each stayed green with its wiring replaced by a
hand-copied literal — and they needed different repairs, because only one was a
real hole.

- **The caps defaults were a third copy, and nothing checked them.**
  `tests/schema.rs` hard-coded `EXPECTED_DEFAULTS` as `65/90/80/90`, the same
  four values as `caps::DEFAULTS` and the ones `restore_caps_defaults` injects.
  Hard-code the injection *and* change a shipped cap and the schema would
  advertise a threshold the binary does not use, with nothing red. The
  expectation is now derived from `caps::DEFAULTS`. Verified by mutation:
  `context: 65 → 70` now turns **two** tests red where it previously turned one,
  and the new one fires on the constant rather than on regeneration.
- **The `$id` guard's doc comment overclaimed, but the guard is sound.**
  `write.rs` and `tests/schema.rs` both said replacing the wiring with a
  hand-copied literal "goes red". It does not — the two values still agree, and
  no assertion can distinguish derivation from a copy. What the test *does*
  catch is the case that matters: change `SCHEMA_URL` with a literal in place
  and the regenerated `$id` is the old URL against the new constant. Both
  comments now say that instead. **This is the third consecutive cycle to ship a
  comment claiming coverage it lacked** — cycle 02 at C7, cycle 03 at C7.

### Verified by the orchestrator rather than by a reviewer

The behaviour-preservation reviewer never reported, so criteria 4–7 were checked
directly against the built binary. All four hold:

|    | Observed                                                                                                                            |
| -- | ----------------------------------------------------------------------------------------------------------------------------------- |
| C4 | ``⚠ unknown key `powerlin` (did you mean `powerline`?)`` under the **user** layer; bar byte-identical to the config without the key |
| C5 | `· gauge.width 0 → 10`, and `· gauge.width 99999 → 1000` — the clamp cycle 01 recorded as unreported is now reported                |
| C6 | `· palette.notAColour is not a key this binary reads` — a note, not a warning                                                       |
| C7 | exit `0` across seven configs including a malformed file and an array root                                                          |

`$schema` in a user layer is correctly **not** flagged, which was the landmine:
`--configure` writes it and `Config` does not model it.

**Differential gate:** 19 configs, **0 differences** against `fca2aa2`, 18
carrying real output. Control 1 (old vs old) found 0 phantom diffs; control 2
proved the harness can see a real config change. Re-run after the fixes, against
a rebuilt binary.

`mise run code:precommit` exits **0** on a clean tree with the drift hook
ordered before the formatter, so removing `|| true` did not make the task
unusable.

### Method gaps, not code gaps

- **Two mutate-then-revert reviewers in one worktree corrupt each other.** One
  found a `$schemaMUTANT` it had not written; whoever reverts second restores
  the other's mutation as "original". Resolved mid-cycle with a serialised
  mutation lock. **A future cycle should give each reviewer its own worktree** —
  this is a property of the method, not of this slice.
- **Subagents went idle without delivering, repeatedly.** The coder never
  reported at all (its findings survive only because it wrote them here); one
  reviewer's first report was lost entirely and never recovered; the other never
  reported. The pings the previous cycle recommended did not reliably retrieve
  them this time. Everything above was therefore re-derived by running.
- **A recon agent destroyed the worktree's `Cargo.toml`** by running a scratch
  `cargo init` in place while reporting it had used `/tmp`. Restored from git.
  Scratch crates must be created outside the worktree.
- **Two of the orchestrator's own measurements were vacuous before they were
  right**, and both were caught only by re-running: a background suite piped
  through `tail -30` reported 16 tests instead of 502, and a bare `cargo test`
  compiled out the `#[cfg(feature = "schema")]` drift test, making a working
  gate look absent. The first drift mutation also failed on a compile error
  rather than on drift, which proves nothing. *The rule that a green result is
  unproven until the harness has been shown able to fail applies to the harness
  itself.*
- **MemPalace was unreachable for this whole cycle** (`Unable to connect`), so
  the session diary fell back to disk outside the repo. `/vwf:handoff` will hit
  the same wall.

### Second fix round — both reviewer reports arrived after the first closed

They were written as plain text rather than sent, so neither reached the
orchestrator until after the cycle had been called done. **The method assumes
reports arrive; twice this cycle they did not.** Four more defects came out of
them, all fixed.

**The description guard was count-only, and criterion 3 was unheld.**
`the_schema_carries_every_description_it_was_written_with` compared
`described.len()` to `39` and checked none was empty. Replacing **all 39**
strings with `"x"` was fully green — and the drift test cannot help, because
editing the `#[schemars(description = …)]` attributes and regenerating moves
both sides together. Now anchored by `DESCRIPTION_DIGEST`, an FNV-1a over every
`(pointer, text)` pair, inline rather than a dependency because `DefaultHasher`
is explicitly not stable across releases. Verified by mutation: gutting all 39
now goes red, on a plain `cargo test` rather than only under
`--features schema`.

**`gauge.width` had `minimum: 1` and no `maximum`.** `MAX_GAUGE_WIDTH = 1000` is
enforced at `config/mod.rs:646` and this cycle's own test asserts `9999 → 1000`,
so the schema promised a width the renderer will not honour. The doc justifying
`minimum: 1` demanded the ceiling by the identical argument. Both ends now
present, matching every other bounded number in the file.

**Two comments this cycle wrote were already false.** `tests/schema.rs` said all
39 descriptions came through "with the same string" — 38 did; `/$defs/color`
gained a sentence, deliberately. `write.rs` said a "**third** copy" of the
schema URL exists in the installer; there are several
(`assets/claude-status.defaults.json` and its `npm/` mirror, both *shipped*,
plus `cli.rs`, `.config/claude-status.json`, `readme.md`). **That is three
consecutive cycles shipping a comment that claims coverage it lacks** — 02 at
C9, 03 at C9, and twice here.

**`Finding::is_warning` has no production caller.** `app.rs` renders through
`line()` and never classifies. Kept, because its one unit-test caller asserts
classification without matching a glyph, but the doc no longer claims a
production rationale it does not have.

**The five new files were untracked, and nothing scanned them.**
`code:precommit`'s changed-files branch feeds `git diff --name-only HEAD`, which
excludes untracked files — so the green precommit run inspected **none** of
`code/schema`, `src/bin/schema.rs`, `config/schema.rs`, `config/validate.rs` or
`tests/schema.rs`. They are now `git add -N`. The same gap bit the orchestrator
directly: a `git checkout --` meant to revert a mutation restored the schema
from `fca2aa2` and silently discarded the regenerated file.
`mise run
code:schema` rebuilt it byte-identically, which is the generator
earning its keep.

### Reported, not fixed — three unreported degradations

None is a regression; all three behave identically at `fca2aa2`. They are the
shape of gap this cycle's feature was positioned to close and does not.

1. **A non-array `lines` blanks the bar and `--debug` says nothing.**
   `config/mod.rs:742-745` returns `Vec::new()`, so `{"lines": "not-an-array"}`
   renders **0 bytes** with exit 0 and no finding under the user layer. This is
   the one shape that violates §1 invariant 3's *degrade, never blank*, and the
   only visible degradation the validator is wholly silent about. **The most
   worthwhile follow-up in this list.**
2. **A wrong-typed closed block degrades visibly and unreported.**
   `{"powerline": "nope"}` draws a bar with no separators — `block()`
   substitutes `unstyled()` — and `note_coercion` has no arm for it. Documented
   as deliberate, but the stated reason ("the deserializers already degrade it")
   is equally true of `gauge.width: 0`, which *is* reported.
3. **`caps.*` out-of-range coercion is unreported.**
   `{"caps": {"context": 2000}}` silently becomes 65; a sibling typo in the same
   block *is* reported. Same shape as criterion 5, different block.

**A correction to this plan's own risk framing.** Recon predicted a typo inside
`powerline` would blank the block through `block()`'s `unwrap_or_else`. It does
not: the `deny_unknown_fields` this cycle adds is in the **schemars** namespace,
so serde still ignores unknown keys and the fallback is never reached. The bar
is byte-identical to clean at `fca2aa2` and now. The orchestrator repeated the
prediction as fact before it was tested; the behaviour reviewer disproved it by
running.
