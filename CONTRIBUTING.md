# Contributing

Building, testing and releasing `claude-status`. If you are here to *use* the
status line, [readme.md](./readme.md) is the page you want — and note that it is
written as the **npm listing page**, so keep developer detail out of it and put
it here instead.

## Setup

```sh
mise run setup:all         # toolchain
```

This repo uses [mise](https://mise.jdx.dev) for tools and tasks. Every task
lives as a file under `.config/mise/tasks/`; `mise tasks` lists them, though it
hides the ones marked `hide=true`.

The three-file config split is `.config/mise.toml` (everything dev and CI share)
plus `mise.dev.toml` and `mise.ci.toml` for what only one environment needs. **A
tool the base config declares must not be redeclared in an environment config**
— `mise run code:toolchain` enforces that, and pre-commit runs it. The rule
exists because it was once broken: the base pinned rust to the `minimal` profile
while `mise.dev.toml` quietly overrode it to `default`, so local machines had
clippy and CI did not, and the first release failed on it.

## The tasks

```sh
mise run code:test         # the suite — cargo test, tsc, tsup, node --test
mise run code:lint         # clippy, -D warnings
mise run code:format       # dprint
mise run code:schema       # regenerate schemas/claude-status.schema.json
mise run code:sec          # gitleaks + grype
mise run code:toolchain    # dev and CI agree on the shared tools
mise run build:statusline  # the bar
mise run build:installer   # the npm package, staged into target/npm/
mise run build:all         # both of the above, in order
```

`code:schema` regenerates `schemas/claude-status.schema.json` from the Rust
config types. **Run it whenever you touch a config type** — add a field, change
a bound, edit one of the `#[schemars(description = …)]` strings — and commit the
result. A pre-commit hook and a test both run `mise run code:schema --check`, so
a stale schema fails the commit and fails CI rather than shipping a file that
describes a config the binary no longer reads.

It builds behind `--features schema`, which is off by default: `schemars` is an
optional dependency and the released binary carries none of it. That is also why
`code:test` and `code:lint` each run a second pass with the feature on —
otherwise nothing would ever compile the generator.

`build:statusline` builds every published target and smoke-tests the host slice
— the one this machine can actually execute. Pass `--target <triple>` to build
just one. On a non-Mac host it stops and says so rather than staging an
incomplete set.

`build:installer` stages one npm package into `target/npm/`, with the binary
inside it. The package's version is substituted from `Cargo.toml` and the task
fails if the two disagree.

## Platform support

**Apple Silicon macOS only.** `supported_targets` in
`.config/mise/tasks/_scripts/_rust` is where the list lives, and its comment
names everything a new platform has to touch — three places that do *not* derive
from the table, so adding one is never a one-line change.

[Contract §9](docs/spec/statusline-behaviour.md) is the durable record of why
the set is one row, and of the bar any new target has to clear: a native runner
that **builds and runs the suite**, because a binary that is built but never
executed is a claim nobody checked.

### Building it elsewhere anyway

Unsupported, and not a soft "we'd rather you didn't". On Linux, natively:

```sh
cargo build --release
# then point Claude Code's statusLine at target/release/claude-status
```

That is a normal native build with no cross-compilation, and it is the only
route anyone has a reason to try. Two things to know before you do.

**Windows will not compile.** The crate reaches for Unix APIs in three places —
a process-group call in `spawn_detached`, the process-runner's test fixtures,
and a `chmod` in the e2e suite — so this is a small piece of work rather than a
one-line `cfg`, and fixing only the first still fails to build. None of the
three blocks Linux.

**On Linux the TLS stack pulls in `ring`**, whose C code needs a working C
toolchain, so the first failure you hit is likely a missing compiler rather than
anything Rust. Credentials degrade rather than break: the keychain lookup guards
on `cfg!(target_os = "macos")` and falls back to `~/.claude/.credentials.json`.

Nothing outside macOS is built, tested or checked in CI, so treat any of it as
your own.

## Releasing

Tag-driven. Bump `version` in `Cargo.toml` — the single source for every
published version, and what the binary self-reports — commit, then push a
matching `v*` tag. CI checks the tag against `Cargo.toml` before it builds
anything.

The npm package and the binary carry **one** version. They were briefly split
while the installer was being proven; embedding the binary made that one
artifact claiming two versions of itself, so `crate_version()` is the single
source again and `build:installer` fails if the staged manifest disagrees.

`release.yml` publishes to npm via OIDC trusted publishing, with no stored
token. It cannot perform the *first* publish of a package — a Trusted Publisher
is configured on a settings page that does not exist until the package does — so
a new package name has to be created by hand once, with a granular token, before
OIDC can be registered for it.

The GitHub Release carries the binary too. That is for anyone who wants it
directly, and for a Homebrew tap later; nothing in the published npm package
points at it.

## Conventions

- **Commits** follow `.config/git-conventional-commits.yaml`. The scope list
  there is authoritative — pre-commit rejects anything else, so do not invent
  one.
- **Formatting** is dprint's job and **correctness** is clippy's; the two do not
  overlap. `mise run code:format` and `mise run code:lint`.
- **Docs** that describe behaviour belong in
  [the contract](docs/spec/statusline-behaviour.md), which records decisions
  with their reasoning and amends rather than rewrites. Cycle plans live under
  `docs/plans/`.
