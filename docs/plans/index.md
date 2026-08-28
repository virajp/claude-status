---
type: vwf-plan-index
title: Cycle plans — claude-status
description: One row per cycle plan, its target repo, what it requires, what has
  landed, and its status.
status: stable
---

# Cycle plans

The Rust rewrite of the Claude Code statusline. Each plan is a diff against the
desired state, and each ends at something runnable.

That desired state used to be one behaviour contract. The `spec-retirement`
cycle split it by what each part is *for* — the site describes the product,
[the decision record](../decisions.md) carries the reasoning, and behaviour is
pinned by the suite — because a single document restating what the tests already
held is the thing that drifted.

## Active

**None.** `npm-installer` landed on 2026-08-27 and was archived the same day —
it is listed under [Archived](#archived) below.

The four folders before it — `config-and-cli`, `distribution`, `website` and
`spec-retirement`, ten plans between them — all landed and were archived on
2026-08-27 too.

`retire-the-spec` was always last, because every other plan wrote into the
document it deletes. It ran on 2026-08-26/27 and the document is gone; what it
carried is split across the site, [the decision record](../decisions.md),
[the usage-mirror contract](../usage-mirror-contract.md) and `tests/fixtures/`.

What is not yet cut into a cycle lives in [`backlog.md`](./backlog.md) — **three
entries**, moved there on 2026-08-27 out of cycles that had already closed
`done`: the config generator's wasm bar preview, code signing and notarisation,
and Linux targets. The backlog read "currently empty" until then, which was true
of what anyone had *raised* and false of what had been *deferred*. Its entries
carry their own evidence rather than linking into the archive, for the reason
the Archived section gives.

### Cross-folder dependencies

Two, and both are easy to miss because the folders read as independent:

- **`distribution/01` requires `config-and-cli/03`.** It deletes the npm
  installer, which is the only thing that can wire Claude Code until
  `--configure` exists.
- **`website/02` requires `config-and-cli/04`.** The form is built from the
  generated schema.

The soft ordering recorded here — that `website/01` should land before
`distribution/02` so the formula's caveats do not link a site that does not
exist — was overtaken once and is now **resolved**.

It was recorded here as a live defect: both website plans had landed and
`claude-status.virajp.dev` still did not resolve, because the Pages project and
DNS record were never created. That is no longer true. The project exists
(`claude-status-virajp-dev`, direct upload), and it serves both
`claude-status.virajp.dev` and `claude-status-site.pages.dev`. The formula's
caveats pointed at the `pages.dev` name, which was the interim this note
proposed.

**Repointed 2026-08-27.** The formula's `homepage` and caveats now give
`claude-status.virajp.dev`, which was measured serving 200. The guard in
`tests/release.rs` that *forbade* the vanity domain — written when the DNS
record did not exist — is inverted: it now forbids the `pages.dev` interim,
because both addresses serve the same pages and **two addresses is how one of
them stops being maintained without anybody noticing**.

Half of the note this replaces was wrong: `cli.rs`'s help text already gave the
vanity domain, with a test pinning it. Only the formula was behind.

**Closed by `v1.1.0` on 2026-08-27.** The tap's copy was the old one until a
release rendered it — the formula there is generated per release and cannot be
edited in place, which is the property that stops it drifting, and is also why
this sat open. `bump-tap` has now run: the published formula gives
`homepage "https://claude-status.virajp.dev"`, and `pages.dev` appears in it
zero times.

## The direction these replace

An earlier set of five plans, written the same day, was scrapped rather than
amended. It assumed npm stayed the channel, the installer was ported to Rust
rather than deleted, the config kept its current shape and paths, and there was
no website. Enough of that changed at once that editing would have left five
documents arguing with themselves.

**Their findings were harvested, not lost** — the accessor coercion table, the
`settings.json` key asymmetry, the tap and formula reasoning, the
strict/permissive validation split, and the release pipeline's shape all carried
into the new set. Git history holds the originals.

## Archived

Twenty-two cycles, all executed. They live on disk under `docs/plans/archived/`
and are **gitignored** — the record is kept locally, out of the repo. Nothing
below links, because in a fresh clone there is nothing to link to; the merge
commit is the durable pointer.

### Archived 2026-08-27 — npm-installer

One plan, `status: done`. Seven commits, all pushed straight to `main`, so each
carries a plain sha rather than a merge.

| Plan                                        | Requires | Landed                         |
| ------------------------------------------- | -------- | ------------------------------ |
| `2026-08-27-npm-installer/01-npm-installer` | —        | `0648e9a`, then six follow-ups |

The follow-ups are the cycle, not afterthoughts: `372d2ee` (the `EALLOWGIT`
fix), `fbb20a9` (the rename to `@virajp.dev`), `cc27fce` (a readme in the
tarball), `3d32fc4` (images to the site, `_headers` made precise), `f788df6`
(`v1.1.2`) and `f59b8dd` (`v1.1.3`).

**Four of the five gaps it recorded came from one blind spot** — the plan
verified the installer's logic and never its publish, and `npm publish` is not
something this repository can execute. The archived plan carries them.

### Archived 2026-08-27 — the four 2026-08-23 folders

Ten plans, all `status: done` before they moved. Rows are in execution order
within each folder.

| Plan                                                 | Requires            | Landed              |
| ---------------------------------------------------- | ------------------- | ------------------- |
| `2026-08-23-config-and-cli/01-typed-config`          | —                   | `6eefd79`, no merge |
| `2026-08-23-config-and-cli/02-config-relocation`     | 1                   | merged in `586134e` |
| `2026-08-23-config-and-cli/03-cli-surface`           | 2                   | merged in `4556946` |
| `2026-08-23-config-and-cli/04-schema-and-validation` | 3                   | merged in `83859e6` |
| `2026-08-23-distribution/01-drop-npm`                | config-and-cli 3    | merged in `fd8d4de` |
| `2026-08-23-distribution/03-release`                 | 1                   | merged in `e990ab4` |
| `2026-08-23-distribution/02-homebrew-formula`        | 3                   | merged in `ee6f939` |
| `2026-08-23-website/01-site`                         | —                   | merged in `ce3126c` |
| `2026-08-23-website/02-config-generator`             | 1, config-and-cli 4 | merged in `f6b22c4` |
| `2026-08-23-spec-retirement/01-retire-the-spec`      | all nine above      | `8471678`, no merge |

**Two landed without a merge commit**, which is why they carry a plain sha:
`typed-config` and `retire-the-spec` were pushed straight to `main`. For
`cli-surface` the follow-up scan fix is `fca2aa2`; for `release` the work spans
`10796c4`, `d1cf9b0`, `91d4237` and `57ee92f` before `e990ab4` completes it.

**`distribution` rows are in execution order 1 → 3 → 2.** The two plans each
declared the other a prerequisite; a formula cannot pin a digest that does not
exist, so the release went first.

### The twelve before them

| Plan                                | Requires                         | Landed                    |
| ----------------------------------- | -------------------------------- | ------------------------- |
| `2026-08-19-1400-main-bar`          | —                                | merged in `7cb6247`       |
| `2026-08-19-1401-subagent-panel`    | main-bar                         | merged in `8728158`       |
| `2026-08-19-1402-spend`             | main-bar                         | merged in `d68baf6`       |
| `2026-08-19-1404-caps-hook`         | main-bar                         | merged in `61a5445`       |
| `2026-08-19-1403-distribution`      | subagent-panel, spend, caps-hook | merged in `33d6abc`       |
| `2026-08-21-2121-macos-only`        | —                                | merged in `e1d6125`       |
| `2026-08-22-2147-github-artifacts`  | —                                | `68c2c21`, since reversed |
| `release-fix/01-apple-silicon-only` | github-artifacts                 | `2474d51`                 |
| `release-fix/02-embed-the-binary`   | 01                               | `4f9dc93`                 |
| `release-fix/03-mise-consolidation` | —                                | `ed93a7c`                 |
| `release-fix/04-readme-for-npm`     | 02                               | `3288e9c`                 |

Filename timestamps record when each plan was written, not when it ran.

**Three acceptance criteria were archived open.** Two — one binary per platform
from a real publish, and a machine with no toolchain running it — were carried
into `distribution/03`, restated for Homebrew, and **closed by the releases
themselves**: `v0.1.0` then `v1.0.0` published the assets, and `v1.1.0` proved
the tarball installs and runs. The third, `macos-only`'s `EBADPLATFORM` proof,
**died with npm** and is closed as no longer applicable.

## What has shipped

**This section said "Nothing has shipped" until 2026-08-27.** It was written
before `distribution/03` ran and was never updated as the releases went out.

- **`v0.1.0`**, then **`v1.0.0`** (`e990ab4`) — the first real assets.
- **`v1.1.0`** (`a4d9d5b`) — the OS-trust-store fix for the spend fetch, the
  Node 24 action bump, and the release-pipeline repairs that took three attempts
  to land. Its `bump-tap` run is what finally repointed the Homebrew formula at
  `claude-status.virajp.dev`.
- **`v1.1.1`, `v1.1.2` and `v1.1.3`** — the `npm-installer` cycle. `v1.1.1` is
  the release the npx channel's first, hand-made npm publish was cut from;
  `v1.1.2` is where Trusted Publishing first minted its own credential, and
  `v1.1.3` is the first release where it did so unattended and on the first
  attempt.
- **`site-v3`** — the site began serving `/media/`, which is where the npm
  readme's images live.
- **Three channels, not one.** This entry said "Homebrew is the channel; pnpm is
  retired" until 2026-08-27, and both halves are now out of date. `brew`, `mise`
  and `npx @virajp.dev/claude-status --install` all work.
  **`@askviraj/claude-status` stays unreclaimed** — the npx channel publishes
  under a different scope, so the `0.0.1` placeholder that entry describes was
  never the thing that shipped.

**One target.** `supported_targets()` has one row, `aarch64-apple-darwin`. Linux
was evaluated and **deferred, not rejected**; the evaluation is preserved in
[`backlog.md`](./backlog.md) — including the correction that its "TLS is already
`rustls` with baked roots" premise stopped being true in `v1.1.0`. Do not
re-derive it.

## Not planned here

**Phase 5 — the `ai-plugins` cutover.** Removing `tools/statusline/` from
`virajp/ai-plugins` and pointing that repo's docs at this one. A different
repository, gated on a shipped release.

The [usage mirror](../usage-mirror-contract.md) and `$AI_PLUGINS_USAGE_DIR` are
**explicitly unchanged** by every plan in the tree — it is a live contract with
that repo.

**Code signing and notarisation.** Still unowned, and recorded as such in the
[decision record](../decisions.md#still-unowned-code-signing-and-notarisation).
More visible with a Homebrew tap, since people expect a brew-installed binary to
be signed, and it is not.

**Linux.** See above.
