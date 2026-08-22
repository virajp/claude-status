---
type: vwf-plan-index
title: release-fix — 2026-08-22
description: Three ordered plans that cut the published set to Apple Silicon
  alone, put the binary back inside the one npm package, and collapse the mise
  tool split so dev and CI resolve the same toolchain.
status: active
covers: [
  docs/spec/statusline-behaviour.md,
]
supersedes: [
  docs/plans/2026-08-22-2147-github-artifacts.md,
]
timestamp: 2026-08-22T17:30:00Z
tags: [ distribution, targets, npm, mise, ci ]
---

# release-fix — 2026-08-22

| # | Plan                                             | What it changes                                                           |
| - | ------------------------------------------------ | ------------------------------------------------------------------------- |
| 1 | [apple-silicon-only](./01-apple-silicon-only.md) | one published target; Intel Mac and the never-shipped Linux set both out  |
| 2 | [embed-the-binary](./02-embed-the-binary.md)     | the binary ships **inside** the npm package; the download path is unwound |
| 3 | [mise-consolidation](./03-mise-consolidation.md) | one tool list for dev and CI, same rust profile                           |
| 4 | [readme-for-npm](./04-readme-for-npm.md)         | `readme.md` becomes the package listing page, not a repo doc              |

**Order is load bearing.** Plan 2's whole justification is that plan 1 has
already happened: embedding is only obviously right once there is exactly one
binary to embed. Running 2 first would be arguing for a fat package while the
set is still plural. Plan 4 comes after 2 because plan 2 deletes the readme's
download, network and proxy passages — rewriting the file first would mean
editing paragraphs that are about to be removed. Plan 3 is independent and could
run any time, except that its verification is "CI goes green", which wants the
matrix plan 1 settles.

## Why this reverses last cycle, and why that is not churn

`2026-08-22-2147-github-artifacts` moved the binary out of npm and onto a GitHub
Release, fetched and verified at install time. It shipped, it works, and it is
now being undone. That is worth being honest about rather than quietly
rewriting.

**The reasoning was sound for the input it had.** With three published packages,
the download genuinely bought something: one npm package instead of three, one
Trusted Publisher registration instead of three, and a first publish that did
not have to reserve three names by hand. Every one of those benefits is a
benefit *of not having per-platform packages*.

**Cutting to one target delivers all of them for free.** One target means one
package whether the binary is inside it or not. So the download's entire upside
evaporates, and what remains is only its costs:

|                        | download                                         | embedded               |
| ---------------------- | ------------------------------------------------ | ---------------------- |
| npm packages           | 1                                                | 1                      |
| Trusted Publishers     | 1                                                | 1                      |
| package size           | 15.5 KB + 1.0 MB fetched                         | ~1.0 MB                |
| network at `--install` | **required**                                     | none                   |
| air-gapped install     | **broken**                                       | works                  |
| `HTTPS_PROXY`          | **not honoured by Node's fetch**                 | not applicable         |
| integrity story        | manifest pins a digest; verify the mutable asset | npm's own immutability |
| release ordering       | GitHub Release **must** precede npm publish      | none                   |

The integrity row is the sharpest. The download design's central move was
pinning digests in the immutable artifact because *"a release asset is mutable —
it can be deleted and re-uploaded at the same URL"*. Embedding makes that
problem not exist: there is no second artifact to distrust. A whole mechanism —
`checksums.json`, digest verification, three distinguishable failure modes, a
test HTTP server in its own process — disappears rather than being maintained.

**What survives from that cycle**, and should not be reverted with the rest:

- **One npm package.** The per-platform packages, `platform.template.json`, the
  `optionalDependencies` generation and the pin check are gone and stay gone.
- **The receipt records the binary's digest.** It never did before, so
  `--uninstall` could not apply to the binary the "edited since install" guard
  it already applied to the config. That was an independent bug fix that merely
  arrived alongside; it is now computed from the staged binary rather than from
  a download.
- **README and LICENSE staged into the package**, with the copy no longer
  swallowing its own failure.

## The decision this all rests on

**One target: `aarch64-apple-darwin`.** Everything else follows.

The evidence for cutting rather than growing is in the ecosystem, not in
preference. The first tag push failed because **pnpm has no macOS x64 build** —
it publishes `darwin-arm64`, `linux-*` and `win32-*` and nothing else, at any
recent version, so every binary backend fails identically and no backend swap
helps. Carrying a target whose own build tooling has stopped supporting it means
owning that gap indefinitely.

Linux was surveyed before being dropped, and the survey said it was *viable* —
which is precisely why declining it is a choice rather than a constraint:

- credentials degrade correctly (`src/modules/spend/creds.rs:72` guards on
  `cfg!(target_os = "macos")` and falls back to `~/.claude/.credentials.json`);
- the crate's platform-specific spots are Unix, not Apple — contract §9 names
  them as what blocks **Windows**;
- TLS needs no system library, because `ureq` is pinned to rustls with baked
  roots specifically to avoid `openssl-sys`.

What it would have cost: two more native runners, a glibc-versus-musl decision
with a real portability floor attached, and — the one that actually bites — the
end of local complete builds. A Mac cross-compiles to the other Apple slice with
nothing but a rustup target; it cannot produce a Linux binary without a linker
it does not have. `build:all` would stop being able to make a releasable set on
a maintainer's machine.

And contract §9's struck-through six-target paragraph set the bar for adding any
target back: *"Four of the six were never verified. A local build proved
architecture, not execution."* Meeting that bar means a native runner that
builds **and runs the suite** per target. That is the right bar, and it is more
than this tool needs today.

## Version lines

The npm package version stays hand-set and separate from `Cargo.toml` for now,
per the standing instruction to test the installer at `0.x` before matching
them.

Worth flagging as this lands: embedding weakens the original reason for the
split. That reason was to republish a `0.x` installer freely while the *fetch
path* was proven, without burning binary versions on installer bugs. With the
binary inside the package, a published version is a specific binary, and a
package claiming `0.1.0` while carrying a `1.0.0` binary is a small standing
confusion. Plan 2 keeps the split because the instruction stands; matching them
is a one-line change whenever you want it, and `crate_version()` goes back to
being the single source it describes itself as.
