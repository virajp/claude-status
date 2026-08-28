---
type: vwf-plan-backlog
title: Backlog — claude-status
description: Ideas accepted but not yet planned. Each entry is a candidate
  slice, not a commitment, and carries what is already true so a plan does not
  re-derive it.
status: stable
---

# Backlog

Things worth doing, not yet cut into a cycle. An entry graduates by becoming a
plan doc in [`docs/plans/`](./index.md) — until then nothing here is a promise,
and the "already true" notes are the point: they are what stops a future plan
from re-discovering the same ground.

## Pending

**Three of these four came out of finished cycles, not from anybody raising
them.** The fourth — the flaky test — came out of a release it broke. Each was
deferred deliberately inside a plan that then closed `status: done`, which is
exactly how they became invisible: nothing is half-built, no cycle is waiting,
and a reader checking this file on 2026-08-27 would have been told there was
nothing pending. Moved here so the next planning pass sees them.

**Each entry carries its evidence rather than linking to the plan it came
from.** Those plans were archived on 2026-08-27 into `docs/plans/archived/`,
which is **gitignored** — in a fresh clone there is nothing to link to, and a
backlog whose reasoning lives behind a dead link is a backlog that gets
re-derived. Commit shas are given instead: they are the durable pointer, the
same convention [`index.md`](./index.md) uses for archived cycles.

### The config generator's bar preview — and it is WebAssembly, not JavaScript

**Deferred from** `2026-08-23-website/02-config-generator` (archived; landed in
`f6b22c4`), section *"The cycle was cut: steps 4 and 5 are deferred, and so are
criteria 4, 5 and 6"*. Steps 4 and 5 were cut with acceptance criteria 4, 5 and
6. Nothing partial was left behind — the page says plainly that it does not draw
a bar and sends the reader to `--debug`.

**Already true, and measured rather than argued** — a recon pass in that cycle
built it before deferring it:

- The render path is **already pure**:
  `render_main(&MainFacts, &GitFacts, &Config, Option<&str>) -> String`
  (`src/modules/render/main_bar.rs`) touches no IO.
- It **compiles to `wasm32-unknown-unknown` today** with a raw ABI — no
  `wasm-bindgen`, no JavaScript toolchain, nothing that would put npm back in
  the tree `distribution/01` took it out of.
- It reproduced `tests/golden/fixture.txt` **byte for byte** (669 == 669) from a
  browser-shaped build.
- **247 KB raw, 86 KB gzipped** — four times under the repo's 1024 KB commit
  limit.
- The blocking diff is small and none of it is on the render path: `ureq` in one
  file (`src/modules/spend/http.rs`) and eleven Unix-trait errors across three.

**So the deferred criteria changed shape.** Criteria 4 and 5 were written for a
hand-written JavaScript port that could drift from the Rust; a wasm build of the
same code cannot drift, which makes them **vacuous rather than hard**. Do not
plan the JS port — it is the thing that was ruled out, not the thing that is
waiting.

**Caveat added 2026-08-27:** the `ureq` blocker got slightly larger. It now
carries the `platform-verifier` feature, so the dependency to cut out of a wasm
build pulls `rustls-platform-verifier` and `security-framework` with it. It is
still one file.

### Code signing and notarisation

**Deferred from** `distribution/§9`, restated as still-unowned in the archived
`2026-08-23-distribution/02-homebrew-formula` (`ee6f939`) and `03-release`
(`e990ab4`), and — the copy that survives in the repo — in
[`docs/decisions.md`](../decisions.md#still-unowned-code-signing-and-notarisation).

**Already true:** a brew-installed binary is downloaded rather than built, the
same as npm's was — but **"brew installed it" makes people expect it is signed,
and it is not**. The practical cost shows up whenever a binary is moved between
machines by hand: Gatekeeper quarantines it and the recipient needs
`xattr -d com.apple.quarantine` before it will run, which is not something to
put in front of a user. Nothing about this changed in v1.1.0.

### Linux targets

**Deferred rather than rejected** in the archived
`2026-08-23-distribution/index.md`, under *Platforms*. The evaluation is
reproduced in full below rather than pointed at, because that folder is now
gitignored and the whole purpose of the note was that nobody re-derives it.
`supported_targets()` still has one row.

**Already true:** the technical cost is low — every dependency is pure Rust,
home resolution is `$HOME`, and the keychain is a capability check that falls
back to `~/.claude/.credentials.json`. **The blockers are not technical:**
nobody has confirmed Claude Code on Linux writes that credentials file (if it
does not, the spend segment silently never renders), and Homebrew serves Linux
poorly enough that supporting it properly means a second channel. Revisit with
evidence.

**Correction, 2026-08-27:** that evaluation said "TLS is already `rustls` with
baked roots". **This is no longer true and the change is load-bearing for a
Linux port.** The spend fetch now verifies against the OS trust store via
`rustls-platform-verifier` — see
[the decision](../decisions.md#the-spend-fetch-trusts-the-os-not-a-baked-root-set).
That crate does support Linux, but it resolves roots there through the system
certificate store rather than the macOS keychain, so "TLS needs nothing" is a
claim a Linux plan must re-check rather than inherit.

---

### The release-blocking flaky test

**Surfaced by** the archived `2026-08-27-npm-installer` cycle, which did not
cause it and did not fix it. Raised here because it **cost a release**, not
because anyone was looking for it.

`_shared::proc::tests::the_deadline_is_shared_not_per_command` gives a
`Deadline::in_ms(300)` to a `sleep 0.2`, so **100 ms covers process spawn**. On
`v1.1.2`'s release run a shared macOS runner lost that race, `test` went red,
and `publish`, `bump-tap` and `publish-npm` were all skipped. A re-run passed
with no code change.

**Already true, and measured rather than argued:**

- **It predates the cycle that found it.** `src/_shared/proc.rs` is untouched by
  all seven of that cycle's commits.
- **The cost is not the minute it wastes.** A red release run reads as "the
  release failed" — the tag is pushed, the version is burned, and the person
  looking has to read a log to learn it was a coin flip. On a tag that had
  already published, that reading is wrong in the expensive direction.
- **It is one of three timing tests in that module**, and the neighbours are not
  equivalent: `a_large_output_does_not_deadlock_on_a_full_pipe` asserts a kill
  happened and `an_already_expired_deadline_spawns_nothing` uses a zero deadline
  — neither races a real sleep against a real budget.

**What it is actually testing is worth keeping**: that a `Deadline` is shared
across commands rather than restarting per command. That is a property of the
arithmetic, not of the wall clock, and the open question is whether it can be
asserted without spawning anything at all — which would remove the flake rather
than widen it.

**Do not just raise the deadline.** A wider margin is a slower test that fails
less often; it is still a wall-clock race on shared hardware, and the next
runner contention finds it again.

## Graduated

**The four entries that were here have all graduated.** They were cut into plans
on 2026-08-23, ran, and were archived on 2026-08-27 — so the sha is the durable
pointer, not a path.

| Was                                 | Became                                    | Landed    |
| ----------------------------------- | ----------------------------------------- | --------- |
| `$schema` in every generated config | `config-and-cli/04-schema-and-validation` | `83859e6` |
| Config validation in `--debug`      | `config-and-cli/04-schema-and-validation` | `83859e6` |
| A Homebrew formula                  | `distribution/02-homebrew-formula`        | `ee6f939` |
| Remove the npm installer            | `distribution/01-drop-npm`                | `fd8d4de` |

Planning turned up more than the entries asked for, and the extra plans are
worth knowing about because no backlog entry predicted them:

- **There were no Rust config types** to hang a schema or a validator off, so
  `config-and-cli/01-typed-config` (`6eefd79`) exists to create them.
- **Retiring the installer stopped being a port and became a deletion**, once
  `--configure` moved into the binary (`config-and-cli/03-cli-surface`,
  `4556946`) and it was settled that nothing needs migrating.
- **A website was commissioned** — `2026-08-23-website/` — which is now part of
  the install flow rather than marketing, since both the formula's caveats and
  `--help` point at it.

## Closed

Kept as a record of what was raised and where it went, so a later reader does
not re-open any of them.

- **Nothing seeded the repo-level config layer.** Shipped as the installer's
  `--configure` (`d97dc4b`), with the binary growing a matching autoseed on the
  render path (`60f5c0f`) so a repo gets its layer without anyone running a
  command. It answered its own open questions: the **installer** owns the flag,
  not the binary; an existing `claude-status.json` is kept and only gains a
  missing `projectName`; a lone `statusline.json` is rewritten rather than
  renamed (`c42aabc`) with its `$schema` repointed; the name is the directory
  basename, not the git remote; and it honours `--dry-run`. `projectName` became
  repo-level only with `autoConfigureRepo` defaulting on (`03f6773`).

- **An unresolvable `$HOME` had no defined meaning.** Raised by security review
  during `macos-only`, deferred, then fixed in that same cycle. The contract now
  says absent-never-relative and the four callers agree with it.

- **Path-derived segment text reached the row unfiltered.** Same origin, same
  outcome: filtering now sits at the single point every segment's text passes
  through.
