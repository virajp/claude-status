---
type: vwf-plan
title: retire-the-spec — 2026-08-23
description: Cycle plan (a diff) retiring docs/spec/statusline-behaviour.md by
  rehoming the three things only it carries — the decision record, the
  ai-plugins contract and the reference payloads — and deleting the rest.
status: active
covers: [
  docs/decisions.md,
  docs/usage-mirror-contract.md,
  tests/fixtures/README.md,
]
requires: [
  docs/plans/2026-08-23-config-and-cli/01-typed-config.md,
  docs/plans/2026-08-23-config-and-cli/02-config-relocation.md,
  docs/plans/2026-08-23-config-and-cli/03-cli-surface.md,
  docs/plans/2026-08-23-config-and-cli/04-schema-and-validation.md,
  docs/plans/2026-08-23-distribution/01-drop-npm.md,
  docs/plans/2026-08-23-distribution/02-homebrew-formula.md,
  docs/plans/2026-08-23-distribution/03-release.md,
  docs/plans/2026-08-23-website/01-site.md,
  docs/plans/2026-08-23-website/02-config-generator.md,
]
timestamp: 2026-08-23T15:00:00Z
tags: [ docs, spec, retirement, decisions, contract ]
---

# Plan: retire-the-spec — 2026-08-23

## Slice

The contract document itself. `docs/spec/statusline-behaviour.md` is deleted,
and the three things only it carries are rehomed first.

**This cycle changes no behaviour and touches no `src/`.** It is the last plan
in the tree because every other plan writes to this file.

## Current state (actual)

`docs/spec/statusline-behaviour.md` is 1333 lines across fourteen sections,
maintained by **nine strike-through amendment blocks** rather than rewrites.

**It has stopped being reliable.** The audit in `docs/spec/DRIFT-2026-08-23.md`
— deleted with the spec in step 6, and in git history — validated every section
against the code and found five claims that are actively wrong — a fixture that
does not run (§12), a precedence order that contradicts the config order it
claims to print (§3), a data-preservation rule the code inverts (§3), a
test-isolation recipe that isolates nothing (§10), and a keychain kill-switch
superseded by `46ab142` and never updated (§7). It also found two code defects,
fixed separately.

**One failure mode produced nearly all of it.** An amendment updates the section
it is thinking about and never sweeps for text it falsified. `8f4efe0` added a
whole CLI surface and a whole config block, amended §9, and left §1, §3, §4a and
§5 describing a binary with one fewer surface than it has. The 2026-08-20 spend
amendment at `:884-886` contradicts `:861` fourteen lines above it.

**This is recurrent, not a backlog.** The archived `subagent-panel` cycle
already found and fixed a stale §12 fixture
(`docs/plans/archived/2026-08-19-1401-subagent-panel.md:218`); §12's other
fixture is now broken the same way.

**The code already routes around it.** Three comments cite the contract to
record that it is behind: `schedule.rs:24-29` ("the contract gives only the
conclusion"), `subagent.rs:91` ("the contract mentions neither the env var nor
the default"), `defaults_integrity.rs:175-179` ("is not the precedence order
contract §3 prints"). Where the two disagreed, the audit found the code comment
right every time.

**What actually holds the behaviour today** is 380 tests — 83 integration across
six files, 290 unit in `src/`, 57 in `installer/` — plus 8 exact-ANSI goldens.

**All nine other plans reference it**, 48 times across seven sections, in two
distinct ways that this plan has to treat differently:

| Kind                                                               | Example                                                                                                                                          | What retirement must do                                   |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------- |
| **Write-back** — a Docs step whose content is "§N records X"       | `01-typed-config:169`, `02-config-relocation:142-144`, `03-cli-surface:151`, `04:143`, `dist/01:119`, `dist/02:124`                              | Nothing. Each has already run by the time this plan does. |
| **Read-as-authority** — a decision derived from what the spec says | `dist/02:43` ("§5 guarantees `--version` prints nothing but the version" → the formula's `test do`), `02-config-relocation:186`, `website/02:58` | The claim must survive somewhere citable.                 |

## Target state (per contract)

`docs/spec/statusline-behaviour.md` does not exist. Nothing links to it.

Its content has been split by what each part is *for*:

- **The user-facing behaviour** — what the bar shows, how to wire it, how to
  configure it — lives on the site, which
  [website/01](../2026-08-23-website/01-site.md) already specifies as "what the
  bar shows, how to wire it, how to configure it, and how to write a repo-level
  config". That is the spec's §1–§7 restated for the people who need it.
- **The decision record** — why npm, why not an async runtime, why Zola, the
  Linux deferral, the six-targets reversal — lives in `docs/decisions.md`. This
  is the half with no other home: it is reasoning, not behaviour, and no test
  can hold it.
- **The `ai-plugins` contract (§8)** lives in `docs/usage-mirror-contract.md`,
  because it governs a consumer in another repository and cannot be verified by
  this repo's tests alone.
- **The reference payloads (§12)** live as real files under `tests/fixtures/`,
  where they can be executed instead of transcribed.

## Delta — ordered steps

### 1. Prove the site covers the user-facing half

Walk §1–§7 against the shipped site and record, per section, the page that
covers it. Anything uncovered is either added to the site in this cycle or
recorded as a **Gaps** entry — never dropped silently.

**This is the gate for the whole plan.** If the site does not cover it, the spec
is not redundant yet and the deletion in step 6 does not happen.

### 2. Harvest the decision record into `docs/decisions.md`

One entry per decision, each carrying **the reasoning and the date**, not just
the outcome. The nine amendment blocks are the source; a strike-through is a
decision that reversed, and both halves are kept — the audit's whole argument is
that the record of *why* a choice changed is the part with no substitute.

At minimum: the channel decision and its npm→Homebrew reversal; the six→two→one
target narrowing; the Linux deferral and the credentials-file blocker; async
runtime rejected; JSON over YAML; the caps-hook ownership reversal; the escape
filter; `--uninstall` not restoring a migrated file.

**Transcribe, do not summarise.** A decision compressed to its conclusion is
exactly the failure this plan is fixing.

### 3. Rehome §8 as `docs/usage-mirror-contract.md`

Lift it as-is, then correct it against the code — the audit found the shipped
layout already diverges from what §8 documents:

- `ctxSize` is inserted **only when present**, while the six other value fields
  are written as `null` (`usage.rs:75-77`).
- `resets_at` is mirrored **raw**, so an ISO-8601 string stays a string
  (`usage.rs:64-66`).
- `context-caps.js` writes `<session>.state.json` into the same directory, which
  §8 never mentions and `usage.rs:46-49` guards against colliding with.

Link it from `src/modules/usage.rs` so the next person to touch the writer finds
it.

### 4. Move §12's payloads into `tests/fixtures/`

One file per payload. **Fix the broken invocation as they move** — §12's
main-bar example pipes to bare `claude-status`, which resolves to
`Mode::MissingFlag` and prints an error rather than a bar; it needs
`--statusline`.

[website/02](../2026-08-23-website/02-config-generator.md) consumes these as the
JS preview's measurement set, so it reads the files rather than transcribing
from prose.

### 5. Re-point every reference

`covers:` in all ten plan docs, `docs/plans/index.md`'s "each plan is a diff
against the behaviour contract" framing, `CLAUDE.md`, `CONTRIBUTING.md`,
`readme.md`, and the three code comments that cite contract sections by number
(`schedule.rs:24-29`, `subagent.rs:91`, `defaults_integrity.rs:175-179`) — those
become citations of the test that pins the behaviour, since that is what
actually holds it.

A `grep -rn 'statusline-behaviour'` returning nothing outside
`docs/plans/archived/` is the check.

### 6. Delete `docs/spec/statusline-behaviour.md`

And `docs/spec/DRIFT-2026-08-23.md` with it — the audit is scaffolding for this
cycle, and git history holds both.

### 7. Record the replacement discipline

A short section in `CLAUDE.md`: behaviour is pinned by tests and code comments,
decisions are recorded in `docs/decisions.md`, the user-facing description is
the site. **No document restates behaviour that a test already holds** — that
restatement is what drifted.

## Acceptance criteria (from contract)

1. Given the repo after this cycle, when `grep -rn 'statusline-behaviour' .`
   runs excluding `docs/plans/archived/` and `.git/`, then there are no matches.
2. Given `docs/decisions.md`, then it carries an entry for every one of the nine
   amendment blocks, each with its date and its reasoning, and both halves of
   every reversal.
3. Given `docs/usage-mirror-contract.md`, then it documents the `ctxSize`
   asymmetry, the raw `resets_at`, and the `<session>.state.json` neighbour —
   the three things §8 omitted.
4. Given `tests/fixtures/`, when each payload file is piped to the binary with
   the invocation the file documents, then each produces the output it claims —
   in particular the main-bar payload renders a bar, not the missing-flag error.
5. Given the site, then §1–§7's user-facing content is reachable from it, per
   the step 1 mapping.
6. Given `mise run code:all`, then it passes — no test referenced the spec.
7. Given `docs/plans/index.md`, then no plan's `covers:` names a file that does
   not exist.

## Risks / drift

**Deleting too early is the real risk, and step 1 is the guard.** If the site
has not shipped, or has shipped thinner than
[website/01](../2026-08-23-website/01-site.md) specifies, this plan deletes the
only description of the product. The step 1 mapping is a gate, not a formality —
it fails the cycle rather than producing a Gaps entry.

**The decision record is the part that cannot be reconstructed.** Behaviour is
recoverable from tests and code; reasoning is not. If step 2 is rushed, the
deletion in step 6 destroys it. This is why step 2 says transcribe rather than
summarise, and why it precedes the delete by four steps.

**§8 is a live contract with a repository this cycle cannot test.** The audit
found it already documents a layout the code does not write. Rehoming it is an
opportunity to correct that, but a correction made against `usage.rs` alone is
still unverified on the consumer side — `context-caps.js` lives in `ai-plugins`.
Note it as unverified rather than implying it is agreed.

**Read-as-authority references outlive the file.** `dist/02`'s formula test
asserts a `--version` guarantee that §5 made. After deletion the guarantee is
held by whatever test pins it; if no test pins it, that is a Gaps entry and a
test to write, not a sentence to relocate.

**Archived plans keep their references, deliberately.** They are a record of
what was true when they ran, and rewriting them would falsify that. Criterion 1
excludes `docs/plans/archived/` for this reason.

## Out of scope for this cycle

- **Any change to `src/`.** The two code defects the audit found
  (`usage.rs:29-34`, `app.rs:436`) are fixed separately and are not gated on
  this plan.
- **Writing the site.** [website/01](../2026-08-23-website/01-site.md) owns it;
  this cycle only verifies coverage and fills gaps the mapping exposes.
- **Correcting the spec's other drift.** Tiers 3 and 4 of the audit are not
  fixed — the sections carrying them are rewritten by plans 1–4 and dist/01–02,
  and whatever survives is deleted here. Fixing prose on its way to deletion is
  work with no reader.
- **Retiring `docs/plans/index.md` or the plan tree.** Unrelated.

## Step 1 — the coverage gate, walked 2026-08-26

**Verdict: the gate PASSES, with one correction to the plan's own framing.**

The plan assumes §1–§7 are user-facing and therefore belong on the site. Walking
them, that is true of most but not all. Three parts are **not** user-facing, are
correctly absent from the site, and would have been dropped silently if "is it
on the site?" had been the only question asked.

| Section            | Covered by                                                       | Verdict                                                                                                                                                                                            |
| ------------------ | ---------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| §1 What it is      | `_index.md` (landing), `segments.md`                             | **Covered.**                                                                                                                                                                                       |
| §1 Five invariants | — nothing on the site, correctly                                 | **Not site material.** Implementation constraints, addressed to whoever writes the binary. Behaviour is held by tests; the *reasoning* goes to `docs/decisions.md`.                                |
| §2 Input contracts | — the site names "the payload", never its shape                  | **Not site material.** Nobody authors this payload; Claude Code sends it. The shape belongs in `tests/fixtures/` (step 4), executable; "every field is optional, parse defensively" is a decision. |
| §3 Configuration   | `configure.md`, `repo-config.md`, `generate.md`                  | **Covered**, and more usably — the generator builds a config from the live schema.                                                                                                                 |
| §4 Rendering model | `segments.md` (catalogue), `configure.md` (powerline, colours)   | **Covered.**                                                                                                                                                                                       |
| §4a Escapes        | — nothing on the site                                            | **Not site material.** A security property a reader benefits from and never acts on. Held by `_shared::text` and its tests; the reasoning goes to `docs/decisions.md`.                             |
| §5 CLI surface     | `install.md` (the surfaces table), `diagnosing.md`, and `--help` | **Covered twice.** `--help` is pinned by `cli.rs`'s own tests.                                                                                                                                     |
| §6 Git resolution  | `segments.md` (branch, worktree, dirty markers)                  | **Covered.**                                                                                                                                                                                       |
| §7 Spend           | `segments.md` (the four gates), `configure.md`, `diagnosing.md`  | **Covered.**                                                                                                                                                                                       |

**So the deletion is not blocked**, but step 2's scope is wider than the plan
states: it must also carry the five invariants, §2's defensive-parsing rule and
§4a's escape rule, none of which are decisions in the "why npm" sense and none
of which the site should hold. They are cross-cutting engineering constraints
whose *reasoning* has no other home — which is the same argument the plan makes
for the decision record, applied to three things it did not list.

Recorded here rather than acted on silently, because a gate that quietly widens
the next step is how a plan stops describing what happened.

## Step 2 — the decision harvest, walked 2026-08-26

`docs/decisions.md` is written. Every dated amendment block in the spec was
walked in order rather than recalled, and each is either transcribed as an entry
or listed under **§14 Amendments that carried no decision** with where its
behaviour now lives — so nothing was dropped silently.

**The step-1 widening is discharged.** The five invariants (§1), §2's "every
field is optional, parse defensively", and §4a's escape rule are all carried, as
§2, §3 and §6 of the new file.

**The two 2026-08-26 amendments are carried**: the `project` segment's fallback
to the git root's directory name, and `typeSymbols._default` moving from U+F544
to U+F1B2.

Four things the plan's own minimum list got slightly wrong, recorded because a
step that quietly re-scopes itself is the failure this cycle exists to fix:

| Plan says                                             | What the spec actually carries                                                                                                                                                                                                                                                           |
| ----------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| "JSON over YAML"                                      | **No such standalone decision exists.** The nearest is the `caps` cycle deleting the `<cwd>/.config/vwf.yaml` line-scrape rather than keeping it as a second source. Transcribed there, under *Caps become config* — not invented as a config-format decision.                           |
| "the Linux deferral and the credentials-file blocker" | The credentials file is what **unblocks** Linux, not what blocks it: `from_keychain()` guards on `cfg!(target_os = "macos")` and falls back to it, so credentials degrade rather than break. Linux was surveyed, came back **viable**, and was declined on cost. Transcribed as written. |
| "the nine amendment blocks"                           | There are **thirty-six** dated amendment, correction, reversal and supersession markers as of today; the nine were the count when the plan was written. All are accounted for.                                                                                                           |
| §12's payloads are step 4's                           | Unchanged — but §2's payload *shape* is the executable half of what step 1 sent to step 2. Only the "parse defensively" **rule** is a decision, and only it was transcribed. The shapes stay for step 4.                                                                                 |

Nothing was deleted. Steps 3–7 are untouched.

## Step 3 — §8 rehomed, walked 2026-08-27

`docs/usage-mirror-contract.md` is written, and `src/modules/usage.rs` links it
from its module doc in place of the `contract §8` citation it carried.

**All three audit findings are documented**, each as its own section under *What
the old contract omitted*: the `ctxSize` absent-vs-`null` asymmetry, the raw
`resets_at`, and the `<session>.state.json` neighbour.

Four things the walk found that the plan's step-3 text does not say:

| Plan says                                         | What the code actually shows                                                                                                                                                                                 |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| "`context-caps.js` writes `<session>.state.json`" | **`--caps-hook` writes it too**, at `app.rs:163`, and will be the only writer once the JS hook goes. The name is still not unilaterally ours while both exist. Corrected in the doc rather than transcribed. |
| "`usage.rs:46-49` guards against colliding"       | It is a **`debug_assert!`** (`usage.rs:52`), so it holds the shape in the test build and is absent from the released one. Recorded as what it is; a runtime guard would imply a defence that does not ship.  |
| the six `null` fields                             | Correct — but the writer's **own doc comment said "the four rate-limit fields"**, understating it by the two context fields the test at `usage.rs:179` already enumerates. Corrected in place, one line.     |
| "touches no `src/`"                               | This step is the exception the plan itself asks for. The `src/` change is **two doc comments and no code**; the suite is unchanged at 582.                                                                   |

**One thing recorded that §8 never claimed either way:** the in-repo reader uses
**five** of the nine keys (`caps/mod.rs:46-55`). `sessionId`, `ts`, `ctxUsed`
and `ctxSize` exist solely for the cross-repo consumer, so this repo's suite
proves they are still *emitted* and nothing about what they mean. Stated in the
doc, because assuming otherwise is how a cross-repo contract rots quietly.

**The consumer side is marked unverified throughout**, per the plan's risk note
— the corrections were made by reading this repository, and whether
`context-caps.js` copes with each shape is not decidable from here.

## Step 4 — the payloads made executable, walked 2026-08-27

`tests/fixtures/main-bar.json` and `tests/fixtures/subagent.json` exist, with
`tests/fixtures/README.md` documenting each one's invocation and what it
produces. `tests/e2e.rs` reads both with `include_str!`.

**There were three copies of this JSON, not two.** §12's shell examples, and a
`const` per payload inside `tests/e2e.rs`. Making the files real without
deleting the consts would have made four, so the consts now read the files —
which is what "executed instead of transcribed" has to mean if it means
anything.

**The broken invocation is fixed and pinned as a control.**
`the_reference_payload_without_its_flag_is_the_missing_flag_error_and_not_a_bar`
pipes the payload with no flag and asserts the one-line missing-flag error.
Without it the neighbouring test proves the *payload* renders but not that the
**flag** is what makes it — and the flag is the half §12 documented wrong.

**Both new guards were proved able to fail:**

| Guard                                                    | Control                                         | Result                                           |
| -------------------------------------------------------- | ----------------------------------------------- | ------------------------------------------------ |
| the missing-flag control                                 | pass `--statusline` instead of no flag          | FAILED as required, then passed on revert        |
| `every_reference_payload_is_named_in_the_fixture_readme` | drop an undocumented `.json` into the directory | FAILED naming the file, then passed once removed |

**The three documented invocations were also run by hand** against the built
binary, not only through the harness: the bar renders, the panel's row survives
`jq -r .content`, and the no-flag case prints the error.

**One claim in the new README was wrong when written and is corrected.** It said
the main-bar payload produces "the two-line powerline bar". It produces **one
line**: the payload's `current_dir` is `/tmp/demo`, which is not a git
repository, so `project`, `worktree` and `branch` all omit and an all-empty line
is dropped. Caught by running it rather than by reading it — the same way §12's
own error would have been caught at any point in the years it stood.

Test count is **584**, up two from 582.

**On website/02's consumption:** the plan says it reads these payloads as the JS
preview's measurement set. Checked — website/02 measures the preview against
`tests/golden/*.txt`, not against the payload files, and the site transcribes no
payload of its own. Nothing to re-point; no gap.

## Step 5 — references re-pointed, walked 2026-08-27

**The step's own inventory was short by an order of magnitude.** It names "the
three code comments that cite contract sections by number". There were
**twenty-eight** citations across `src/` and `tests/`, and two references the
step does not mention at all — one of them user-facing.

| Found                                      | Where                                                                                                    | Now                                                                                                    |
| ------------------------------------------ | -------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| **14 `covers:` entries**, not ten          | nine plan docs **and four folder `index.md`s** and this plan                                             | `docs/decisions.md`; this plan and its index name the three documents they produced                    |
| **a live link on the shipped site**        | `site/content/_index.md` sent every reader to the contract on GitHub                                     | the decision record. It would have 404'd the moment step 6 ran, on the page users actually land on     |
| **a link in the issue forms**              | `.github/ISSUE_TEMPLATE/config.yml`, offered to anyone about to file a bug                               | the site's Segments page — the user-facing answer to "what is it meant to do"                          |
| **a fourth code comment**, not three       | `src/_shared/text.rs` carried a markdown link reference to the contract                                  | `docs/decisions.md`                                                                                    |
| **28 `contract §N` citations**             | `lib.rs`, `app.rs`, `_shared/{mod,proc,json,text}.rs`, `git.rs`, `config/*`, `render/*`, five test files | the fact stated plainly, the test that pins it, or `docs/decisions.md`                                 |
| **five markdown links to the drift audit** | which step 6 deletes together with the spec                                                              | unlinked. The *mention* is a record and stays; the *link* is a reference to a file that will not exist |

The three the step did name — `schedule.rs`, `subagent.rs`,
`defaults_integrity.rs` — each now cites the test that holds the behaviour, by
test name.

**`tests/site.rs` had a guard that would have failed on step 6 by design.**
`the_readme_and_the_behaviour_contract_name_the_same_site` asserted the contract
names the site, as the gate for this very cycle. It is now
`every_doc_that_sends_a_user_to_the_site_names_the_same_address`, covering the
readme and the issue-form config — the surviving pair, one of which this step
only just pointed at the site.

### Criterion 1 cannot be met as written, and the resolution is on the record

The criterion asks that `grep -rn 'statusline-behaviour'` return nothing outside
`docs/plans/archived/`. **A plan whose step 6 is "delete this file" has to name
the file**, and so does the provenance line of each document harvested out of
it. Read literally, the criterion deletes its own explanation — the same trap
`tests/e2e.rs` already documented for the `--refresh` rename, and it is resolved
the same way.

**The line drawn is between a reference and a record.** A reference points at
the file as a live source of truth and must go; a record names it as history and
stays. What that leaves:

- **Nothing outside `docs/` names it at all** — not `src/`, not `tests/`, not
  the site, not the workflows, not the issue forms.
- Inside `docs/`, only `docs/plans/` (the cycle that deletes it, plus landed
  plans recording what they were diffs against) and two provenance lines, in
  `decisions.md` and `usage-mirror-contract.md`, saying where their content came
  from.
- **No `covers:` names it**, which is criterion 7 and is met in full.

Suite unchanged at **584**; the site builds.

## Gaps surfaced during execution

1. **The commit-scope list has no home for the two documents this cycle
   creates.** `.config/git-conventional-commits.yaml` maps scopes to paths, and
   `docs/decisions.md` (step 2) and `docs/usage-mirror-contract.md` (step 3)
   fall under none of them — `spec` is `./docs/spec`, which step 6 deletes. Step
   2 committed under `spec` because the harvest's source is the spec, but that
   scope stops existing four steps later. **Step 5 or step 7 has to decide what
   replaces it**, and doing nothing leaves a `docs` scope that the hook rejects.
   Not fixed here: editing the convention config is not step 2's.
