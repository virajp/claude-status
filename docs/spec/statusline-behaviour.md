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

### Three invariants that outrank everything else

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
> preserving the user's theming, and `--uninstall` puts it back. The binary only
> ever knows the new name, so no per-render stat is spent on a legacy path.
> Until the Phase 5 cutover **both files exist on purpose** — the JS bar is
> still live and still reads the old name. Neither is stale.

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

`~/.cache/ai-plugins/spend.json`, overridable with `$AI_PLUGINS_SPEND_CACHE`
(honour a leading `~`). **Machine-global on purpose** — one fetch per interval
per machine, however many sessions are open.

> Pick the new path deliberately. `~/.cache/claude-status/spend.json` is the
> right name for this repo, but the env var and path are a compatibility
> surface. Decide whether to migrate an existing cache or just let it re-fetch
> (re-fetching is harmless — one request).

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
   `$AI_PLUGINS_SPEND_URL`) with `Authorization: Bearer <token>`.
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

---

## 8. The usage mirror — a contract with `ai-plugins`

**This is not internal.** It is the reason vwf's context-cap hook can work at
all, and that hook is staying in `ai-plugins`.

Context-window and rate-limit figures arrive **only on the statusline payload**
— never on hook stdin. So every main-bar render mirrors them to a session-keyed
file, which a `PostToolUse` hook then reads.

- Enabled only when `$AI_PLUGINS_USAGE_DIR` is set. Inert otherwise.
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

## 9. Open decision: distribution

**Resolve this before Phase 4; it does not block Phases 1–3.**

Today the bar is installed by `pnpx @askviraj/ai-plugins --statusline`, an npm
CLI that copies a JS file to `~/.claude/scripts/statusline` and merges four keys
into `settings.json`. A Rust binary cannot be `pnpx`'d, so this has to change.

| Option                               | For                                                                              | Against                                                               |
| ------------------------------------ | -------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| **GitHub Releases + install script** | Standard for Rust CLIs; no toolchain needed; `curl \| sh` is one line            | You own a shell installer, per-platform builds and a checksum story   |
| **npm with platform binaries**       | Keeps `pnpx`, which existing users already know; the pattern esbuild and swc use | A published package per platform; awkward for a repo with no other JS |
| **`cargo install`**                  | Trivial to publish                                                               | Requires a Rust toolchain — most users will not have one              |
| **Homebrew tap**                     | Great on macOS, one command                                                      | A second channel to maintain; Linux users still need another          |

**Recommendation:** GitHub Releases with prebuilt binaries for
`aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu` and
`aarch64-unknown-linux-gnu`, plus a small install script that also merges the
`settings.json` keys. Add a Homebrew tap later if it is wanted.

Whatever is chosen, **the installer must keep the receipt discipline** the
current one has: record what was there before, so an uninstall *restores* the
user's previous bar rather than deleting it and leaving them with none. And
replacing a statusline the installer did not write must require explicit
consent.

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
connection refusal. Then one real `--refresh-spend --debug` to confirm the
credential path works on a live machine.

### Phase 4 — distribution

Whatever §9 resolves to, plus `--version` and the installer.

*Verify:* install into a throwaway `$HOME` (`mktemp -d`, with `HOME`,
`XDG_CONFIG_HOME` and `XDG_DATA_HOME` redirected), render with the *installed*
binary, then uninstall and confirm the tree is byte-identical to before.

### Phase 5 — cut over

Update `ai-plugins` to stop shipping `tools/statusline/`, point its docs here,
and keep `context-caps.js` reading the same usage file.

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
  a refresh. Zero `refreshMinutes` in test configs, and point
  `$AI_PLUGINS_SPEND_URL` at a closed port or a stub.
- **Assert that no diagnostic reaches stdout.** Capture both streams separately
  and check stdout holds only the bar. This is the invariant most likely to
  regress and least likely to be noticed.

## 12. Reference fixtures

```sh
# main bar
echo '{"model":{"display_name":"Opus 4.8"},"effort":{"level":"high"},"session_name":"users-and-groups","workspace":{"current_dir":"/tmp/demo"},"cost":{"total_cost_usd":46.51,"total_duration_ms":33540000},"context_window":{"used_percentage":26,"context_window_size":1000000,"total_input_tokens":259000},"rate_limits":{"five_hour":{"used_percentage":7,"resets_at":1774200000},"seven_day":{"used_percentage":1.0,"resets_at":1774600000}}}' | claude-status

# subagent panel — NDJSON, so pipe through jq to see it
echo '{"columns":120,"tasks":[{"id":"t1","name":"reviewer","type":"review","status":"running","description":"Auditing auth flow","tokenCount":18234}]}' | claude-status | jq -r .content

# spend, narrated
AI_PLUGINS_SPEND_CACHE=/tmp/spend.json claude-status --refresh-spend --debug
```

## 13. What not to port

- **`context-caps.js`** — stays in `ai-plugins`. It is vwf policy, not
  statusline behaviour. This repo's obligation to it is §8 and nothing more.
- **The four retired render targets** (OpenCode, Oh-My-Pi, Cursor) and every
  trace of them. Cursor never had a status surface; the other two are
  discontinued.
- **The npm receipt/uninstall machinery**, unless §9 lands on npm. The
  *discipline* (record prior state, restore rather than delete) is worth
  keeping; the implementation is not.

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
