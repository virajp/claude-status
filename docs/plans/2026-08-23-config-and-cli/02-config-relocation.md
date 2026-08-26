---
type: vwf-plan
title: config-relocation — 2026-08-23
description: Cycle plan (a diff) moving the user config into
  ~/.config/claude-status/, storing only non-defaults, narrowing the repo layer
  to projectName, and deleting the render path's ability to write files.
status: done
covers: [
  docs/decisions.md,
]
requires: [
  docs/plans/2026-08-23-config-and-cli/01-typed-config.md,
]
timestamp: 2026-08-23T14:02:00Z
tags: [ config, xdg, paths, autoseed, defaults ]
---

# Plan: config-relocation — 2026-08-23

## Slice

Contract §2 and §3. Four changes to where config lives and what it contains:

1. The user config moves to `~/.config/claude-status/config.json`.
2. A written config contains **only non-defaults**.
3. The repo layer is narrowed to `projectName` alone, and is never generated.
4. The render path loses its ability to write files at all.

## Current state (actual)

**The user config is a bare file**, `~/.config/claude-status.json`
(`layers.rs:20`, `CONFIG_FILE_NAME`). The comment beside it notes it is *not*
the JS bar's `statusline.json`, and that `--install` migrates the old file.

**Home is resolved from `$HOME` and nothing else** (`paths.rs:15`), deliberately
not a platform config directory, because `dirs::config_dir()` on macOS gives
`~/Library/Application Support`.

**The spend cache is already correctly placed** at
`~/.cache/claude-status/spend.json` (`spend/cache.rs:94`), with a
`CLAUDE_STATUS_SPEND_CACHE` override used by the tests.

**The installer seeds the defaults byte-for-byte.**
`installer/src/modules/config.ts` writes `assets/claude-status.defaults.json`
verbatim as the user's config. So every install freezes a full copy of the
shipped defaults.

**A render creates the repo config.** `src/modules/config/autoseed.rs` writes
`<repo-root>/.config/claude-status.json` from the render path, gated on
`Config::auto_configure_repo` — an opt-**out** key whose shipped value is
`true`. It seeds `$schema` and `projectName`, and it also *migrates* a
`statusline.json` it finds, rewriting the `$schema` URL.

**The repo layer is a full config layer.** It participates in the same deep
merge as the user layer and can override anything.

**There is nothing to migrate.** `claude-status` has never been released. No
user has a config at any of these paths.

## Target state (per contract)

| Thing              | Path                                     |
| ------------------ | ---------------------------------------- |
| User config        | `~/.config/claude-status/config.json`    |
| Repo config        | `<repo-root>/.config/claude-status.json` |
| Spend cache & lock | `~/.cache/claude-status/`                |

The user config holds only keys whose value differs from the binary's default,
so an unset key follows the binary forward. The repo config holds `projectName`
and nothing else, is written by a human, and is documented in `--help` and on
the website.

**A render reads. It never writes.** With no config file anywhere, the bar
renders from the embedded defaults, and that is a supported, tested state rather
than a degraded one.

## Delta — ordered steps

### 1. Move the user config into a directory

`~/.config/claude-status/config.json`. A directory rather than a bare file
because the tool will accumulate more than one thing to store, and a directory
is also one thing to delete.

**No fallback to the old path.** Nothing was ever released, so a fallback would
be compatibility with a state that never existed.

### 2. Keep the cache where it is, and say why

`~/.cache/claude-status/` is already correct and does not move. Record the
reason in §3 so it is not "tidied" into the config directory later: a spend
figure derived from an account token is machine-local and regenerable, and a
config directory is the thing people commit to a dotfiles repo and sync between
machines.

### 3. Teach the config to serialise only its non-defaults

A `Config` can already be compared with `Config::default()` after
[plan 1](./01-typed-config.md). Serialisation walks the tree and emits a key
only where the two differ.

**`#[serde(skip_serializing_if)]` is not sufficient on its own** for the open
maps: `palette` and `segments` are `BTreeMap`s whose *defaults are non-empty*,
so "skip if empty" would emit the whole shipped palette the moment a user
changes one colour. A map is diffed entry by entry.

The output always carries `$schema`, which is not a default — it is a pointer
that makes the file editable.

### 4. Narrow the repo layer to `projectName`

The repo layer deserializes into a one-field type. Any other key present is
**ignored, and reported by `--debug`** — not merged, and not an error, because
§3's never-fail rule still holds.

This is a deliberate narrowing of §2's three-layer merge. The repo layer existed
to name the project; letting it override styling made every repo a place where
the bar could look different for reasons nobody could find.

### 5. Delete `autoseed.rs` and `autoConfigureRepo`

Both go. The module, the config key, the schema entry, and the tests.

**The invariant this buys is worth naming**: a status line that redraws every
four seconds now provably touches nothing on disk during a render. That is
easier to reason about than any amount of care about *when* it writes.

### 6. Delete the `statusline.json` migration

`autoseed.rs`'s migration arm and the installer's. The JS bar's config is
another tool's file, and with nothing released there is no user holding one that
this binary was ever going to read.

### 7. Make zero-config a tested state, not an accident

`layers::load(None, None)` already returns the embedded defaults, and a golden
covers it. Extend that to the real paths: with `$HOME` pointing at an empty
directory and no git root, a full bar renders and nothing is created.

### 8. Docs

§2 records the three-layer merge with the repo layer restricted to
`projectName`. §3 records the new paths, the config/cache split and its reason,
the non-defaults-only rule, and that a render never writes. §9's `--install`
description is left alone —
[distribution/01](../2026-08-23-distribution/01-drop-npm.md) owns it.

## Acceptance criteria (from contract)

1. Given `$HOME` pointing at an empty directory and no git root, when the bar
   renders, then it renders in full and **nothing is created anywhere** under
   that `$HOME`.
2. Given a config differing from the defaults in exactly one key, when it is
   serialised, then the output holds `$schema` and that one key.
3. Given a config that changes one entry of `palette`, when it is serialised,
   then only that entry is emitted, not the whole shipped palette.
4. Given a user config at `~/.config/claude-status/config.json`, when the bar
   renders, then it is applied; given one at the old
   `~/.config/claude-status.json`, then it is **ignored**.
5. Given a repo config carrying `projectName` and `gauge`, when the bar renders,
   then the name applies and the gauge does not, and `--debug` reports the
   ignored key.
6. Given any render at all, when the process is traced, then it opens no file
   for writing outside `~/.cache/claude-status/`.
7. Given the repo after this cycle, when it is searched, then `autoseed.rs`,
   `autoConfigureRepo` and every `statusline.json` reference are gone.
8. Given `tests/golden/`, when the suite runs, then every golden matches without
   regeneration.

## Risks / drift

**Non-defaults-only serialisation is only as good as plan 1's `Default`.** If
`Config::default()` disagrees with `DEFAULTS_JSON` anywhere, this cycle writes a
file that either omits a key the user set or emits one they did not. Plan 1's
criterion 2 is the guard; if it was weakened to pass, this cycle inherits the
weakness and criterion 2 here will not catch it.

**The open-map diff is the subtle part.** Emitting a whole `palette` because one
colour changed would defeat the purpose entirely and would look like it worked —
the file is valid, the bar renders, and the user has silently frozen every other
colour. Criterion 3 exists for exactly that, and it is worth extending to
`segments` and `symbols` by hand.

**Narrowing the repo layer removes a capability.** Anyone using a repo layer for
styling loses it. There is nobody in that position — nothing has shipped — but
the *contract* said the layer could override anything, so §2 is being reduced,
not clarified. It is recorded as a reversal rather than a tidy-up.

**Deleting autoseed removes the only thing that created the repo layer.** After
this cycle a repo config exists only if a human writes one, which means
discoverability moves entirely to `--help` ([plan 3](./03-cli-surface.md)) and
the website. If both are vague, the feature is effectively gone — the file being
supported is not the same as anyone knowing it exists.

**Criterion 6 is worth actually running, not reasoning about.** "A render writes
nothing" is easy to believe and easy to be wrong about — a cache touch, a lock
file, a log. Trace it.

## Out of scope for this cycle

- **`--configure`, `--refresh`, `--help` and `--debug`'s no-config report.**
  [Plan 3](./03-cli-surface.md). This cycle creates the zero-config state; plan
  3 makes the CLI speak about it.
- **The schema.** [Plan 4](./04-schema-and-validation.md) regenerates it; this
  cycle edits it by hand only where a key was deleted.
- **Deleting the installer.**
  [distribution/01](../2026-08-23-distribution/01-drop-npm.md).
- **The usage mirror and `$AI_PLUGINS_USAGE_DIR`.** §8 is a contract with
  another repo and is explicitly unchanged.

## Gaps surfaced during execution

Executed 2026-08-23. 410 → 429 tests. All eight criteria addressed, but **three
of them cannot be met as written** — see B. Fifteen gaps, none blocking the
merge; most are in **this plan** rather than the contract.

The method is what found them. Recon ran before any code, and a differential run
against the pre-cycle binary was the gate, per [plan 1](./01-typed-config.md)'s
closing recommendation. Both earned their cost: recon found eight plan defects
before a line was written, and the differential proved the forgiveness rule
byte-for-byte across nine malformed configs — the exact class plan 1 shipped a
regression in.

### A. Defects in this plan's own text

**A1. Steps 1 and 4 collide in the same six lines.** `layers.rs:55-56` built the
user *and* repo paths from one expression inside one loop that merged both
layers identically. Step 1 moves the user path while the repo path stays; step 4
gives the repo layer a different deserialization. The plan presents them as
independent steps; they are one edit. Anyone planning them in separate cycles
would have had to undo the first.

**A2. The open-map inventory is short by four.** Step 3 names `palette` and
`segments`. There are five top-level open maps — adding `symbols`, `typeSymbols`
and `subagent.statuses` — plus the nested `SegmentStyle::Styled(Map)`. The
schema agrees with five. This is the same defect as plan 1's gap 3, one cycle
later, and it matters more here: an un-diffed map is silently frozen at today's
values.

**A3. There is no serialiser to extend.** Step 3 reads as an amendment
("serialisation walks the tree") to something that does not exist — `Config`
derived `Deserialize` only. Eight types needed `Serialize`, three of them
hand-written (`Caps`, `SegmentEntry`, `SpendConfig`), and `$schema` has no field
on `Config` at all, so it must be re-inserted after serialisation. The one
config-shaped writer, `autoseed.rs:82`, hand-built a `Map` and was deleted by
step 5. Net-new construction, budgeted as an edit.

**A4. Criterion 5 requires restructuring `layers::load`, which no step says.**
`load` deep-merged all three layers into one `Value` before `Config::new`, so
the repo layer's key identity was gone before anything could report it.
Reporting ignored keys meant inspecting the repo layer pre-merge and adding a
fourth field to `LayerSource`. Nor did `--debug` have any ignored-key channel to
extend.

**A5. The §-numbering is wrong throughout, and it propagated into the code.**
Config is contract **§3**; §2 is *Input contracts*. The plan says §2 at lines
22, 116 and 143, and "§3's never-fail rule" is really §1 invariant 3. The
implementation copied the error into four doc comments before it was caught.
**The plan text is still wrong and is left as-is** — this entry is the record.

**A6. Step 5's deletion inventory is short by five**, and one entry is dangerous
— see C1. The plan names the module, key, schema entry and tests; it omits
`assets/claude-status.defaults.json:3`, `app.rs:69-85` + its call site,
`tests/defaults_integrity.rs` (both the dotted-helper assertion and the
schema↔asset parity test, which goes red if only one side is edited),
`readme.md:145-150`, and the installer's `topUp()`, which re-adds
`autoConfigureRepo` by variable key with no literal in the file.

**A7. Repo caps are deleted, not narrowed — a second reversal the plan never
names.** The plan says the repo layer "existed to name the project".
`caps/config.rs:1-14` says otherwise: it records repo-caps-win-outright as a
**knowingly-taken** tradeoff. Narrowing un-takes it. Recorded in §3 as a
numbered pair with styling, caps named the larger — styling lost a capability
nobody used; caps reverses an argument made on the record. It is also the one
reversal that is **security-positive**: verified through `--caps-hook`, a cloned
repo setting `caps.context: 99` previously suppressed the breach directive
entirely, and now cannot.

### B. Acceptance criteria that cannot be met as written

**B1. Criterion 6 is false today, for reasons this cycle does not change.** "A
render opens no file for writing outside `~/.cache/claude-status/`" ignores the
usage mirror (`app.rs:219` → `usage.rs:60`) and the caps hook's state file
(`app.rs:169`), both of which write on the render path and are both **explicitly
out of scope** — §8 is a live contract with `ai-plugins`. The criterion needs
`$CLAUDE_STATUS_USAGE_DIR` as a second carve-out. Tested with the variable
*set*, and with an assertion that the mirror actually fired, so the test cannot
pass by never exercising the one path that writes.

**B2. Criteria 1 and 6 contradict each other by the spend cache.** An empty
`$HOME` is precisely what makes a render spawn the detached refresh child, which
creates `~/.cache/claude-status/`. So "nothing is created anywhere under that
`$HOME`" is false, while criterion 6 explicitly permits that directory. **Worse,
the naive test for criterion 1 passes by luck** — the parent exits before the
child writes, so a snapshot taken immediately sees a clean tree. Fixed with a
bounded poll on the child's lock. *Any future cycle asserting "nothing was
written" must wait for that child.*

**B3. Criterion 7 is unsatisfiable as literally written, and is unmet for
`installer/`.** "Every `statusline.json` reference is gone" would wrongly delete
`~/.config/ai-plugins/receipts/statusline.json` (the ai-plugins install receipt)
and `~/.claude/scripts/statusline` (the JS bar script) — different files owned
by other tools. Read as qualified to the config layer, the Rust half is done and
the installer half is not: step 6 says to delete the installer's migration arm
while "Out of scope" defers the installer to
[distribution/01](../2026-08-23-distribution/01-drop-npm.md). The two pull
opposite ways and the work is unowned. Left in place deliberately;
**`configure.ts:113-116` is now actively wrong** rather than merely undeleted —
it carries every key into a repo-level file "so the theming survives", which
after the narrowing it does not.

**B4. Criterion 8 is vacuous.** No golden reads a config file — every one goes
through `layers::load(None, None)` or `Config::new(...)`. All 8 passing proves
nothing about steps 1–4. The asset edit is the only thing linking this cycle to
the goldens, and C1 is that link's real risk.

### C. What execution found in the code

**C1. The defaults asset cannot be edited through an editor buffer, and step 5
requires editing it.** `assets/claude-status.defaults.json` is `-text -diff`
(`.gitattributes:5`), excluded from dprint, and `defaults.rs:8-13` says
"**never** edit it through an editor buffer… verify by rendering, never by
reading a diff." 28 of its values are Nerd Font private-use codepoints that a
normal write destroys silently — and because the file is `-diff`, **reading the
diff will not show you.** Removed by byte splice, asserting the reconstruction
identity before writing; verified independently by `cmp` against the pre-cycle
file with that one line filtered out. This, not the goldens, is criterion 8's
real risk.

**C2. Comparing against the raw asset instead of `Config::default()` would emit
every `f64` field into every config, for every user, having changed nothing.**
`json!(15) == json!(15.0)` is **false** — `serde_json::Number` compares its
internal variant, so the asset's integer `15` and the default `15.0` are unequal
*as written*. The diff is safe only because both sides pass through `to_value`,
which makes them both `f64` before comparison. The two approaches look
equivalent and are not. Pinned by a test that also asserts the representations
genuinely differ, so it cannot rot into a no-op.

**C3. Four security tests were silently defanged by the narrowing.** Three e2e
`--debug` tests and one in `powerline.rs` planted repo-layer `lines`,
`spend.show` and `symbols` and asserted they were sanitised. Once the repo layer
ignores those keys, all four pass having exercised **nothing** — the exact
failure their own comments warn about. Re-pointed at inputs that still reach the
sanitiser, and verified by mutation: replacing `sanitize()` with `to_string()`
now fails all four, where before the repair it failed none. A removed attack
surface also gained its own test, so a future widening goes red.

**C4. A degraded block does not survive a write→read round trip.** A malformed
`subagent` (`5`, `"x"`, `null`, `[]`) degrades to an unstyled state that is only
*partly* expressible: the segments come back, `statuses: {}` does not, so the
panel's status colours silently return. This is **not** the documented `palette`
exemption, which emits nothing. Pinned rather than fixed — degradation is lossy
by design, mapping four inputs onto one state, and a writer that round-tripped
it would preserve the damage rather than the configuration. Latent while nothing
writes; `--configure` inherits a known boundary.

**C5. The `spend_debug` / `spend_render` fixtures had silently stopped applying
their config layer** — they seeded the old bare path and kept passing, i.e. they
were asserting against the embedded defaults. Repointed. **The weakness is
larger than the repair**: pointed back at the dead path, `spend_debug` still
passes 11/12 and `spend_render` 6/8, so 17 of those 20 tests would not notice if
their config layer vanished. Pre-existing, and the same weak-test class as plan
1's gap 6.

**C7. The no-writes guard was blind, and the blindness replaced an earlier lucky
pass.** B2's fix — a bounded poll on the child's lock — did not work, for two
independent reasons. The lock is `O_EXCL`-created and `Drop`-unlinked, so its
absence is the state **both before and after** the child: structurally incapable
of signalling completion, and the poll returned on its first iteration inside
the fork→lock window. Separately, the seven-mode matrix **never spawned a child
at all** — its fixture set `refreshMinutes: 0`, which `schedule::decide`
short-circuits to `Disabled` before it looks at the cache, so the wait was inert
in every iteration. Both tests carried comments claiming the coverage they
lacked.

*No green run could have shown this.* It was found by mutation — a child that
sleeps and writes into `$HOME` left the criterion-1 test **passing**, with the
file provably created. The repair watches the cache file's bytes (written on
every terminal path, never removed, so the change is monotonic), takes an
explicit expect-a-child flag rather than guessing, and **panics** if an expected
child never materialises, so a future change that stops spawning turns callers
red instead of vacuous. A row with non-zero `refreshMinutes` *and the seeded
cache deleted* was added — without the deletion the schedule calls it fresh and
still never forks, which is the second way this could have stayed inert.
Verified by matrix, not by green: the mutation now turns both tests red, and
neutering the wait as well turns criterion 1 green again — the control proving
the wait is load-bearing.

**C8. The criterion-1 test was reading the real macOS keychain.** An empty
`$HOME` has no `.claude/.credentials.json`, so `creds::load` falls through to
the keychain arm, which shells `security` — and the keychain is **not scoped by
`$HOME`**. The module's fourth neutralisation is unavailable by construction in
the one test whose whole premise is an empty home. Fixed by emptying `PATH` so
`security` cannot resolve, which is a harder guarantee than seeding a
credentials file and keeps `$HOME` genuinely empty. Introduced by this cycle and
unnoticed through three rounds, including while the child was being made to run.

**C9. Six doc comments were falsified and missed by the first correction pass**,
including two in the contract: §Security still named the repo config as the
hostile input where "cloning a hostile repo is the entire attack", and §The
shipped defaults still pointed at `~/.config/statusline.json`. `readme.md` still
promised per-repo styling and per-repo caps, and claimed no config file is
created — which is false of the shipping installer. **This is the exact drift
pattern the drift audit documents** — an amendment updates its own section and
never revisits the earlier text it falsified — very nearly repeated inside the
cycle that cites it. Amended in place rather than annotated further down, for
that reason.

**C6. The autoseed integration path was structurally suppressed.** Every e2e
`--statusline` run inherits the test process's cwd — this repo, which has a
`.config/claude-status.json` — so `autoseed::ensure` short-circuited in **all**
of them. Its deletion was invisible to the e2e suite; only the differential run
showed it, by leaving a 138-byte file in a scratch repo under the old binary and
nothing under the new one.

### Also noted, not acted on

- **The serialiser has no production caller.** `--configure` is
  [plan 3](./03-cli-surface.md); the installer is distribution/01. It is tested,
  not exercised. LTO strips it — the release binary is **16.5 KB smaller** than
  pre-cycle, so deleting autoseed more than paid for it.
- **Claim 2 is not delivered end to end.** The installer still seeds the full
  asset verbatim, so the freeze it causes outlives this cycle. The contract now
  says so explicitly rather than in the past tense, and the installer test that
  pins the old behaviour carries a note naming distribution/01 as its owner — it
  was otherwise a test that moved with the code and kept asserting the property
  the cycle set out to remove.
- **`~/.config/claude-status/` was already the installer's state directory**
  (`receipt.json`). No filename collision, but step 1's "a directory is one
  thing to delete" rationale would take the receipt with it. distribution/01
  deletes the installer anyway.
- **`spend_render` is wall-clock sensitive, pre-existing.** One unreproduced
  2-of-8 failure under CPU contention. `spend_render.rs:271` hard-asserts
  `render_took < 1s` and `:209` `< 3s` on detached-child timing — both present
  unchanged at `ae983b3`, and this cycle's diff to that file is the config path
  and nothing else. Did not reproduce in 6 further full runs plus 17 targeted.
- **`--debug`'s ignored row allows intra-line ambiguity.** A repo key named
  `x, y, z` is indistinguishable from three keys, since the row joins with
  `", "`. Line and section forgery are **not** possible — verified against
  newline, CR, NEL, C1 CSI, ESC and bidi overrides, plus a 300-char key and 3000
  keys. Cosmetic.
- **A repo layer can still blank the user's `projectName`** by setting it to a
  bad type. Pre-existing and identical in both binaries — inherent to that layer
  owning the key.
