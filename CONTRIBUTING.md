# Contributing

Building, testing and releasing `claude-status`. If you are here to *use* the
status line, [readme.md](./readme.md) is the page you want — keep developer
detail out of it and put it here instead.

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
mise run code:test         # the suite — cargo test --features schema
mise run code:lint         # clippy, -D warnings
mise run code:format       # dprint
mise run code:schema       # regenerate schemas/claude-status.schema.json
mise run code:sec          # gitleaks + grype
mise run code:toolchain    # dev and CI agree on the shared tools
mise run build:statusline  # the bar
mise run build:all         # the whole publishable set — today, just the bar
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

`build:all` is a thin wrapper around `build:statusline`. It used to stage an npm
package beside the binary and check the two agreed on a version; the `drop-npm`
cycle deleted the package, so there is one artifact and nothing to disagree
with.

## Platform support

**Apple Silicon macOS only.** `supported_targets` in
`.config/mise/tasks/_scripts/_rust` is where the list lives, and its comment
names everything a new platform has to touch — two places that do *not* derive
from the table, so adding one is never a one-line change.

[Distribution, in the decision record](docs/decisions.md#11-distribution) is the
durable account of why the set is one row, and of the bar any new target has to
clear: a native runner that **builds and runs the suite**, because a binary that
is built but never executed is a claim nobody checked.

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

Tag-driven, on **three** lines that ship independently:

| Line      | Bump first                              | Then tag     | Ships                                                          |
| --------- | --------------------------------------- | ------------ | -------------------------------------------------------------- |
| `v*`      | `Cargo.toml` **and** `npm/package.json` | `v1.2.0`     | the binary, the Homebrew tap, and the installer pointing at it |
| `npm-v*`  | `npm/package.json`                      | `npm-v1.2.1` | the npx installer alone, against the newest binary release     |
| `site-v*` | nothing                                 | `site-v5`    | claude-status.virajp.dev                                       |

Each tag is checked against **the manifest that owns its number** before
anything else runs, so a mismatched tag fails in seconds.

**There are three versions because there are three artifacts.** The binary
self-reports `crate_version()`, and the release refuses to publish one whose
`--version` disagrees. `npm/package.json` is what the registry publishes the
installer under, and `npm/asset.json` records which binary that installer
fetches — so the relationship is written down rather than implied by two numbers
being equal. They *were* one number, back when the package carried the binary;
[the reversal](docs/decisions.md) records why that stopped being right, and the
three consecutive releases that rebuilt a binary whose source had not changed.

**A binary release needs both bumps.** npm cannot republish a version, and the
installer must be published again to point at the new binary — which needs a new
installer number. Forget it and npm refuses the publish; the binary release is
complete and untouched, so bump `npm/package.json` and push an `npm-v` tag.

**Both tag lines are in one workflow file, and must stay there.** npm's trusted
publishing binds to a repository *and* a workflow filename, so a second
publishing workflow cannot authenticate however correct its YAML.

**The GitHub release is the binary's whole distribution.** Per target it carries
a `.tar.gz` with `claude-status` at the archive root, the raw binary beside it,
and a `SHA256SUMS` covering both. The tarball is what a Homebrew formula
consumes; the raw binary is for anyone who wants it directly. `distribution/01`
retired the npm channel and deleted the TypeScript installer, the Node toolchain
and the OIDC trusted-publishing setup with it — see
[npm is retired as a channel](docs/decisions.md#npm-is-retired-as-a-channel) for
why, and note that `npm-installer` brought a channel back on different terms:
what is published now is an installer that downloads this release, never a
package carrying the bytes.

Both asset names come from `asset_name()` in
`.config/mise/tasks/_scripts/_rust`, driven by `supported_targets`, so adding a
target adds both shapes with no edit to the workflow.

### The Homebrew tap

**The formula's source is `.config/homebrew/claude-status.rb`, in this repo.**
Edit it there. The copy in
[`virajp/homebrew-tap`](https://github.com/virajp/homebrew-tap) is generated:
`bump-tap` renders the whole file after every release, substituting `url` and
`sha256` for the release just published, and overwrites the tap's copy. So the
tap cannot drift, and the first release creates the formula rather than needing
one seeded by hand.

The url and name come from the release GitHub just published rather than being
rebuilt from the version, so there is nothing to keep in step there either.

The template carries a real released `url`/`sha256` pair rather than
placeholders, which is what lets the brew gates check it at all.

**Check it inside a tap, not as a loose file.** `brew style` on a bare path
applies generic RuboCop cops that do not apply to formulae — Sorbet sigils,
`Style/Documentation`, frozen string literals — and reports offences on text
that is clean in a tap. Only formula-specific cops like
`FormulaAudit/DependencyOrder` mean anything there.

```sh
brew tap-new scratch/verify --no-git
cp .config/homebrew/claude-status.rb \
  "$(brew --repo scratch/verify)/Formula/claude-status.rb"
brew style scratch/verify
brew audit --strict --formula scratch/verify/claude-status
brew info --formula scratch/verify/claude-status   # the caveats, without installing
brew untap scratch/verify                          # do not skip this
```

`/opt/homebrew` is shared across every worktree and checkout on the machine, so
a scratch tap left behind will contaminate someone else's `brew style` run with
a duplicate-formula error. Untap when you are done.

It authenticates with a GitHub App — `APP_ID` holds the App's **Client ID**, not
the numeric App ID, which is the one thing about the setup that catches people
out. The minted token is scoped to the tap, narrowed to `contents: write`, and
revoked when the job ends.

**The install is three commands, and none is avoidable:**

```sh
brew trust --formula virajp/tap/claude-status
brew tap virajp/tap
brew install --formula virajp/tap/claude-status
```

Homebrew 6 requires explicit trust for the **formula**, not merely for the tap,
so qualifying the name fully does not avoid it — there is no one-command
install. And the qualified name carries `brew trust` but **not** `brew install`,
which is why the tap is a step of its own rather than something the third
command infers.

**This file has now claimed the wrong number twice** — first one command, then
two. See
[the correction](docs/decisions.md#the-install-is-three-commands-and-none-is-avoidable),
which keeps every version and how each got through; it is worth reading before
writing any other "this is the command" line here.

Trust is recorded in `~/.homebrew/trust.json` and is a one-time step per
machine, not per upgrade — as is the tap. That is also why both are easy to get
wrong: once you have trusted the formula and tapped the repo, the broken
instructions work for you. **Neither can be checked on a machine that has
already done it.**

#### Bumping the formula by hand

Only needed if the job fails. Run the **same helpers CI runs** rather than
editing the tap's formula — hand-editing it puts the tap out of step with the
template, and the next release silently overwrites whatever you wrote.

```sh
tag=v1.2.3
source .config/mise/tasks/_scripts/_rust

# Ground truth, from the release itself — never typed out.
gh release view "$tag" --json assets \
  --jq '.assets[] | select(.name | endswith(".tar.gz")) | [.name, .url] | @tsv'
gh release download "$tag" -p SHA256SUMS -O SHA256SUMS

name="<the name printed above>"
url="<the url printed above>"
digest="$(digest_for SHA256SUMS "$name")"

# Renders the whole formula into a checkout of the tap.
render_formula .config/homebrew/claude-status.rb "$url" "$digest" \
  <tap-checkout>/Formula/claude-status.rb
```

Then commit and push in the tap checkout. Verify first, in the tap:

```sh
brew audit --strict --formula virajp/tap/claude-status
```

**Do not use `brew audit --online`** — it requires the homepage to resolve,
which drags the site's availability into the release path.

Note that no brew check catches a wrong asset name: `audit` does not fetch the
url, and `--strict` was measured exiting 0 against one returning 404. Copying
the url out of the release, as above, is the only thing that prevents it.

## Conventions

- **Commits** follow `.config/git-conventional-commits.yaml`. The scope list
  there is authoritative — pre-commit rejects anything else, so do not invent
  one.
- **Formatting** is dprint's job and **correctness** is clippy's; the two do not
  overlap. `mise run code:format` and `mise run code:lint`.
- **Docs** go to whichever of three homes fits, and **no document restates
  behaviour a test already holds** — that restatement is what drifted last time.
  Behaviour is pinned by the suite and by the comment beside the code;
  **decisions** and their reasoning go in
  [docs/decisions.md](docs/decisions.md); the **user-facing** description is the
  site, under `site/`. Cycle plans live under `docs/plans/`.
