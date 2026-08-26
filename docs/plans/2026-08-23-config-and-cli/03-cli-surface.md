---
type: vwf-plan
title: cli-surface — 2026-08-23
description: Cycle plan (a diff) reshaping the binary's flags — --refresh
  renamed, --configure added to wire Claude Code, --help documenting the repo
  layer, and --debug reporting a missing config as normal.
status: done
covers: [
  docs/decisions.md,
]
requires: [
  docs/plans/2026-08-23-config-and-cli/02-config-relocation.md,
]
timestamp: 2026-08-23T14:03:00Z
tags: [ cli, configure, settings, debug, help ]
---

# Plan: cli-surface — 2026-08-23

## Slice

Contract §5 (CLI surface). The binary's flag set becomes the whole product
surface, because after
[distribution/01](../2026-08-23-distribution/01-drop-npm.md) there is no
installer to carry any of it.

| Flag            | Does                                                                    |
| --------------- | ----------------------------------------------------------------------- |
| `--statusline`  | render the main bar — unchanged                                         |
| `--subagent`    | render the subagent panel — unchanged                                   |
| `--refresh`     | refresh the spend cache and exit — **renamed** from `--refresh-spend`   |
| `--caps-hook`   | unchanged                                                               |
| `--debug`       | report config, wiring and a sample render — **gains a no-config state** |
| `--version`     | print the version — unchanged                                           |
| `--help` / `-h` | full usage, **and the no-argument behaviour**                           |
| `--configure`   | **new** — wire Claude Code's `settings.json`                            |

## Current state (actual)

**The flags today** (`src/_runtime/cli.rs:84-90`): `--version`/`-V`, `--help`/
`-h`, `--debug`, `--statusline`, `--subagent`, `--refresh-spend`, `--caps-hook`.

**No-argument behaviour already matches**, per §5: TTY stdin → help; piped stdin
→ the one-line missing-flag error.

**`--version` is constrained by §5**: *"must be checked first and print nothing
but the version, because the installer distinguishes an installed binary from a
bundled one by the shape of that answer."* The installer is being deleted, so
that **reason** expires — the guarantee does not, and this cycle has to say why
it is kept rather than leave §5 asserting a rationale that no longer holds.

**The binary wires nothing.** All of it is in the npm installer:

- `installer/src/modules/settings.ts` (172 lines) — three keys, `statusLine`,
  `subagentStatusLine`, and the `PostToolUse` caps hook. It carries an
  `Ownership` type with four states (`absent`/`ours`/`ours-stale`/`foreign`)
  because "is this ours to change?" was a real question.
- **The three keys are not symmetric.** The first two are scalars. `PostToolUse`
  is a **list**, and the only key that *removes* an entry on uninstall — so it
  must be merged into and filtered out of, never replaced.
- `installer/src/modules/receipt.ts` (83 lines) — prior state, so uninstall
  restores rather than infers.

**`--debug` already reports the layers.** `debug_report_with`
(`src/_runtime/app.rs:321`) prints `CONFIG LAYERS (low to high)` with each
layer's label, load state and path, and it distinguishes the three reasons a
path can be absent (`<embedded>`, `<no $HOME>`, `<no git root>`). After
[plan 2](./02-config-relocation.md), "no config file at all" is the **normal**
case rather than a broken one, and the report still calls it `not found`.

## Target state (per contract)

`brew install` then `claude-status --configure` is the whole setup. `--help` is
where a user learns the repo layer exists. `--debug` describes a config-free
machine as working, not as missing something.

## Delta — ordered steps

### 1. Rename `--refresh-spend` to `--refresh`

A hard rename with no alias. Nothing has shipped, so an alias would be
compatibility with a version that never existed. `proc::spawn_detached` passes
this flag to its own child, so the caller in `app.rs` moves with it.

### 2. Add `--configure`

It writes three keys into `~/.claude/settings.json`:

- `statusLine` → `claude-status --statusline`
- `subagentStatusLine` → `claude-status --subagent`
- `PostToolUse` → the `claude-status --caps-hook` entry

**It overwrites.** An existing status line from another tool is replaced without
asking. That is the decision; it is not an accident, and steps 3 and 6 exist to
keep it from being a nasty surprise.

**The `PostToolUse` list is still merged, not replaced.** Overwriting the status
line keys is the decision; deleting another tool's unrelated hooks is not. Add
our entry, replace a previous entry of ours, leave everything else.

**Unrelated keys in `settings.json` are preserved.** The file holds far more
than these three, and it belongs to Claude Code.

### 3. Say what `--configure` will do, before it does it

Because there is no receipt and no undo, the destructive case must be visible:
if an existing `statusLine` or `subagentStatusLine` is present and is not ours,
print what is being replaced. `--dry-run` prints and writes nothing.

**No receipt, no `--unconfigure`.** Recorded as a decision, not an omission: the
flag overwrites, and a user who wants their old status line back sets it again.
The website documents this.

### 4. Seed an empty user config

`--configure` also creates `~/.config/claude-status/config.json` if absent,
containing `$schema` and nothing else — a starting point that an editor can
complete against.

**An existing config is never touched.** Not merged, not topped up, not
reordered.

### 5. Rewrite `--help`

It is now the only documentation that ships with the binary. It carries the flag
table, what `--configure` writes, the config paths, **how to write a repo-level
config** — the path, that `projectName` is its only key, and an example — and
the website URL.

The repo layer has no other discovery route after
[plan 2](./02-config-relocation.md) deleted the autoseed. If `--help` is vague
about it, the feature is gone in practice.

### 6. Make `--debug` treat "no config" as normal

`CONFIG LAYERS` distinguishes *absent* from *broken*. A missing user config
reads as using defaults, not as `not found`:

```text
CONFIG LAYERS (low to high)
  embedded loaded     <built in>
  user     using defaults  ~/.config/claude-status/config.json (no file)
  repo     using defaults  <no git root>
```

It also reports the ignored non-`projectName` repo keys from plan 2's step 4,
and — after [plan 4](./04-schema-and-validation.md) — the validation findings.

### 7. Docs

§5's table gains `--configure` and the rename. Its `--version` paragraph is
rewritten: the guarantee stays, and the reason becomes that `--version` is the
formula's `test do` assertion and the one output shape a script can rely on. §3
records the seeded-empty-config behaviour.

## Acceptance criteria (from contract)

1. Given a `settings.json` with unrelated keys, when `--configure` runs, then
   the three keys are set and every other key is byte-identical.
2. Given a `PostToolUse` list containing another tool's hook, when `--configure`
   runs, then our entry is present and theirs is untouched.
3. Given `--configure` run twice, then the second run leaves the file
   byte-identical to after the first.
4. Given a `statusLine` belonging to another tool, when `--configure` runs, then
   it is replaced **and** what was replaced is printed.
5. Given `--configure --dry-run`, then it prints its plan and no file on disk
   changes.
6. Given no `~/.config/claude-status/config.json`, when `--configure` runs, then
   one is created containing `$schema` alone; given an existing one, then it is
   byte-identical afterwards.
7. Given `--help`, then it names the repo config path, that `projectName` is its
   only key, and the website URL.
8. Given a machine with no config files, when `--debug` runs, then it reports
   defaults in use and does not describe the state as an error.
9. Given `--version`, then stdout is exactly the version — unchanged by anything
   in this cycle.
10. Given the repo after this cycle, when it is searched, then `--refresh-spend`
    appears nowhere.

## Risks / drift

**`--configure` edits a file it does not own, and now has no undo.** The receipt
that made this reversible is deliberately gone. Criteria 1–3 are the guard, and
they matter more than they look: a malformed `settings.json` breaks Claude Code
itself, not just the bar. Read-modify-write preserving unknown keys, and never a
regenerate-from-scratch.

**Criterion 3 — idempotence — is the one most likely to be quietly false.** A
hook list that grows by one entry per run is the classic failure, and it takes
three runs to notice.

**"Overwrite without asking" will generate the first support question.** A user
with an existing status line runs `--configure` and loses it, with nothing to
restore from. That is the accepted design, but it means step 3's printing is not
cosmetic — it is the entire mitigation, and it should be loud.

**Losing the receipt loses more than undo.** `receipt.ts` also recorded whether
a file existed before, which is how uninstall knew what to delete versus leave.
Nothing in this cycle needs that, but nothing can reconstruct it later either;
if an uninstall path is ever wanted, it starts from nothing.

**§5's `--version` rationale expires quietly.** The sentence stays true while
its stated reason stops being. Step 7 must actually rewrite it — a contract that
justifies a rule with a thing that no longer exists teaches the next reader that
the rule is vestigial.

## Out of scope for this cycle

- **An `--unconfigure` or any uninstall path.** Explicitly decided against.
  `brew uninstall` removes the binary; the `settings.json` keys are the user's
  to clear.
- **Generating a repo config.** Documented in `--help`, written by the user.
- **Schema validation in `--debug`.** [Plan 4](./04-schema-and-validation.md).
- **Deleting `installer/`.**
  [distribution/01](../2026-08-23-distribution/01-drop-npm.md), which is where
  the TypeScript this cycle reimplements actually gets removed.
- **A config-editing TUI.** The website's generator is that surface.

## Gaps surfaced during execution

Executed 2026-08-23. 429 → 502 tests. All ten criteria addressed, but **five
cannot be met as written** — see B. Twenty-one gaps.

Recon ran before any code and found **30 defects in this plan**, including the
JSON shapes it prescribes for `settings.json` being wrong. Two adversarial
review passes then found **eleven more in the implementation**, none of which a
green suite could have shown: three were demonstrated by mutation, and one by
racing 1,470 processes. Every finding below was reproduced by running something.

### A. Defects in this plan's own text

**A1. Both render keys are objects, not scalars — and `PostToolUse` is nested
two levels deep.** Step 2 says "the first two are scalars". They are
`{type, command, padding, refreshInterval}` and `{type, command}`, and
`PostToolUse` is not a top-level key at all: it is `hooks.PostToolUse`, an array
of **groups**, each holding its own `hooks` array. Writing what the plan says
produces a file Claude Code silently ignores. The truth was already in this repo
— `--debug` has read these keys since before the cycle.

**A2. `padding: 0` and `refreshInterval: 4` are unmentioned, and one is
load-bearing.** Contract §1 records that `refreshInterval: 4` is what makes "the
bar renders every turn and every few seconds" true. Dropping it changes the
redraw cadence of every install. The plan's own §1 example was **also**
falsified by the bare-name decision and still showed the npm installer's
absolute path.

**A3. The binary had no error channel, and the plan's first fallible mode needs
one.** `run()` always returned `0` and `dispatch` returned a bare `String`.
`--configure` is the first **writing, fallible** mode, so no `$HOME`, an
unwritable file, a corrupt one or an `io::Error` could be reported at all —
`brew install && claude-status --configure` in a script could not detect
failure. The `dispatch`/`run` restructure is unbudgeted work this plan does not
name.

**A4. Step 6 is not implementable from the data it reads.** It says
`CONFIG
LAYERS` "distinguishes absent from broken". `LayerSource.loaded` was one
bool and `read_json_file` returns `None` for missing *and* malformed alike, so
the two were the same state. A third state on the struct plus a `load` that
stats and parses separately — again unbudgeted, and the same shape of change
cycle 02 hit at *its* A4. The plan's mock output is also impossible as drawn:
the state column is `{:10}` and `"using defaults"` is 14 characters.

**A5. `--dry-run` cannot be bolted onto this parser safely.** Unrecognised
tokens were silently ignored, so `--configure --dry-runn` — one typo — performed
a **real, unundoable write** of the user's `settings.json`. Fixed by collecting
unknown arguments and making them fatal for `--configure` **only**: a stray
argument must never silently destroy a file, and must never blank a status line
either. `Cli` lost `Copy` as a consequence.

**A6. Step 4 never says which `Config` to serialise, and the wrong one corrupts
the global config.** `layers::load` merges the repo layer's `projectName`, so
writing the *loaded* config from inside any repo carrying one would pin that
repo's name into `~/.config/claude-status/config.json` for every repo. It must
be `Config::default()`. This also decides cycle 02's inheritance: under
`Config::default()` neither of `write.rs`'s pinned exemptions can bite; under a
loaded config **both arm at once**.

**A7. The expiring `--version` rationale has five copies, not one.** §5
justifies "checked first, prints nothing but the version" with a reason about
the npm installer, which is being deleted. The same rationale is repeated at
`cli.rs:73-75`, `app.rs:41-42`, `cli.rs:180-182` and
`.config/mise/tasks/build/statusline:80-81` — plus a fifth at
`.github/workflows/release.yml:145`, inside the job that is now the replacement
rationale's live evidence.

**A8. §5's table never listed `--caps-hook`**, though the plan's own slice table
calls it "unchanged", and step 7's doc scope was short by three sites —
`statusline-behaviour.md:913` is in **§7**, not §5, and `readme.md` is not
mentioned at all.

### B. Acceptance criteria that cannot be met as written

**B1. C1's "byte-identical" is impossible.** The writer emits 2-space pretty
JSON, so indentation and spacing are normalised for keys nobody touched.
Implemented and tested as **value**-identical. Its other half is wrong too: "the
three keys are set" cannot be asserted at the top level, because one of them is
nested under `hooks`.

**B2. C7 names a website that does not exist.** The plan leans on a site three
times and never says what it is. `--help` now prints
`https://claude-status.virajp.dev` — the domain recorded in
[the plans index](../index.md) — which **404s until
[website/01](../2026-08-23-website/01-site.md) ships**. That plan already notes
"a dead link as a user's first impression" as its own likeliest failure; this
cycle adds a second consumer with the same dependency.

**B3. C10 is self-contradictory.** "`--refresh-spend` appears nowhere" cannot
hold in a repo whose own plan documents the rename — this document contains the
string, as do cycles 01 and 02's gaps. Enforced instead as **"a record, not a
reference"**: any line carrying the old flag must also carry `rename`, scanned
over `src/`, `tests/`, `docs/spec/`, `.config/`, `.github/` **and the repo
root**. `docs/plans/` is excluded by path rather than by directory name, so the
exclusion cannot silently widen.

**B4. C8 is vacuous.** "Reports defaults in use and does not describe the state
as an error" passes at `586134e` with zero lines written — `not found` is not an
error, and the report already ran on defaults. Re-implemented to assert the
*new* strings positively, and to assert that a planted unparseable config reads
`UNREADABLE`.

**B5. C9 cannot fail.** Nothing in this cycle touches `--version`. Fine as a
regression guard; it is not evidence.

**B6. C4 guards only the `foreign` case**, and so misses what the four-state
model exists for — `ours-stale`, which must be rewritten *quietly* rather than
shouted about, at both render keys and the hook.

### C. What execution found in the code

**C1. `--configure` silently destroyed other tools.** Ownership was
`contains("claude-status")`, so `claude-statusline`,
`/opt/claude-statusbar
--statusline` and `claude-status-pro` all read as
**ours** and were overwritten **with no warning**, while the control
(`starship prompt`) warned correctly. The module's own doc calls that warning
"the entire mitigation" for having no undo, and it never fired for the names
most likely to collide. Worse on the hook side, which *deletes*:
`node /work/vendor/context-caps.js --lint` — another project's hook — was
removed outright.

Fixed by matching the **program token**: the command's first shell word,
quote-aware, reduced to a basename. Verified across 12 cases including
`sh -c "claude-status --statusline"` (program is `sh`, correctly foreign), a
quoted path containing a space, and a trailing `# claude-status` comment that no
longer claims ownership. `LEGACY_HOOK_PATH` is now scoped to `.claude/hooks/`,
the only place `ai-plugins` ever wrote it — that arm is the one where a false
positive costs data.

**C2. The double-fire fix did not stop the double-fire.** "Replace every entry
of ours" was implemented as *making them identical*, and its test asserted
`len() == 2` calling that "deduplicated". Claude Code executes **every** entry
in the array, so the actuator still fired twice per tool call — the exact
failure the no-early-exit loop exists to prevent, with a comment explaining why
it was fine. Now the first entry is updated **in place** (preserving the group's
`matcher` and siblings), later ones are removed, and a group our removal emptied
is dropped while an already-empty group is left alone. *This defect originated
in an under-specified instruction from the orchestrator, not in the plan.*

**C3. The write widened permissions on every run, and a signal made it
permanent.** `fs::write` created the temp at `0666 & ~umask` and the mode was
restored only **after** the rename, so `settings.json` — which can carry an
`env` block with credentials — was caught wider than its original 0600 in
**10/10 runs**, and the temp sat at 0644 for the whole write. `remove_file` ran
only on the `Err` arm, so `kill -9` left a **world-readable copy of the user's
secrets on disk forever**. Fixed at the root: the temp is now *created* at the
target's mode, so no wider mode ever exists — which also removes the post-rename
chmod whose error was being discarded.

**C4. `~/.claude/settings.json` is a symlink on the author's machine**, into a
dotfiles repo. Temp-then-rename **replaces a symlink with a regular file**,
orphaning the real file — the user's settings would silently revert on their
next dotfiles sync, with nothing observable at the time. It would have hit the
repo's own author on his own machine on the first real run. Not in the plan, not
in the contract, and **the npm installer has the identical bug**.

Fixed by resolving only when the file itself is a symlink — unconditional
`canonicalize` also resolves parent directories, which would report a symlink
that is not there.

**C5. The symlink fix was defeated by a *dangling* symlink.** With the target
temporarily absent — dotfiles repo not cloned yet, `stow` not run, volume
unmounted — `canonicalize` fails, the fallback used the link path, and the
rename destroyed the link. Exit 0, nothing on stderr, no note even under
`--dry-run`. Now refused at read time, where the other refusals live.

**C6. "Merge, never rewrite" was violated inside our own keys.** Both write
sites replaced whole values, so a hook's `timeout: 45` — a real Claude Code key
— and any unrecognised sibling were deleted, every run. Now merged over: our
keys win, everything else survives. A *foreign* value is still replaced whole,
since its keys belong to another tool's schema.

**C7. Three guards passed with the behaviour they name deleted.** All three
demonstrated by mutation:

- Replacing all 58 lines of `HELP` with a **9-line keyword soup** passed every
  test — for a criterion whose own note says "a vague `--help` is the feature
  being gone in practice". Now pinned by five section headers and a length
  floor.
- A genuine `--refresh-spend` reintroduction **in `readme.md`**, phrased as a
  live instruction, passed the guard written to prevent exactly that — the scan
  never walked the repo root.
- "Leave an unchanged file completely alone" turned only a *unit* test red when
  broken; both three-run tests stayed green, because an identical rewrite
  produces identical bytes. The `nothing to change — left untouched` line was
  asserted nowhere.

**C8. Concurrency is safe, and the reason is structural.** ~1,640 races across
two independent harnesses: **no corrupt or truncated file is possible**, and
13,116 concurrent reads produced zero torn reads. `wire` is a pure deterministic
function of the bytes it read, so two racing runs compute identical output and
the last rename is byte-identical to what it replaced; a run starting after
another finished writes nothing at all. The only exposure is a **third-party**
writer inside a ~1 ms window of a 3.2 ms process — a lost update, never an
invalid file.

**C9. Six doc comments and four readme passages were falsified.** Including
`readme.md`'s "the only thing this tool writes on its own is the spend cache",
which this cycle made false, and "three surfaces… you won't run them by hand",
when the cycle adds a fourth that must be run by hand. Same drift pattern the
drift audit documents, and the same one cycle 02 recorded at its C9.

### Also noted, not acted on

- **`--configure` can blank the bar on an npm-installed machine.** The installer
  wrote an absolute `~/.claude/bin/claude-status --statusline`, which is
  correctly *ours*, so `--configure` rewrites it to the bare name — quietly, by
  design. If `claude-status` is not on Claude Code's `PATH`, the bar goes blank
  **immediately after the user ran the command meant to fix their setup**. The
  `PATH` dependency is the accepted cost of surviving `brew upgrade`; it is now
  named in `readme.md` beside the collision note, together with the fact that
  `npx --uninstall` would discard the Rust wiring rather than restore it.
- **A hardlinked `settings.json` goes stale**, and a **read-only (0400) one is
  still rewritten** — temp-then-rename needs only directory write permission.
  Both are inherent to atomic writing, both are now warned about on stderr, and
  neither changes the write: refusing would cost atomicity, and a truncated
  `settings.json` breaks Claude Code outright. A hardlink cannot be followed at
  all — it is a second name for an inode, not a pointer to a path.
- **Numbers are re-encoded.** Integers beyond `u64` and floats past 17
  significant digits lose precision. `u64::MAX`, `1.0` and `-0.0` round-trip.
  `arbitrary_precision` would change number handling across the whole binary
  including the config path.
- **Three files Claude Code reads, we refuse**: `1e400`, nesting deeper than
  128, and a UTF-8 BOM. The refuse direction is safe, but the messages name the
  parser's complaint rather than the cause, and a BOM is plausible from a
  PowerShell-authored file.
- **A renamed copy of this binary is no longer recognised as ours** — the cost
  of C1's narrowing, which trades a false negative (a second group appended) for
  avoiding a false positive (another tool destroyed). No shipped install can
  reach it.
- **Two phantom differential results, both caught by controls.** A wall-clock
  `ts` in the usage mirror produced 60 differences — proven phantom by running
  the **old binary against itself**, which differed from itself — and separately
  a `date +%s` called once per binary straddled a second boundary. Both are the
  failure cycle 02 recorded twice: a green result from a comparison of nothing.
  The harness now prints how many invocations carried a real mirror.
- **The installer and the binary both write
  `~/.config/claude-status/config.json`** until
  [distribution/01](../2026-08-23-distribution/01-drop-npm.md), with different
  contents.
- **`spend_render`'s wall-clock sensitivity** did not reproduce across ~22 runs.
  Pre-existing; see cycle 02's ledger.
