# claude-status — build plan

A ground-up rewrite of the Claude Code powerline statusline, extracted from
[`virajp/ai-plugins`](https://github.com/virajp/ai-plugins) into this repo.

> **This must be written in Rust.** Not TypeScript, not Go, not a Node script
> with a Rust helper. The whole binary is Rust, and every phase below assumes
> it. If you find yourself reaching for Node to do "just this one part", stop
> and solve it in Rust instead.

## Why Rust, so the constraint is not arbitrary

The bar renders on **every turn and every few seconds** — `refreshInterval: 4`
in the shipped config — in every open session at once. Process startup is
therefore the dominant cost, and it is paid constantly.

The current implementation is a single ~900-line CommonJS file run as
`node ~/.claude/scripts/statusline`. Node's own startup is roughly 30–50 ms
before a line of it executes; a Rust binary is 1–2 ms. That is the entire
argument, and it is a good one: this is a program whose runtime is almost
entirely startup.

Two secondary wins follow. A single static binary has no runtime to install and
cannot break when the user's Node changes. And the spend subsystem (below) does
file locking, atomic replacement and a network fetch in a detached child —
things Rust expresses more honestly than a script leaning on `try {} catch {}`.

**Do not port the JavaScript line by line.** It is the *specification* of
behaviour, not a design to imitate. Where this document describes behaviour,
match it. Where it describes structure, use your judgement.

---

## 1. What it is

One binary, two rendering surfaces, selected **by an explicit flag**:

| Invocation     | Render                                                |
| -------------- | ----------------------------------------------------- |
| `--statusline` | the **main bar** — two powerline lines                |
| `--subagent`   | the **subagent panel** — NDJSON, one row per subagent |

> **Amended 2026-08-19** (`main-bar` cycle). This originally specified
> shape-detection on a `tasks` array in the payload. An explicit surface is
> diagnosable and a shape heuristic is not — a payload that stopped carrying
> `tasks` would silently render the wrong surface, with no way to tell from the
> output. The cost is that **the installer must rewrite both `settings.json`
> keys on every upgrade**, and anyone who hand-swaps the binary without
> re-running it gets the missing-flag line below instead of a bar.

Claude Code is wired to it with two keys in `~/.claude/settings.json`, both
pointing at the same binary and **both carrying their flag**:

```json
{
  "statusLine": {
    "type": "command",
    "command": "${HOME}/.claude/bin/claude-status --statusline",
    "padding": 0,
    "refreshInterval": 4
  },
  "subagentStatusLine": {
    "type": "command",
    "command": "${HOME}/.claude/bin/claude-status --subagent"
  }
}
```

Invoked with **no flag at all** — which is what a stale `settings.json` produces
after an upgrade — the binary discriminates on whether stdin is a TTY. A TTY
means someone typed it, so it prints full help. A pipe means Claude Code invoked
it, so it prints exactly one line on stdout:

```text
claude-status: missing --statusline or --subagent (run --help)
```

One line fits the bar and names the fix. Twenty lines of usage would be
unreadable in a status line, and printing nothing would leave the user with a
silently blank bar and no clue.

### Five invariants that outrank everything else

These cut across every section below. Each is here rather than beside the
feature it constrains because more than one feature has to obey it, and the ones
added later were added precisely because a rule kept in one place had been
applied in one place.

1. **stdout is the bar.** Claude renders whatever arrives there. Diagnostics,
   warnings, errors — all stderr, always. A single stray byte on stdout is a
   corrupted status line.
2. **A render never blocks.** No network call, no unbounded subprocess, no
   waiting on a lock. Anything slow is read from cache or skipped. The git
   subprocesses are the only exception, and the whole set of them is
   hard-bounded at 250 ms.

   > **Amended 2026-08-19** (`main-bar` cycle). This originally said "the two
   > git subprocesses … both are hard-bounded at 250 ms". There are up to
   > **four** — the ahead count, `diff --numstat HEAD`, its `--cached` fallback,
   > and the untracked probe — and the old implementation ran them
   > **sequentially at 250 ms each**, a ~1 s worst case. They now run on two
   > threads under **one shared 250 ms deadline**, so the budget is what the
   > invariant always claimed it was. See [§6](#6-git-resolution).
3. **A render never fails visibly.** Any panic or error must still produce a
   usable line. The current implementation catches everything and falls back to
   printing `⚡ Claude`. Reproduce that: wrap the render in
   `std::panic::catch_unwind`, print the fallback, and put the real error on
   stderr.
4. **Only the renderer emits escapes.** **Added 2026-08-21** (`macos-only`
   cycle), after review found the powerline separators reaching the row
   unfiltered. Every dynamic value is stripped of control characters before it
   is written, on **both** rendering surfaces and in `--debug`. See
   [§4a](#4a-what-may-carry-an-escape-and-what-may-not).
5. **An unresolvable `$HOME` means absent, never relative.** **Added
   2026-08-21** (`macos-only` cycle), after review found four callers of the
   home-directory helper had each invented their own answer and one had invented
   the wrong one.

   `$HOME` is the only source of the user's home directory. When it is unset or
   empty, every path derived from it is **absent** — the feature that needed it
   does nothing, and says so where there is somewhere to say it. A path that
   names the home directory and cannot resolve one must **never** degrade to the
   unexpanded text: `~/x` and `spend.json` taken literally are *relative* paths,
   so the process writes into whatever directory Claude Code was launched from
   believing it wrote into the home one. That is a stray file in the user's
   working tree, and a cache that never hits because the next session starts
   somewhere else.

   Concretely, the spend cache path ([§7](#7-the-spend-subsystem)), the usage
   mirror directory ([§8](#8-the-usage-mirror--a-contract-with-ai-plugins)) and
   the credentials **file** are each absent without a home. Invariant 3 still
   outranks this: the render succeeds, the segment omits like any other, and
   `--debug` names the missing `$HOME` rather than reporting an empty result.

   A path that never asked for the home directory is unaffected — an absolute
   `$CLAUDE_STATUS_SPEND_CACHE` works with no `$HOME` at all.

   **The macOS keychain is the exception, and the ordering is deliberate.** The
   keychain is *not* scoped by `$HOME`, so "no home" does not mean "no
   credentials": the fallback in [§7](#7-the-spend-subsystem) can still return a
   real token. The rule is that **the cache path is resolved first, and no fetch
   is made when it is absent** — with nowhere to write the result, a request
   would spend the account's rate limit to produce nothing, on every render.
   This is stated because it is the one case where a `$HOME`-derived absence has
   to gate something that is not itself `$HOME`-derived, and it was previously
   only implied by the order the code happened to run in.

   The same asymmetry is why a test with a fake `$HOME` is **not** protected
   from reaching the live endpoint. Two rules follow, and they are separate —
   stating only the first is how three harnesses came to invent three different
   answers to the second:

   1. **Pin the endpoint.** Every test that can reach `http::fetch` sets
      `$CLAUDE_STATUS_SPEND_URL` itself rather than trusting its runner to have
      exported it.
   2. **Neutralise *both* credential arms.** A fake `$HOME` removes the
      credentials *file*. The keychain arm is not `$HOME`-scoped: it shells out
      to `security`, so it is neutralised by pointing `PATH` at a directory that
      does not exist. A test that wants credentials seeds the file instead; a
      test that wants *none* must do both, or it is asserting whatever happens
      to be true of the machine it ran on.

   **`PATH=""` is not the way to do the second.** An empty `PATH` is a single
   empty entry, which POSIX resolves as the **current directory** — so a
   `security` binary sitting in the package root would be run and its stdout
   parsed as an OAuth document. Unsetting `PATH` is no better: the C library
   falls back to `_PATH_DEFPATH`, which includes `/usr/bin`.

---

## 2. Input contracts

Both payloads arrive as one JSON object on stdin. **Every field is optional.**
Claude Code has changed this shape before and will again, so parse defensively —
a missing or unexpected field omits its segment, it does not fail the render.

### Main bar payload

```jsonc
{
  "session_id": "abc123", // keys the usage mirror
  "session_name": "users-and-groups",
  "model": { "display_name": "Opus 5", "id": "claude-opus-5" },
  "effort": { "level": "high" },
  "workspace": { "current_dir": "/path/to/repo" },
  "cwd": "/path/to/repo", // fallback for the above
  "cost": { "total_cost_usd": 46.51, "total_duration_ms": 33540000 },
  "context_window": {
    "used_percentage": 26,
    "context_window_size": 1000000,
    "total_input_tokens": 259000,
    "current_usage": { // fallback when the above is absent
      "input_tokens": 0,
      "cache_creation_input_tokens": 0,
      "cache_read_input_tokens": 0,
    },
  },
  "rate_limits": {
    "five_hour": { "used_percentage": 7, "resets_at": 1774200000 },
    "seven_day": { "used_percentage": 1.0, "resets_at": 1774600000 },
  },
}
```

Normalisation rules that are **not** obvious and must be preserved:

- `model` may be an object (`display_name`, else `id`) **or a bare string**.
  Strip a trailing parenthetical: `"Opus 5 (1M context)"` → `"Opus 5"`.
- `effort` may be an object (`level`) **or a bare string**.
- `workspace.current_dir` → `cwd` → the process's own cwd, in that order.
- Context tokens: prefer `total_input_tokens` (it is what `used_percentage` is
  computed from); else sum the three `current_usage` fields; else derive from
  `used_percentage × context_window_size`.
- `rate_limits.seven_day` **or** `rate_limits.weekly` — both spellings appear.
- `resets_at` may be epoch **seconds**, epoch **millis**, or an ISO 8601 string.
  Discriminate seconds from millis on `> 1e12`.

### Subagent panel payload

```jsonc
{
  "columns": 120,
  "cwd": "/path/to/repo",
  "model": { "display_name": "Opus 5" }, // panel-wide, not per task
  "effort": { "level": "high" },
  "tasks": [{
    "id": "t1", // required; skip a task without one
    "name": "reviewer",
    "type": "local_agent",
    "status": "running",
    "description": "Auditing auth flow",
    "label": "…", // fallback for description
    "tokenCount": 18234,
    "startTime": 1774200000000,
    "cwd": "/path/to/repo",
  }],
}
```

**Output is NDJSON** — one `{"id": …, "content": …}` object per line, where
`content` is the fully rendered ANSI row. Not a single blob.

Two traps here, both learned the hard way:

- **`type` is almost always the generic `"local_agent"`** regardless of the
  actual subagent type, so it is rendered as a glyph, never as text. Do not fall
  back to showing it as the name.
- Neither `model` nor `effort` is a documented per-task field. Read a per-task
  value if present (a future build may add one), else fall back to the
  panel-wide value, else omit the segment.

---

## 3. Configuration

**Three** layers, deep-merged **low → high**:

1. The **shipped defaults, embedded in the binary**. Always present.
2. `~/.config/claude-status.json` — the per-user config. Seeded with the full
   defaults at install; the user's thereafter.
3. `<repo-root>/.config/claude-status.json` — per-repo overrides. **Wins.**

> **Amended 2026-08-19** (`main-bar` cycle), on two counts.
>
> **The embedded layer is new.** With only the two file layers, a machine with
> neither rendered *blank*. Embedding the defaults means a cold start draws a
> full bar. Output is byte-identical for every install whose user file *is* the
> seeded defaults, which is all of them; the visible change is that a user who
> deleted a key expecting it gone now gets the default back.
>
> **The file is renamed** from `statusline.json` to `claude-status.json`, for
> consistency with the tool's identity. The note below advises against exactly
> this, and the migration is the price: `--install` moves the old file,
> preserving the user's theming. (It said `--uninstall` puts it back; the
> 2026-08-22 amendment below withdraws that.) The binary only ever knows the new
> name, so no per-render stat is spent on a legacy path. Until the Phase 5
> cutover **both files exist on purpose** — the JS bar is still live and still
> reads the old name. Neither is stale.

> **Amended 2026-08-22** (`repo-autoconfig` cycle). Layer 3 may now be
> **created** by a render, and `projectName` leaves layers 1 and 2 entirely.
>
> `autoConfigureRepo` — a boolean, **default `true`** — lets a `--statusline`
> render that finds no layer 3 write one: a repo-level `statusline.json` is
> migrated if present, otherwise `projectName` is seeded from the repo
> directory's name. Writing `false` into layer 2 opts out.
> `npx @askviraj/claude-status --configure` applies the identical rules
> explicitly, for anyone who would rather it never happen on the render path.
>
> **`projectName` is repo-level only.** It ships in neither the embedded
> defaults nor the seeded user config, and the shipped schema describes it
> *without* the asset carrying it — the one deliberate asymmetry between the
> two, pinned by name in `defaults_integrity`. A key that identifies one repo
> has no meaningful value at a layer shared by all of them: embedding the old
> `"Project-Name"` placeholder meant every unconfigured repo rendered the same
> fictional name. A cold start now omits the segment, which is why the
> `cold_start` golden is one line rather than two.
>
> **A migration rewrites; it does not rename.** The legacy file points `$schema`
> at the `ai-plugins` repo, and one kept under that URL is validated against the
> wrong document for the rest of its life. So `$schema` is repointed and the
> file written under the new name, the old one removed only once the new one is
> on disk. Every other key is carried across untouched, with one exception:
> migrating the **user** layer drops `projectName`. The JS bar read that key
> from this same file, but here it is repo-level only, and one kept at layer 2
> would name every repo the user opens after whichever one they set it in —
> exactly what the paragraph above says layer 2 must never do. It is dropped
> rather than moved, because nothing in the user layer records which repo it was
> meant for, and `--configure` derives the right name from the repo it runs in.
> A legacy file that is **not a JSON object** has nothing to set `$schema` on
> and is moved as-is. This applies at both levels and in both writers — the
> binary and the installer.
>
> **`--uninstall` removes; it does not restore a migrated file.** A config the
> install migrated in is this project's file and is removed under its own name,
> guarded by the digest the receipt recorded — an edit since the install keeps
> it. The legacy `statusline.json` it came from is **not** recreated. Bringing
> it back would leave the user holding a config for a tool they no longer have,
> and the receipt therefore records no `movedFrom` for anything. The restore
> discipline still governs `settings.json` **keys**, which are prior state this
> installer overwrote, not files of its own.
>
> Four constraints make the render-path write safe:
>
> - **Read from layers 1 and 2 only.** The flag is resolved before layer 3
>   exists, so a repo cannot enable its own creation. The accessor's fallback is
>   `true`, matching the shipped default, so a config that failed to parse
>   behaves like one that was never written.
> - **`--statusline` only.** `--subagent` and the caps hook resolve a repo root
>   too and stay strictly read-only, so there is exactly one writer.
> - **Silent on every failure.** A read-only checkout, a `.config` that is a
>   file, a full disk: the render proceeds. Invariant 3 outranks seeding a
>   convenience file, and invariant 1 leaves nowhere to complain to. An existing
>   layer-3 file that does not parse is never overwritten — the create path
>   re-checks for the *file*, not for a successful parse.
> - **Costs one stat when off or already done.** `layers::load` already stats
>   layer 3; the create path is reached only when that stat came back empty. The
>   render that writes re-reads the layers, so the name appears on that render
>   rather than the next.
>
> The write is atomic (temp file, then rename), because two sessions can render
> in the same repo at once, and indented rather than compact, because unlike the
> spend cache this is a file a person opens.

A layer that is missing, unreadable, malformed, or **not a JSON object** is
ignored rather than fatal; the render proceeds on the layers below it.

Merge semantics, which must match exactly:

- Objects merge **key by key**, recursively.
- Arrays and scalars are **replaced wholesale**. A repo overriding `lines` means
  to replace the layout, not to append to it.
- Keys `__proto__`, `constructor`, `prototype` are skipped. In Rust this is moot
  — keep the *behaviour* of ignoring them so a config written for the old
  implementation behaves identically.

Repo root is resolved from the render's cwd (see §6). A run outside a repo has
only the user layer.

> Keep the config **file name, location and schema identical** to the current
> implementation. Existing users should be able to point Claude at the new
> binary and see the same bar. Any schema change is a migration you have to
> design; there is no reason to take that on in v1.

### Colour specs

Three accepted forms, anywhere a colour is expected — resolve in this order:

1. A palette name: `"blue"` → looked up in `palette`.
2. A hex string: `"#458588"` or `"#abc"` (3-digit expands by doubling).
3. A literal RGB triple: `[69, 133, 136]`.

Anything unresolvable falls back to `palette.white`, else `[251, 241, 199]`.
Output is 24-bit ANSI: `\x1b[38;2;R;G;Bm` foreground, `\x1b[48;2;R;G;Bm`
background.

### The shipped defaults

The full defaults are committed beside this plan as
[`statusline-defaults.reference.json`](./statusline-defaults.reference.json) — a
**byte-faithful copy** of the current `tools/statusline/statusline.json`. Read
it from there. It is what gets seeded into `~/.config/statusline.json`, and it
is the product's visual identity: Gruvbox palette, Nerd Font glyphs.

> **Do not retype the glyphs.** Almost every symbol in that file is a Nerd Font
> **private-use codepoint**, which renders as nothing or as a box in most
> editors, diffs and terminals — and is therefore silently dropped by
> copy-paste, and by any model transcribing it. This plan originally inlined
> them and lost all 28 that way. Copy the JSON file as bytes, and verify by
> **rendering** the bar, never by reading a diff.

The codepoints, so a lost glyph is recoverable and a port is checkable:

| Key                                | Codepoint                     |
| ---------------------------------- | ----------------------------- |
| `gauge.empty`                      | `U+25B1`                      |
| `gauge.filled`                     | `U+25B0`                      |
| `powerline.cap`                    | `U+E0B6`                      |
| `powerline.sep`                    | `U+E0B0`                      |
| `powerline.sepThin`                | `U+E0B1`                      |
| `powerline.thinFg`                 | `U+0067 U+0072 U+0065 U+0079` |
| `subagent.statuses.done.symbol`    | `U+F00C`                      |
| `subagent.statuses.error.symbol`   | `U+F00D`                      |
| `subagent.statuses.pending.symbol` | `U+F017`                      |
| `subagent.statuses.running.symbol` | `U+F04B`                      |
| `symbols.agent`                    | `U+F007`                      |
| `symbols.ahead`                    | `U+2191`                      |
| `symbols.branch`                   | `U+E0A0`                      |
| `symbols.context`                  | `U+F1C0`                      |
| `symbols.cost`                     | `U+23F1 U+FE0F`               |
| `symbols.dirtyAdd`                 | `U+002B`                      |
| `symbols.dirtyDel`                 | `U+002D`                      |
| `symbols.dirtyMix`                 | `U+00B1`                      |
| `symbols.duration`                 | `U+F017`                      |
| `symbols.folder`                   | `U+F07B`                      |
| `symbols.model`                    | `U+26A1`                      |
| `symbols.project`                  | `U+F401`                      |
| `symbols.repo`                     | `U+F401`                      |
| `symbols.reset`                    | `U+21BB`                      |
| `symbols.session`                  | `U+F02B`                      |
| `symbols.spend`                    | `U+F09D`                      |
| `symbols.tokens`                   | `U+F51E`                      |
| `symbols.win5h`                    | `U+F252`                      |
| `symbols.win7d`                    | `U+F073`                      |
| `symbols.worktree`                 | `U+1F332`                     |
| `typeSymbols._default`             | `U+F544`                      |
| `typeSymbols.background`           | `U+F013`                      |
| `typeSymbols.cloud_agent`          | `U+F0C2`                      |
| `typeSymbols.local_agent`          | `U+F109`                      |
| `typeSymbols.mcp`                  | `U+F1E6`                      |
| `typeSymbols.remote_agent`         | `U+F0C2`                      |
| `typeSymbols.review`               | `U+F06E`                      |
| `typeSymbols.task`                 | `U+F0AE`                      |
| `typeSymbols.test`                 | `U+F0C3`                      |

The non-glyph values worth having inline: `defaultFg` is `white`;
`worktreePattern` is `worktree`; `projectName` is `Project-Name`; `gauge.width`
is `10`; `spend` is `{ refreshMinutes: 15, show: "auto" }`;
`subagent.descBudgetFraction` is `0.45`; and the default `lines` are
`[["model","context","rl5h","rl7d","spend","cost"], ["project","worktree","branch"]]`.

Per-segment default styling: `model` blue/bold/white · `context` aqua · `rl5h`
blue · `rl7d` purple · `session` orange · `spend` orange/bold/white · `cost`
green/bold/white · `duration` aqua · `project` green/bold/white · `worktree`
yellow on grey · `branch` aqua. Subagent segments: `head` bold · `name`
orange/bold · `model` blue · `desc` bg3 · `tokens` aqua · `duration` purple.

The palette, as RGB triples: `aqua` 104,157,106 · `bg3` 102,92,84 · `blue`
69,133,136 · `green` 152,151,26 · `grey` 60,56,54 · `orange` 214,93,14 ·
`purple` 177,98,134 · `red` 204,36,29 · `white` 251,241,199 · `yellow`
215,153,33.

Subagent status matching, tried **in config order**, first `match` regex to hit
wins; an entry with an empty `match` is the fallback for anything unmatched:
`done` green `done|complete|success|finish|ok` · `error` red
`error|fail|cancel|abort` · `running` blue `run|active|progress|working|busy` ·
`pending` bg3 (fallback).

> **Amended 2026-08-21** (`subagent-panel` cycle). Three details of the walk are
> observable and were unstated: an entry with an empty `match` is *recorded* as
> the fallback and the walk **continues**, so with two of them the **last**
> wins; the patterns are unanchored substrings, so `not_ok` matches `done`
> through its `ok` alternative; and a pattern the engine rejects is skipped
> rather than fatal.

### The subagent description budget

Absent from this document until the panel was built, and every part of it is
visible in the output.

- **Width** is `payload.columns`, else `$COLUMNS`, else `80`. A zero falls
  through at each rung rather than winning.
- **Budget** is `max(12, floor(width × subagent.descBudgetFraction))` — so 120
  columns gives 54, and the absent-`columns` case gives 36. A fraction of `0` is
  kept and clamps to the floor of 12.
- **The text** is `description`, else `label`, else nothing; every whitespace
  run collapses to a single space and the result is trimmed. An empty result
  omits the segment — a description carrying a newline would otherwise break the
  row in half.
- **Truncation** is to `budget - 1` **UTF-16 code units** plus one U+2026
  HORIZONTAL ELLIPSIS, so a truncated description is exactly `budget` units
  long. UTF-16 because that is what JS `String.length` and `slice` count. It
  measures units rather than terminal columns, so a CJK-heavy description
  overruns its visual budget — a known cosmetic flaw the original shares.

---

## 4. Rendering model

A **line** is an ordered list of entries. An entry is either a segment id string
(`"model"`) or an object `{name|id, bg?, fg?, bold?}` overriding that segment's
styling inline.

Styling resolves: **inline override → `segments.<id>` default → hard fallback**
(`bg: "blue"`, `fg: defaultFg`, `bold: false`).

### The powerline row

Given segments each carrying `{text, bg, fg?, bold?}`:

1. Open with `cap`, coloured as the **first segment's background**.
2. For each segment: set bg + fg (+ bold), emit `text` (one space either side),
   reset.
3. Between neighbours:
   - **different backgrounds** → `sep`, fg = this bg, bg = next bg.
   - **same background** → `sepThin`, fg = `thinFg`, bg = next bg. This is what
     keeps the seam visible when two adjacent segments share a colour.
4. After the last segment → `sep` with fg = last bg and no bg, so it dissolves
   into the terminal background.

A segment builder returning "no data" **omits the segment entirely** — it does
not render an empty box. A line whose segments all omit renders as nothing, and
an all-empty line is dropped rather than printed blank.

### The segment catalogue

Eleven segments. Exact output text, `{sym.X}` meaning the configured symbol.
Every `·` below is one literal space; the renderer adds one more space on each
side of the whole text.

| id         | Text                                                                      | Omitted when                                   |
| ---------- | ------------------------------------------------------------------------- | ---------------------------------------------- |
| `model`    | `{model}·Opus 5·[high]` — the `[effort]` part only when effort is present | never (falls back to `Claude`)                 |
| `context`  | `{context}·▰▰▰▱▱▱▱▱▱▱·259k/1M·(26%)`                                      | never                                          |
| `rl5h`     | `{win5h}·7.0%·{reset}·4h36m` — the reset half only when known             | `used_percentage` absent                       |
| `rl7d`     | `{win7d}·1.0%·{reset}·5d2h`                                               | `used_percentage` absent                       |
| `session`  | `{session}·users-and-groups`                                              | `session_name` absent **or empty**             |
| `cost`     | `{cost}·$46.51`                                                           | never — an absent cost renders `$0.00`         |
| `spend`    | `{spend}·$75.93/$150·(51%)`                                               | see §7 — four separate gates                   |
| `duration` | `{duration}·9hr 19m`                                                      | `total_duration_ms` **absent**; `0` renders    |
| `project`  | `{project}·Project-Name`                                                  | no `projectName` **in config**, not in payload |
| `worktree` | `{worktree}·{folder}·sub/path` — **two** symbols                          | not inside a worktree                          |
| `branch`   | `{worktree}·{branch}·main·↑·±`                                            | no branch resolved                             |

> **Amended 2026-08-19** (`main-bar` cycle), against the reference builders.
> Four rows were wrong:
>
> - **`rl5h` / `rl7d`** — there is a space **before** the reset glyph as well as
>   after it. The table showed `7.0%{reset}`, which renders the percentage and
>   the glyph run together.
> - **`duration`** — omitted only when `total_duration_ms` is *absent*. An
>   explicit `0` renders `{duration}·0s`.
> - **`branch`** — the worktree prefix is `{worktree}` *then* `{branch}`; the
>   table implied the branch glyph came first. The ahead and dirty markers each
>   follow one space, and each is conditional.
> - **`context`** — with no data at all it still renders, as
>   `{context}·▱▱▱▱▱▱▱▱▱▱·?/?·(0%)`.

An unknown segment id in `lines` writes `statusline: unknown segment "<id>"` to
**stderr** and omits the segment. It must not fail the render, and the exit code
stays 0. A segment builder that *panics* costs only its own segment.

### Formatting helpers

Get these exactly right; they are visible on every render.

- **Tokens** — `1234567 → "1.2M"` (one decimal, trailing `.0` stripped),
  `259000 → "259k"` (rounded, no decimal), below 1000 → the plain number.
  Unknown → `"?"`.
- **Duration** — `> 1h` → `"9hr 19m"`; `> 1m` → `"4m 12s"`; else `"45s"`.
- **Reset-in** — `> 1d` → `"5d2h"`; `> 1h` → `"4h36m"` (minutes zero-padded to
  two digits); else `"12m"`; already past → `"now"`.
- **Gauge** — fixed `width` (default 10). `filled = round(pct/100 × width)`,
  clamped to 0..100 first.
- **Money** — minor units + exponent → `$75.93`, with a whole-dollar amount
  rendering as `$75` (strip a trailing `.00`).

### 4a. What may carry an escape, and what may not

**Added 2026-08-21** (`macos-only` cycle). Before this, nothing said which
strings on the row were trusted, and the answer in the code turned out to be
"the ones somebody remembered".

**Treat every dynamic value as hostile.** Not as a worst case — as the normal
case. A branch name, a directory under a worktree, a session name, a model
string, and a task's `name` and `description` (written by a model, and therefore
steerable by indirect prompt injection) all reach the bar unreviewed. So does
**`<repo-root>/.config/claude-status.json`**, which is read from whatever
repository the user changes into: cloning a hostile repo is the entire attack,
with no further interaction.

**Only the renderer emits escape sequences.** Every dynamic value is stripped of
control characters before it is written. This applies to the main bar, the
subagent panel and `--debug` alike: the panel's NDJSON escaping is **transport
encoding, not a control** — Claude Code decodes it back before rendering — and
`--debug` writes to a terminal like everything else.

Stripped, as **two** filters rather than three:

- **`Cc`** — the Unicode control category, which is `U+0000`–`U+001F`, `U+007F`
  **and** `U+0080`–`U+009F`. C1 needs no rule of its own: it is already `Cc`. It
  matters because a terminal in 8-bit mode reads `U+009B` as CSI with no `ESC`
  in front of it.
- **The invisibles that are not `Cc`** — bidi overrides and isolates
  (`U+202A`–`U+202E`, `U+2066`–`U+2069`), and `U+200B` / `U+FEFF`.

Kept: ZWJ, variation selectors, and the private-use codepoints — the bar is
built from Nerd Font glyphs, so filtering those would erase it.

**The filter belongs at the point every value passes through**, not in each
producer. There are **five** such points, one per surface, and a sixth surface
would need its own:

| Surface            | Chokepoint                                                                | Why there                                                                                                                                                   |
| ------------------ | ------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Main bar           | `segments::build`                                                         | Every segment's text is assembled through it                                                                                                                |
| Subagent panel     | the sweep ending `task_row`                                               | The panel builds its `Segment`s directly and inherits none of the bar's filtering                                                                           |
| Powerline seams    | `Powerline::from_config`                                                  | Config-supplied, and written **outside** any segment's SGR bracket — the widest of the five                                                                 |
| `--debug` report   | one sweep over the assembled report, before the sample render is appended | Many values, many sections, one write                                                                                                                       |
| stderr (all of it) | `_shared::diag`                                                           | stderr is a terminal too. `narrate` is the `--debug`-gated caller; the panic reporter, the unknown-segment warning and the bad-regex warning are the others |

The stderr surface was the last to be found, and for the usual reason: it is not
stdout, so it did not look like a rendering surface. It is one. It was first
patched at two `{}` writes by hand — the per-write pattern this section exists
to reject — then narrowed to `narrate`, which turned out to be **one of six**
writers. `_shared::diag` is now the only `eprintln!` in the crate, which makes
the rule checkable with a grep rather than by reading every call site.

`--debug` earned a chokepoint rather than a call per write, and the reason is
the cycle that added it: filtering the paths first missed the layout entries and
the spend gate table, both of which reach the terminal by a different route.
Anything added to the report later is covered without anyone having to remember.

Two consequences of the `--debug` sweep worth stating, because both are
load-bearing:

- **Newlines survive it.** The report is deliberately many lines, so it uses a
  variant of the filter that keeps `\n` and strips everything else.
- **The `SAMPLE RENDER` section is appended after it.** That section *is*
  renderer output: its SGR codes are meant to be there, and every dynamic value
  inside it already passed through `segments::build`. Sweeping it would strip
  the colours the section exists to show.

**stderr keeps newlines out, and pays for it.** `_shared::diag` uses the row
filter, so one call is one line — which collapses a multi-line panic payload
onto a single line rather than preserving its shape. Deliberate: a panic message
quotes whatever it panicked on, so allowing a newline there would let a branch
name or a config value forge a second `claude-status:` line. A stack's shape is
worth less than a diagnostic whose boundaries a reader can trust.

**A dynamic value may never contribute a newline.** This is a rule, not a
consequence of the one above, and it is why `--debug`'s report-wide sweep is
**not** its only defence: that sweep exempts `\n` so the report can be many
lines, and a value carrying one would forge a line, a section header, or a whole
`CLAUDE WIRING` block in the diagnostic a user reads *because* they are trying
to work out what is wrong. No escape is needed for that attack. Every value in
the report therefore also goes through the row filter, which strips newlines;
only the report's own structure may add them.

The report may still *quote* a hostile value — that is it doing its job. What it
may not do is let the value stop being a quoted value.

**Known residual.** A dynamic value may still contain a private-use separator
glyph and so *look* like a segment boundary. Accepted: the same config layer can
already set the row's colours by design, and the line drawn here is between
theming the bar and escaping out of it.

---

## 5. CLI surface

**`--version` must be checked first and print nothing but the version**, because
the installer distinguishes an installed binary from a bundled one by the
*shape* of that answer.

| Invocation        | Does                                                                                     |
| ----------------- | ---------------------------------------------------------------------------------------- |
| `--statusline`    | render the main bar from the payload on stdin                                            |
| `--subagent`      | render the subagent panel from stdin, as NDJSON                                          |
| `--refresh-spend` | fetch the budget into the cache and exit. Renders nothing                                |
| `--debug`         | the diagnostic report — **and** a modifier on any of the above                           |
| `--version`       | print `X.Y.Z` and exit. Nothing else on stdout, ever                                     |
| `--help` / `-h`   | full usage                                                                               |
| *(nothing)*       | TTY stdin → help; piped stdin → the one-line missing-flag error, see [§1](#1-what-it-is) |

> **Amended 2026-08-19** (`main-bar` cycle). The surface flags are new, per §1.
> `--debug` is now **both a mode and a modifier**, absorbing the `--info` idea
> below: as a mode its report is the output and goes to stdout; as a modifier it
> narrates to stderr and must not change stdout by a single byte.

`--debug` as a modifier narrates decisions on **stderr**. It must compose:
`--version --debug` still prints a bare version; a render with `--debug` still
prints the same bar on stdout. It exists because the spend path is otherwise
completely silent — see §7.

`--debug` as a mode reports the config layers and which resolved, Claude's
wiring as read from `settings.json`, the effective layout, the resolved git
facts, the spend verdict, and a sample render. (This absorbs `--info`, which was
a flag on the `ai-plugins` installer rather than the script; it belongs to
whoever owns the binary, which is now this repo.)

---

## 6. Git resolution

**Filesystem first, subprocess only where unavoidable.** This is a hot path.

- **Root and branch** are read from the filesystem, never from `git`: walk up
  from cwd (bounded, 40 levels) looking for `.git`. If it is a directory, read
  `.git/HEAD`. If it is a *file*, it is a worktree or submodule pointer — parse
  `gitdir: <path>` (may be relative) and read `HEAD` from there. `HEAD` gives
  `ref: refs/heads/<branch>` → the branch, or a detached SHA → first 7 chars.
- **Dirty marker** shells out up to twice: `git diff --numstat HEAD`, falling
  back to `--cached` on a repo with no commits, then
  `git ls-files --others --exclude-standard`. Sum additions and deletions; add
  exactly **1** to additions if the untracked probe is non-empty, however many
  untracked files there are. Then `±` for both, `+` for additions, `-` for
  deletions, empty for clean.
  - Each numstat count is `\d+` or `-`, and the two sides are suppressed
    **independently**. Git reports `-` on both sides for a binary file, so a
    change touching only binaries renders **clean**. That looks like a bug and
    is the shipped behaviour.
  - If the untracked probe fails, the **whole marker is dropped** even though
    numstat succeeded. A partial count would be a quietly wrong number.
- **Ahead marker** shells out once: `git rev-list --count @{upstream}..HEAD`,
  `↑` when > 0. Empty on any error, including no upstream.

Both markers are gated on a resolved **branch**, not on a root: a repo whose
`HEAD` is empty runs no subprocesses at all.

The whole set takes **one shared 250 ms budget**, with the two pipelines on
separate threads.

> **Amended 2026-08-19** (`main-bar` cycle). This said "both subprocesses take a
> 250 ms timeout". There are up to four, and per-subprocess timeouts make the
> worst case ~1 s. One shared deadline across two threads keeps the real budget
> at the 250 ms the invariant in §1 promises.
>
> Note also what the upward walk does with a **broken** `.git`: an `HEAD` that
> cannot be *read* does not stop the walk — it continues to the parent, so a
> nested repo with an unreadable HEAD reports the outer repo. An `HEAD` that
> reads but says nothing useful, or a `gitdir:` pointer that will not parse,
> *does* stop it, with a root but no branch. That asymmetry is faithful to the
> original's try-block scoping and is load-bearing for submodules.

Rust's `std::process::Command` has no built-in timeout. Spawn, move the pipe
into a reader thread, and wait on a channel: a solution that does not drain
stdout can deadlock on a full pipe before its timeout is ever consulted. **Do
not let a slow git hang the bar.**

**Worktree subpath** — split cwd on `/`, find the *last* component matching
`worktreePattern` (default `worktree`, case-insensitive), and take everything
after it. Nothing after it, or no match → not a worktree.

---

## 7. The spend segment — the hard part

Budget it real time. Everything else in this plan is mechanical; this is where
the design decisions are, and every one of them was made for a reason.

### What it shows

The account's monthly budget from claude.ai → Settings → Usage, as
`$75.93/$150 (51%)`. It exists for **team/enterprise** seats, whose limit is a
monthly spend cap rather than the 5-hour and 7-day windows.

### Why a render must never fetch

The figure comes from the OAuth usage endpoint Claude Code's own `/usage` uses.
That endpoint **throttles on accumulated usage** — a tripped account stays 429
for half an hour or more — and the bar can render every few seconds. So:

- A render **reads a cache file** and nothing else.
- When that cache is older than `refreshMinutes`, the render **spawns a detached
  child** (`--refresh-spend`) and draws the cached value immediately. It never
  waits for the child.
- `refreshMinutes: 0` disables the refresh entirely.

### The cache

`~/.cache/claude-status/spend.json`, overridable with
`$CLAUDE_STATUS_SPEND_CACHE` (honour a leading `~`). **Machine-global on
purpose** — one fetch per interval per machine, however many sessions are open.

> **Amended 2026-08-20** (`spend` cycle). The open question below was resolved:
> the path and the env var both took this repo's name, and **no migration was
> written**. An existing `~/.cache/ai-plugins/spend.json` is left in place and
> ignored, so an upgraded install re-fetches exactly once — harmless by the
> contract's own reasoning.
>
> `$AI_PLUGINS_SPEND_CACHE` is **no longer read at all**. Anyone with it
> exported in a shell profile will find it silently ignored.
>
> Only a **leading `~`** expands — unlike `$AI_PLUGINS_USAGE_DIR` in §8, which
> also expands `$HOME` and `${HOME}`. The two are different contracts and this
> document previously conflated them.

```jsonc
{
  "ts": 1787037452146, // when this entry was written
  "plan": "max", // subscriptionType, from the credentials
  "failures": 0, // consecutive failures since the last success
  "backoffUntil": 0, // epoch ms; no refresh before this
  "data": { // null when the account has no budget block
    "usedMinor": 7593,
    "limitMinor": 15000,
    "exponent": 2,
    "percent": 50.6,
    "enabled": true,
  },
}
```

### The refresh child

1. **Take a lock** at `<cache>.lock` — create exclusively (`O_EXCL`). If it
   exists and its mtime is under 2 minutes old, **another refresh is running:
   exit**. Older than that, the holder died: take it over.
2. **Dedupe** — if the cache was written under 60 seconds ago, exit. A sibling
   just ran.
3. **Read credentials** — `~/.claude/.credentials.json` first
   (`claudeAiOauth.accessToken`, plus `subscriptionType` for the plan). On
   macOS, fall back to the keychain:
   `security find-generic-password -s "Claude Code-credentials" -w`. The first
   keychain read may prompt the user once.
4. **Fetch** `https://api.anthropic.com/api/oauth/usage` (overridable with
   `$CLAUDE_STATUS_SPEND_URL`) with `Authorization: Bearer <token>`.
5. **Extract**, in this order:
   - `spend.limit.amount_minor` present → use `spend.used.amount_minor`,
     `spend.limit.amount_minor`, `spend.limit.exponent`, `spend.percent`,
     `spend.enabled`.
   - else `extra_usage.monthly_limit` present → `used_credits`, `monthly_limit`,
     `decimal_places`, `utilization`, `is_enabled`.
   - else → `data: null`. **The account has no budget block.**
6. **Write atomically** — temp file + rename. Release the lock in all paths.

Failure handling:

- **429** → `backoffUntil = now + min(refreshMinutes × 2^failures, 6h)`, keep
  the last good `data`.
- **401** → the token expired. Increment failures, keep the last good data.
- **Network error / no credentials** → increment failures, keep the last good
  data. Never clear a good value because one fetch failed.

### The four gates that hide it

This is the part worth building carefully, because a fully successful refresh
can still render nothing. In order:

1. `spend` is **not in `lines`** → the cache is never even read, and no refresh
   is ever spawned. A user without the segment pays nothing.
2. **No cached `data`** → nothing to show.
3. `data.enabled == false`, or **no `limitMinor`** → the account reports no
   usable budget.
4. `show == "auto"` (the default) and the plan is **not** `team` or `enterprise`
   → hidden. Set `show: "always"` to draw it for Pro/Max extra-usage caps too.

Gate 4 catches most people. On a Max account the figure is fetched and cached
perfectly and then hidden here — and before `--debug` existed, that was
indistinguishable from a broken token.

There is one more subtlety: under `show: "auto"` with an irrelevant plan, the
refresh interval **stretches to 24 hours** rather than `refreshMinutes`. A
Pro/Max machine re-checks its plan daily instead of every quarter-hour.

### What `--debug` must narrate

Every one of the above was a silent `return` in the original. Each now prints:
the cache path and endpoint in use, the prior cache's age/plan/failure count, a
held or stale lock, the 60-second dedupe, where credentials were found (**never
the token itself**), the HTTP status, what was extracted — and, at the end, a
verdict: `WILL RENDER` with the figure, or which gate stops it.

Reproduce the verdict. It is the single most useful line the tool prints.

> **Amended 2026-08-20** (`spend` cycle). `--debug` **as a mode performs a live,
> synchronous, foreground fetch.** It does not merely report the cache, and the
> distinction is the whole reason the subsystem is diagnosable at all:
>
> On a fresh machine the cache **does not exist**. The first render reads
> nothing, therefore draws nothing, and only *then* spawns the detached refresh
> child — whose stdio is `/dev/null`. So run one is *guaranteed* to show no
> budget, and every diagnostic from that first fetch is discarded. A passive
> `--debug` inspecting the cache at that moment could only say "no cache yet",
> which is precisely the useless answer the user already had.
>
> Three consequences, all deliberate:
>
> - It **respects the lock and the backoff but reports them rather than silently
>   obeying** — "a refresh is already running, holder started 14s ago", "in
>   backoff, 28m left — fetching anyway to diagnose".
> - It **bypasses the 60-second dedupe**, because a user typing `--debug` twice
>   wants two answers.
> - It **writes the result to the cache** like any successful refresh, so a
>   `--debug` that works leaves the next render working too. This is the
>   supported fix for a first install that shows no budget.
>
> The token is never printed on stdout or stderr in **any** branch — success,
> 401, 429, no credentials, refused connection, or keychain denial. Only where
> it was found.
>
> As a *modifier* (`--statusline --debug`) this does not apply: a render still
> never fetches, and stdout stays byte-identical.

> **Amended 2026-08-21** (`macos-only` cycle), after review pointed out the
> ordering was real in the code and unwritten here.
>
> **`--debug` fetches even when the user's own gates hide the segment.** The
> four gates in §7 decide whether the figure is *drawn*; they do not decide
> whether it is *fetched*. So `--debug` on a config with `spend` absent from
> `lines` still performs the authenticated request, and then reports
> `gate 1 ✗ HIDDEN`. That is deliberate and is the whole point of the mode: "you
> have it switched off" and "your token is rejected" are different answers, and
> a passive `--debug` could not tell them apart.
>
> **One thing does stop it, and the order matters:** the cache path is resolved
> **first**, and no fetch happens when it is absent (invariant 5). With nowhere
> to write the result there is nothing to diagnose and nothing to keep, so a
> request would spend the account's rate limit to produce nothing.
>
> The corollary for tests: a fake `$HOME` does **not** make `--debug` or the
> refresh path safe, because the macOS keychain is not `$HOME`-scoped. Anything
> that can reach the fetch must pin `$CLAUDE_STATUS_SPEND_URL` **and** seed a
> credentials file, so the file arm answers before the keychain is ever asked.

---

## 8. The usage mirror — a contract with `ai-plugins`

**This is not internal.** It is the reason vwf's context-cap hook can work at
all, and that hook is staying in `ai-plugins`.

Context-window and rate-limit figures arrive **only on the statusline payload**
— never on hook stdin. So every main-bar render mirrors them to a session-keyed
file, which a `PostToolUse` hook then reads.

- Enabled only when `$CLAUDE_STATUS_USAGE_DIR` or `$AI_PLUGINS_USAGE_DIR` is set
  — the new name first, the old one still honoured. Inert otherwise.

  > **Amended 2026-08-21** (`caps-hook` cycle). The variable was
  > `$AI_PLUGINS_USAGE_DIR` alone, and this section said the name does not
  > change. It now migrates: **both** the writer and the reader try
  > `$CLAUDE_STATUS_USAGE_DIR` first and fall back, so a machine still running
  > the JS hook — which only knows the old name — keeps working through the
  > transition. Phase 5 drops the fallback, and with the JS hook gone this
  > binary is both the writer and the only reader, so §8 stops being a
  > cross-repo contract at all.
- Expand a leading `~`, `$HOME` or `${HOME}` in that value — Claude Code may or
  may not have expanded it before exporting.
- Write `<dir>/<session_id>.json`, atomically (temp + rename).
- **Best-effort**: a failure here must never affect the rendered line.

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

> **Keep these field names and this file layout byte-compatible.** A consumer
> lives in another repo. If you want to change the format, that is a coordinated
> change across both, and this document is where the old shape is written down.
> Treat the env var name as part of the contract too — renaming
> `AI_PLUGINS_USAGE_DIR` silently disables vwf's caps.

---

## 9. Distribution — resolved: npm with platform binaries

**Resolved by the distribution cycle, against the recommendation below.** The
options are kept because the reasoning still matters if the channel is ever
revisited.

Today the bar is installed by `pnpx @askviraj/ai-plugins --statusline`, an npm
CLI that copies a JS file to `~/.claude/scripts/statusline` and merges four keys
into `settings.json`. A Rust binary cannot be `pnpx`'d, so this has to change.

| Option                               | For                                                                              | Against                                                               |
| ------------------------------------ | -------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| **GitHub Releases + install script** | Standard for Rust CLIs; no toolchain needed; `curl \| sh` is one line            | You own a shell installer, per-platform builds and a checksum story   |
| **npm with platform binaries**       | Keeps `pnpx`, which existing users already know; the pattern esbuild and swc use | A published package per platform; awkward for a repo with no other JS |
| **`cargo install`**                  | Trivial to publish                                                               | Requires a Rust toolchain — most users will not have one              |
| **Homebrew tap**                     | Great on macOS, one command                                                      | A second channel to maintain; Linux users still need another          |

> **Read this table as of the day it was written.** Every row's reasoning
> assumed the six-target set the paragraph below now strikes through — most
> visibly the Homebrew row, whose "Linux users still need another" stopped being
> an argument against anything on 2026-08-21. The table is kept unedited because
> it is the record of a decision, and a revisited channel should see what was
> actually weighed rather than a tidied version of it.

**The recommendation was** GitHub Releases with prebuilt binaries plus a small
install script that also merges the `settings.json` keys. **The decision went
the other way:** npm with platform binaries — a `@askviraj/claude-status`
wrapper whose `optionalDependencies` carry one package per target, so existing
`ai-plugins` users keep the `pnpx` invocation they already know and the repo
does not have to own a shell installer, a build matrix *and* a checksum story on
day one. A Homebrew tap can still come later.

~~The supported set is **six** targets — macOS and Linux on both architectures,
plus Windows, which Claude Code runs on natively.~~ **Reversed 2026-08-21**
(`macos-only` cycle). The supported set is **two** targets — macOS on both
architectures — and nothing else. Three things the six-target decision could not
weigh at the time:

- **Four of the six were never verified.** `build:cross` proved architecture,
  not execution: no Linux or Windows binary this repo produced was ever *run* by
  anyone. Shipping them was shipping a claim nobody had checked.
- **The C toolchain problem arrived.** The first full build after the spend
  subsystem's TLS stack landed produced four of six, both Windows targets
  failing on a missing archiver. The mitigation was a preflight and a
  `--host-only` flag — two features whose only purpose was making a partial
  build survivable.
- **A platform costs more than a matrix row.** Windows alone was a `cargo-xwin`
  pin, an `llvm-lib` preflight, a `.exe` filename branch, a `USERPROFILE` home
  branch, a `chmod` guard, a parallel `cmd /C` test fixture module and two CI
  runners — for a platform that cannot be tested here.

Nothing had been published when this was decided, so no user lost a platform.
The narrowing is stated at three layers: the wrapper's `"os": ["darwin"]` makes
npm refuse the install, the installer names the host it will not serve before
writing anything, and the readme leads with it.

`supported_targets()` in `.config/mise/tasks/_scripts/_rust` remains the single
source; nothing that can *derive* the list or its length may hard-code it. Four
things cannot derive it, and each is kept in step by hand:

1. `PACKAGES` in `installer/src/modules/binary.ts` — the host→package map.
2. `"os": ["darwin"]` in `npm/claude-status/package.json`.
3. The `build` **and** `test` matrices in `.github/workflows/release.yml`. One
   runner per published target in each: a build matrix that skips a target does
   not ship it, and a test matrix that skips one ships it untested on its own
   architecture. Both are failures, and only the first is currently caught.
4. The crate, where a platform may need a `cfg` that macOS does not.

> **Amended 2026-08-22** (`github-artifacts` cycle). The channel is still npm —
> `npx @askviraj/claude-status --install` is unchanged and is still what a user
> types. What moved is the **bytes**: the binary is a GitHub Release asset that
> `--install` downloads, rather than the payload of one npm package per
> platform. Three published packages become one.
>
> The table above weighed "GitHub Releases + install script" against "npm with
> platform binaries" as alternatives. They were not: this takes the artifact
> half of the first and keeps the entry point of the second, which is why the
> row's "you own a shell installer" cost does not apply — there is no shell
> installer, only the Node CLI that already existed.
>
> **The standard objection does not apply either.** Fetching a binary from an
> npm package is normally a `postinstall` hook, which `--ignore-scripts`
> suppresses and a lockfile cannot vouch for. `--install` is a command the user
> types, which already writes to `~/.claude` and `~/.config`; no package-manager
> setting suppresses it and nothing about it is implicit.
>
> **Integrity is anchored on npm, not on GitHub.** A release asset is mutable —
> it can be deleted and re-uploaded at the same URL — and an npm version is not.
> So `bin/checksums.json` ships inside the package naming every target's asset
> and its SHA-256, and the download is verified against it before anything
> reaches `~/.claude/bin`. A mismatch is fatal and is reported as itself, with
> an explicit instruction not to retry: a mismatch is not a flaky download. The
> trust root therefore does not move, and GitHub is reduced to a bytes-mover.
>
> **Two version lines, deliberately and temporarily.** The tag and `Cargo.toml`
> are the *binary's* version and must agree, which CI enforces. The npm
> package's version is hand-set and is not derived from `Cargo.toml`, so a `0.x`
> installer can be republished while the fetch path is proven without burning
> binary versions on installer bugs. Resolution never keys off the package's own
> version — the manifest names the release, so an older
> `npx @askviraj/claude-status@<old>` installs the binary it was published
> against and never `latest`. When the lines are matched again the manifest
> field simply names the same number and no code changes.
>
> **What this costs.** `--install` was entirely offline and is not any more.
> Air-gapped installs stop working, and the documented fallback is to place the
> binary by hand. Node's `fetch` ignores `HTTPS_PROXY`, so the installer names
> the variable when it sees one set rather than reporting a bare timeout. And
> release ordering inverts: the GitHub Release must exist **before** the npm
> publish, or a published installer points at an asset that is not there.

> **Amended 2026-08-22** (`release-fix` cycle). The published set is **one**
> target: `aarch64-apple-darwin`. Intel macOS is out.
>
> The reason is the ecosystem, not a preference. pnpm — which this repo's own
> task library depends on — publishes `darwin-arm64`, `linux-*` and `win32-*`
> standalone binaries and **no macOS x64 build at any recent version**. CI could
> not install its own tooling on an Intel runner, every binary backend failed
> identically, and no backend swap fixes it. A target whose build tooling has
> abandoned it is a gap this repo would own indefinitely, and it would be owned
> to serve an architecture Apple stopped shipping in 2023.
>
> **Linux was surveyed before being declined**, so that this reads as a choice
> rather than an assumption. It came back viable: `from_keychain()` already
> guards on `cfg!(target_os = "macos")` and falls back to
> `~/.claude/.credentials.json`, so credentials degrade rather than break; the
> crate's platform-specific spots are Unix rather than Apple, and are what
> blocks *Windows*, not Linux; and `ureq` is pinned to rustls with baked roots
> precisely so no `openssl-sys` is in the way. What it costs is two native
> runners, a glibc-versus-musl portability floor, and the end of local complete
> builds — a Mac cross-compiles to another Apple slice with a rustup target and
> cannot produce a Linux binary at all, so `build:all` would stop being able to
> make a releasable set on a maintainer's machine.
>
> The struck-through six-target paragraph below still sets the bar for adding
> any target back, and it is the right one: a native runner that **builds and
> runs the suite** per target. Nothing here lowers it.
>
> With one target, `os` and `cpu` in the npm manifest express the supported set
> **exactly** — no cross product to leak — so npm is the first gate and the
> installer's unsupported-platform message is the second, rather than the only.

> **Amended 2026-08-22** (`release-fix` cycle, second amendment). The binary
> travels **inside** the npm package again. The download-and-verify path added
> earlier the same day is deleted, not disabled.
>
> **The earlier amendment's reasoning was sound for the input it had.** With
> three published packages, fetching bought one npm package instead of three,
> one Trusted Publisher instead of three, and a first publish that did not have
> to reserve three names by hand. Every one of those is a benefit of *not having
> per-platform packages*.
>
> Cutting to one target delivers all of them for free — one target is one
> package whether the binary is inside it or not — so the download's entire
> upside evaporated and only its costs remained: a required network call,
> air-gapped installs broken, `HTTPS_PROXY` unhonoured by Node's `fetch`, a
> release that had to precede the npm publish, and a digest manifest maintained
> against a mutable asset.
>
> **The integrity argument inverts rather than weakens.** The download design's
> central move was pinning digests in the immutable artifact because a release
> asset can be deleted and re-uploaded at the same URL. Embedding makes that
> problem not exist: there is no second artifact to distrust, and npm's own
> immutability is the whole story. `checksums.json`, three distinguishable
> failure modes and a test HTTP server in its own process all disappear rather
> than being maintained.
>
> **Two things from that cycle stay.** One npm package — the per-platform
> packages are gone and stay gone. And the receipt records the **binary's**
> digest, which it never did before, so `--uninstall` applies to the binary the
> same "edited since install" guard it already applied to the config; that was
> an independent fix that merely arrived alongside.
>
> **One version line, again.** `crate_version()` describes itself as *"the
> single source of truth for everything published … deliberately NOT duplicated
> into a package.json that could drift"*. That stopped being true for one cycle,
> when the npm package carried a hand-set `0.x` while the binary was `1.0.0`.
> With the binary inside the package that split made one artifact claim two
> versions of itself, so the manifest is stamped from the crate again and the
> stamp check is back.
>
> The GitHub Release still carries the binary. It is no longer load bearing for
> npm — it is for anyone who wants the binary directly, and for a Homebrew tap
> later.

> **Amended 2026-08-23** (caps as config). The `--caps-hook` thresholds move
> into `claude-status.json` under `caps`, and gain a fourth: `spend`, a percent
> of the account's monthly budget.
>
> **Two things changed at once, and both are loosenings.** The caps used to be a
> constant a repo could only *tighten*, scraped out of `<cwd>/.config/vwf.yaml`
> by a narrow line scan. Now they resolve through the ordinary three layers —
> embedded, user, repo — with the repo layer winning **outright**, and the
> `vwf.yaml` scrape is deleted rather than kept as a second source.
>
> The tighten-only rule existed so a repo could not raise its own limits, and
> the code said so: reversing it *"would let a project silently disable its own
> safety rail, which is the one failure mode of a config-driven cap that nobody
> would notice."* That remains true, and the tradeoff was taken anyway: layer 3
> is a file you commit and review in your own repository, at the same trust
> level as every other setting it already controls, and a caps key that behaved
> differently from its neighbours was a surprise of its own. A user who wants
> the old guarantee gets it by not writing `caps` into a repo config.
>
> **`spend` is a percentage, not an amount**, because a budget is denominated in
> the account's own currency and only a percentage means the same thing on every
> seat. It is evaluated **before** the other three — level `4`, above the 7-day
> `3` — because a rate-limit window empties itself on a timer while an exhausted
> budget needs somebody to act.
>
> Its figure does **not** come from the usage mirror, which a render writes from
> the payload and which carries no spend data. It comes from the spend cache the
> refresh child maintains, read as a local file. The hook still never fetches,
> exactly as a render never does. A seat with no budget block yields `None`,
> which never breaches — not even against a cap of `0`.
>
> Defaults are unchanged where they existed (context 65, five-hour 90, seven-day
> 80) and `spend` ships at 90. A key that is absent, negative, non-numeric or
> absurd falls back to its shipped default rather than being clamped, so a
> config that failed to make sense behaves like one that was never written. `0`
> is a real cap meaning "breach on any usage at all", and is not mistaken for
> unset.

Whatever the channel, **the installer must keep the receipt discipline** the
current one has: record what was there before, so an uninstall *restores* the
`settings.json` keys it overwrote — the user's previous bar comes back rather
than being deleted and leaving them with none. Files it wrote are removed, not
restored to some earlier name; the two are different obligations. And replacing
a statusline the installer did not write must require explicit consent.

---

## 10. Build phases

Each phase ends at something runnable. Do not start the next until the current
one's verification passes.

### Phase 1 — render the main bar

Config loading and the two-layer merge; colour resolution; the powerline
renderer; every segment except `spend`; the formatting helpers; git resolution.

*Verify:* piping the fixture payload (§11) produces a bar byte-identical to the
current implementation's, modulo the spend segment. Compare with `diff`, not by
eye.

### Phase 2 — the subagent panel

Status matching, type glyphs, the description budget, NDJSON output.

*Verify:* the subagent fixture round-trips; `| jq -r .content` renders a sane
row; a task without an `id` is skipped rather than crashing.

### Phase 3 — the spend subsystem

The cache, the lock, the detached child, credential reading, the fetch, the
extraction ladder, backoff, the four gates, and `--debug`.

*Verify:* against a **stub HTTP server**, not the real endpoint — exercise 200
with a `spend` block, 200 with `extra_usage`, 200 with neither, 401, 429, and a
connection refusal. Then one real `claude-status --debug` to confirm the
credential path works on a live machine.

### Phase 4 — distribution

The npm packages §9 resolved on, plus `--version` and the installer.

*Verify:* install into a throwaway `$HOME` (`mktemp -d`, with `HOME`,
`XDG_CONFIG_HOME` and `XDG_DATA_HOME` redirected), render with the *installed*
binary, then uninstall and confirm the tree is byte-identical to before.

### Phase 5 — cut over

Update `ai-plugins` to stop shipping `tools/statusline/` and point its docs
here. `context-caps.js` goes with it — the `caps-hook` cycle moved that actuator
into this binary as `--caps-hook`, so the installer replaces the
`node …context-caps.js` command with `claude-status --caps-hook` rather than
leaving a Node hook behind.

---

## 11. Testing

Three layers, and the middle one is the one people skip:

1. **Unit** — the formatting helpers, colour resolution, config merge, the
   timestamp normaliser, the spend extraction ladder and the gate logic. All
   pure; all cheap.
2. **Golden renders** — a fixture payload in, an exact expected ANSI string out.
   This is what catches a separator regression or an off-by-one in the gauge,
   and nothing else will. Keep the fixtures small and readable.
3. **End-to-end** — run the built binary as a subprocess with a fake `$HOME`,
   which is how Claude Code invokes it.

Two hazards to design around from the start:

- **The keychain is not scoped by `$HOME`.** A test with a fake home can still
  trigger a real credential read and a real network call if a stale cache spawns
  a refresh. Zero `refreshMinutes` in test configs, point
  `$CLAUDE_STATUS_SPEND_URL` at a closed port or a stub, **and seed a
  `.claude/.credentials.json` inside the fake home** — without it the keychain
  fallback reaches the developer's real token.
- **Making a previously inert path live can turn a passing test into a
  live-fetch test, silently.** When a `match` arm stops being a stub, re-audit
  the *unit* tests too, not just the integration ones — an in-process test that
  calls the dispatcher is one arm away from a real request, and nothing about it
  will look different when it starts fetching.
- **Assert that no diagnostic reaches stdout.** Capture both streams separately
  and check stdout holds only the bar. This is the invariant most likely to
  regress and least likely to be noticed.

## 12. Reference fixtures

```sh
# main bar
echo '{"model":{"display_name":"Opus 4.8"},"effort":{"level":"high"},"session_name":"users-and-groups","workspace":{"current_dir":"/tmp/demo"},"cost":{"total_cost_usd":46.51,"total_duration_ms":33540000},"context_window":{"used_percentage":26,"context_window_size":1000000,"total_input_tokens":259000},"rate_limits":{"five_hour":{"used_percentage":7,"resets_at":1774200000},"seven_day":{"used_percentage":1.0,"resets_at":1774600000}}}' | claude-status

# subagent panel — NDJSON, so pipe through jq to see it. The FLAG chooses the
# surface, not the payload's shape: without it this renders the main bar.
echo '{"columns":120,"tasks":[{"id":"t1","name":"reviewer","type":"review","status":"running","description":"Auditing auth flow","tokenCount":18234}]}' | claude-status --subagent | jq -r .content

# spend, diagnosed — NOTE: this FETCHES, live, in the foreground.
# Both overrides matter: the cache one keeps it off ~/.cache/claude-status,
# and without the URL one it hits the real endpoint with your real token.
CLAUDE_STATUS_SPEND_CACHE=/tmp/spend.json \
  CLAUDE_STATUS_SPEND_URL=http://127.0.0.1:1/never \
  claude-status --debug

# spend, against the real endpoint — one request, and it writes the real cache
claude-status --debug
```

## 13. What not to port

- ~~**`context-caps.js`** — stays in `ai-plugins`. It is vwf policy, not
  statusline behaviour. This repo's obligation to it is §8 and nothing more.~~
  **Reversed 2026-08-21** (`caps-hook` cycle). The ownership argument is sound
  and never addressed the performance one: the hook is wired as
  `node ${HOME}/.claude/hooks/context-caps.js` on `PostToolUse`, so it paid
  Node's startup after **every tool call** — measured at **28.6 ms** against
  this binary's **2.8 ms** for the same work. It is now `--caps-hook`, one more
  mode on the same binary. vwf still owns the *policy*: the caps, the thresholds
  and the directive wording live in `vwf.yaml` and in the vwf skills, and this
  binary only actuates them.
- **The four retired render targets** (OpenCode, Oh-My-Pi, Cursor) and every
  trace of them. Cursor never had a status surface; the other two are
  discontinued.
- **The npm receipt/uninstall machinery**, unless §9 lands on npm. The
  *discipline* (record the prior state of anything you overwrite, and restore it
  rather than leaving a default in its place) is worth keeping; the
  implementation is not.

## 14. Before you write code

- **Verify every crate choice against current documentation** — Context7 or
  docs.rs — rather than trusting a suggestion. This document deliberately names
  none: the right JSON, HTTP and timeout crates as of your build date are for
  you to check, not for a plan written earlier to assert.
- **Consider avoiding an async runtime entirely.** There is exactly one network
  call, it happens in a detached child, and it may block freely. Pulling in a
  full async runtime for it would work against the startup-time argument that
  motivates this rewrite.
- **Read the original before reimplementing anything subtle.** The spend
  subsystem, the git resolution and the powerline seam logic all encode
  decisions this document summarises but does not fully justify. Source:
  `tools/statusline/statusline` in `virajp/ai-plugins`, plus
  `docs/plugins/statusline.md` in that repo, which documents the behaviour and
  the traps.
