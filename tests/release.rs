//! **The release archive is byte-reproducible.**
//!
//! A Homebrew formula pins a `sha256`. `distribution/02` will read that digest
//! out of `SHA256SUMS` and commit it into a formula in another repository. From
//! that moment, any re-run of the same tag that produces different bytes breaks
//! `brew install` for everyone, with nothing in the run to signal it —
//! `--clobber` replaces the asset at a stable URL and reports success.
//!
//! `tar -czf` cannot give that guarantee. It embeds the member's mtime, the
//! owner and group of whoever ran it, and gzip stamps its own timestamp into the
//! header. Two runs from identical bytes therefore produce two digests. That was
//! measured, not assumed: the same binary tarred twice, seconds apart, yields
//! different sha256s while the binary's own digest is unchanged.
//!
//! The Rust build itself **is** reproducible — rebuilding after `touch`ing a
//! source yields a byte-identical binary — so normalising the archive is
//! sufficient. Nothing else in the pipeline contributes drift.
//!
//! This file pins the shell helper that does the normalising, by running it,
//! and pins that the release workflow actually calls it.

use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let path = root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} is missing or unreadable: {e}", path.display()))
}

/// Run a bash snippet with `_scripts/_rust` sourced, from a scratch directory.
///
/// `bash` and not `sh`: the helper is bash, as is every task script in this
/// repository, and `_rust` is sourced by the workflow the same way.
fn bash(script: &str, cwd: &std::path::Path) -> std::process::Output {
    Command::new("bash")
        .arg("-c")
        .arg(format!(
            "set -e\nsource {}/.config/mise/tasks/_scripts/_rust\n{script}",
            root().display()
        ))
        .current_dir(cwd)
        .output()
        .expect("bash runs")
}

/// **Two archives of the same bytes are the same archive.**
///
/// The controls are the point. A test that only asserts equality passes just as
/// well when the harness is broken and both digests are empty, so this also
/// proves the comparison can see a *difference* — change the content and the
/// digest must move.
#[test]
fn the_release_archive_is_byte_identical_across_runs() {
    let dir = tempfile::TempDir::new().expect("a temp dir");
    let src = dir.path().join("stage");
    std::fs::create_dir_all(&src).expect("stage dir");
    std::fs::write(src.join("claude-status"), b"#!/bin/sh\necho hi\n").expect("fake binary");

    let out = bash(
        r#"
        # Two archives of identical content, with the member's mtime and the
        # ambient umask deliberately different between them — the exact drift
        # `tar -czf` would bake in.
        TZ=UTC touch -t 202001010000.00 stage/claude-status
        reproducible_tar one.tar.gz stage claude-status
        TZ=UTC touch -t 209901011111.11 stage/claude-status
        reproducible_tar two.tar.gz stage claude-status
        shasum -a 256 one.tar.gz two.tar.gz | awk '{print $1}'

        # CONTROL: different content must produce a different digest, or the
        # comparison above proves nothing.
        printf 'different\n' > stage/claude-status
        reproducible_tar three.tar.gz stage claude-status
        shasum -a 256 three.tar.gz | awk '{print $1}'
        "#,
        dir.path(),
    );

    assert!(
        out.status.success(),
        "the helper failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let digests: Vec<&str> = std::str::from_utf8(&out.stdout).expect("utf8").lines().collect();
    assert_eq!(digests.len(), 3, "expected three digests, got {digests:?}");
    assert!(!digests[0].is_empty(), "the harness produced an empty digest — it is not measuring anything");

    assert_eq!(
        digests[0], digests[1],
        "two archives of identical bytes differ — a re-run of a tag would break every pinned formula"
    );
    assert_ne!(
        digests[1], digests[2],
        "changing the archived content did not change the digest — this comparison cannot fail, so its passing means nothing"
    );
}

/// The archive still puts the binary at the root, executable.
///
/// Reproducibility is worthless if it is reproducibly wrong. `bin.install`
/// reads the archive root and nothing else, and a 644 member ships a formula
/// that installs something the user cannot run.
#[test]
fn the_reproducible_archive_keeps_the_binary_executable_at_its_root() {
    let dir = tempfile::TempDir::new().expect("a temp dir");
    let src = dir.path().join("stage");
    std::fs::create_dir_all(&src).expect("stage dir");
    std::fs::write(src.join("claude-status"), b"#!/bin/sh\n").expect("fake binary");

    let out = bash(
        r#"
        chmod 755 stage/claude-status
        reproducible_tar a.tar.gz stage claude-status
        tar -tvf a.tar.gz
        "#,
        dir.path(),
    );

    assert!(out.status.success(), "listing failed: {}", String::from_utf8_lossy(&out.stderr));
    let listing = String::from_utf8_lossy(&out.stdout);

    assert!(
        listing.contains("claude-status"),
        "the member is not named `claude-status`: {listing}"
    );
    assert!(
        !listing.contains('/'),
        "the archive has a directory prefix — `bin.install` reads the root only: {listing}"
    );
    assert!(
        listing.contains("rwxr-xr-x"),
        "the archived binary is not executable: {listing}"
    );
}

/// **The workflow uses the helper rather than a bare `tar -czf`.**
///
/// The helper existing is not the guarantee; the release path calling it is.
/// This is the assertion that fails if someone reintroduces `tar -czf` in the
/// collect step, which would restore the drift silently.
#[test]
fn the_release_workflow_archives_through_the_reproducible_helper() {
    let workflow = read(".github/workflows/release.yml");

    assert!(
        workflow.contains("reproducible_tar"),
        "the collect step no longer calls `reproducible_tar` — the archive is drifting again"
    );

    let collect = workflow
        .split("Collect the release assets")
        .nth(1)
        .expect("the collect step exists");
    let collect = collect.split("- name:").next().expect("the step body ends");

    // Comments stripped before the scan. The step's own prose explains what it
    // stopped doing and names the construct to do so; matching on that would
    // make the guard fire on an accurate comment, and force the next author to
    // write around the test rather than say the true thing.
    let code: String = collect
        .lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !code.contains("tar -c"),
        "the collect step archives with a bare `tar`, which embeds mtime, owner and a gzip timestamp: {code}"
    );
}

/// The helper is shared, not duplicated into the workflow.
///
/// The asset *name* is already a silent contract with the formula — nothing in
/// `brew audit`, `brew style` or `brew bump-formula-pr` fetches a URL, so a
/// wrong name is clean at every gate and surfaces as a 404 in front of the
/// first user. The archiving rule has the same shape, so it lives in one file
/// that both the workflow and a local review read.
#[test]
fn the_reproducible_helper_lives_beside_the_other_release_shell() {
    let rust = read(".config/mise/tasks/_scripts/_rust");
    assert!(
        rust.contains("reproducible_tar()"),
        "`reproducible_tar` is not defined in _scripts/_rust — the workflow and a local review would drift apart"
    );
}

/// Return one job's YAML body from `release.yml`.
fn job(workflow: &str, name: &str) -> String {
    let after = workflow
        .split(&format!("\n  {name}:\n"))
        .nth(1)
        .unwrap_or_else(|| panic!("no `{name}` job in release.yml"));
    // A job ends at the next top-level (two-space) key.
    let mut body = String::new();
    for line in after.lines() {
        let is_next_job = !line.starts_with("    ") && line.starts_with("  ") && line.trim_end().ends_with(':');
        if is_next_job {
            break;
        }
        // Comments stripped. These steps explain what they stopped doing and
        // name the construct to do it, so a scan that read the prose would pass
        // on a comment alone — which is exactly what happened when this was
        // first written: deleting `install: false` left the guard green,
        // because the comment above it still said the words.
        body.push_str(line.split('#').next().unwrap_or(""));
        body.push('\n');
    }
    body
}

/// **`publish` installs nothing, because it uses nothing.**
///
/// It runs `_rust_reassemble` (plain bash), the collect step (bash, `shasum`,
/// `tar`) and `gh`. It nonetheless used to install rust — with clippy and
/// rustfmt — and zola, on a runner that needs neither.
///
/// That is a failure surface placed *after* `test` and `build` have spent their
/// minutes and produced an artifact: a death there is a green build with no
/// release. It is also the exact shape of the 2026-08-22 failure, where
/// `mise-action` died installing `pnpm` before any repo command ran.
#[test]
fn the_publish_job_installs_no_tools() {
    let workflow = read(".github/workflows/release.yml");
    let publish = job(&workflow, "publish");

    assert!(
        publish.contains("mise-action"),
        "the publish job no longer pins a mise version at all — this test assumes the action is present with install disabled"
    );
    assert!(
        publish.contains("install: false"),
        "the publish job installs tools it never uses; every one is a way for a finished build to fail before it is released"
    );
}

/// **`publish` still installs nothing.**
///
/// The sibling guard that required `install_args` on the cargo jobs is gone.
/// Scoping those installs looked like a cheap risk reduction and was not: three
/// runs of the same commit, minutes apart, produced clippy twice and not the
/// third, with rustup resyncing a `minimal` toolchain at lint time. A release
/// gate that fails at random is worse than an extra tool download, so the
/// declared set is installed again.
///
/// `publish` is different and keeps its `install: false`: it genuinely runs no
/// mise-provided tool — bash, `shasum`, `tar`, `gh` — and it has now succeeded
/// that way on a real release.
/// **A manual dispatch cannot invent a tag.**
///
/// The tag/crate agreement gate is wrapped in `if ref_type = tag`, and a
/// `workflow_dispatch` runs against a *branch*, so the gate is skipped
/// entirely. `publish` then computes the tag from `Cargo.toml` and
/// `gh release create` **creates a tag that nobody pushed**, pointing at the
/// dispatched ref.
///
/// Dispatching from `main` today would therefore publish `v0.1.0` with no human
/// having tagged it — and, once a formula pins the digest, replace a live
/// release's assets with a fresh build.
#[test]
fn a_manual_dispatch_cannot_publish_a_release() {
    let workflow = read(".github/workflows/release.yml");
    let publish = job(&workflow, "publish");

    assert!(
        publish.contains("github.ref_type") || publish.contains("REF_TYPE"),
        "the publish job does not check what kind of ref it is running against, so a workflow_dispatch can create a tag out of thin air"
    );
}

/// **Release notes are generated from the commits, not by GitHub.**
///
/// `--generate-notes` produced 74 characters for `v0.1.0` — a bare changelog
/// link. It has nothing to enumerate here: there was no prior tag to diff, and
/// this repository merges locally rather than through pull requests, which is
/// the input GitHub's generator actually reads.
///
/// `.config/git-conventional-commits.yaml` already describes exactly the
/// changelog this project wants — headlines per commit type, commit and issue
/// URLs, and which types are worth listing. It had never been used for
/// anything but validating commit messages.
///
/// The notes are built in `verify` rather than `publish`, for two reasons:
/// `publish` installs no tools and is worth keeping that way, and `verify`
/// runs before anything is built, so a broken generator costs nothing.
///
/// **`fetch-depth: 0` is load-bearing.** `actions/checkout` is shallow by
/// default, and a changelog walked over one commit is empty.
#[test]
fn the_release_notes_are_generated_from_conventional_commits() {
    let workflow = read(".github/workflows/release.yml");

    let verify = job(&workflow, "verify");
    assert!(
        verify.contains("fetch-depth: 0"),
        "the verify job checks out shallow, so the changelog would be walked over a single commit and come out empty"
    );
    assert!(
        verify.contains("git-conventional-commits"),
        "nothing generates release notes from the commit history"
    );

    let publish = job(&workflow, "publish");
    assert!(
        publish.contains("--notes-file"),
        "the release still takes its body from somewhere other than the generated notes"
    );
    assert!(
        !publish.contains("--generate-notes"),
        "the release still asks GitHub to generate notes, which produced 74 characters for v0.1.0"
    );
}

/// The generator is declared where CI installs it.
///
/// `verify` installs the declared tool set, so the tool has to be in the base
/// config — the same rule that `dprint` was moved for after the first release
/// attempt failed on a runner that did not have it.
#[test]
fn the_changelog_generator_is_declared_for_ci() {
    let base = read(".config/mise.toml");
    let tools: String = base
        .split("[tools]")
        .nth(1)
        .unwrap_or("")
        .split("\n[")
        .next()
        .unwrap_or("")
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter(|l| l.contains('='))
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        tools.contains("git-conventional-commits"),
        "the changelog generator is not in `.config/mise.toml`'s [tools], so `MISE_ENV=ci` will not install it and the release would publish empty notes"
    );
}

/// **The digest lookup is anchored to the whole asset name.**
///
/// `SHA256SUMS` lists the raw binary and the tarball, and the raw name is a
/// strict *prefix* of the tarball's — `claude-status-darwin-arm64` against
/// `claude-status-darwin-arm64.tar.gz`. `shasum` writes the shorter name first,
/// so the obvious `grep "$asset" SHA256SUMS | head -1` hands back the **raw
/// binary's** digest for a `url` that points at the tarball.
///
/// That failure is silent in every direction that matters. The bump job stays
/// green, `brew audit` never fetches the URL, and the formula is well-formed —
/// it just fails the checksum for every user who installs it.
///
/// Measured against the real published manifest, not a guess: v0.1.0's raw
/// binary is `9d088dc5…` and its tarball is `af64e2a6…`.
#[test]
fn the_digest_lookup_is_anchored_to_the_whole_asset_name() {
    let dir = tempfile::TempDir::new().expect("a temp dir");
    // The exact shape `shasum -a 256 *` produces, shorter name first.
    std::fs::write(
        dir.path().join("SHA256SUMS"),
        "9d088dc57367f21870ddb55dba50e7a926ff0b1b5761cae6b0059770019f2f65  claude-status-darwin-arm64\naf64e2a6ed8c0b27d6d9a0473ab5b8c9b7ce1cf123d720f4612c68ae58a9a044  claude-status-darwin-arm64.tar.gz\n",
    )
    .expect("fixture manifest");

    let out = bash(
        r#"
        digest_for SHA256SUMS claude-status-darwin-arm64.tar.gz
        digest_for SHA256SUMS claude-status-darwin-arm64

        # CONTROL: the naive lookup this helper exists to replace. If this does
        # NOT return the raw binary's digest for the tarball's name, the prefix
        # collision has gone away and the assertions above stopped testing it.
        grep "claude-status-darwin-arm64" SHA256SUMS | head -1 | awk '{print $1}'
        "#,
        dir.path(),
    );

    assert!(out.status.success(), "digest_for failed: {}", String::from_utf8_lossy(&out.stderr));

    let got: Vec<&str> = std::str::from_utf8(&out.stdout).expect("utf8").lines().collect();
    assert_eq!(got.len(), 3, "expected three digests, got {got:?}");

    assert_eq!(
        got[0], "af64e2a6ed8c0b27d6d9a0473ab5b8c9b7ce1cf123d720f4612c68ae58a9a044",
        "the tarball's name returned the wrong digest — every `brew install` would fail the checksum"
    );
    assert_eq!(
        got[1], "9d088dc57367f21870ddb55dba50e7a926ff0b1b5761cae6b0059770019f2f65",
        "the raw binary's name returned the wrong digest"
    );
    assert_eq!(
        got[2], got[1],
        "the naive lookup no longer returns the raw binary's digest for the tarball's name, so this test's premise is stale and it is guarding nothing"
    );
    assert_ne!(got[0], got[1], "the two assets must not share a digest — the fixture is wrong");
}

/// **An asset that is not in the manifest fails loudly, rather than emptily.**
///
/// The lookup is keyed by asset name, so a *wrong* name does not return a wrong
/// digest — it returns nothing. An empty `--sha256` is worse than a wrong one:
/// `brew` falls back to a best-effort download instead of refusing, so the
/// formula ships with no integrity check at all.
///
/// This is the compounding half of the asset-name hazard. Nothing offline
/// catches a wrong name — `brew audit --strict` exits 0 on a URL that 404s,
/// measured — so this guard is the only thing standing between a renamed asset
/// and a formula with an empty digest.
#[test]
fn an_asset_missing_from_the_manifest_fails_instead_of_returning_empty() {
    let dir = tempfile::TempDir::new().expect("a temp dir");
    std::fs::write(
        dir.path().join("SHA256SUMS"),
        "af64e2a6ed8c0b27d6d9a0473ab5b8c9b7ce1cf123d720f4612c68ae58a9a044  claude-status-darwin-arm64.tar.gz\n",
    )
    .expect("fixture manifest");

    let missing = bash("digest_for SHA256SUMS claude-status-aarch64-apple-darwin.tar.gz", dir.path());
    assert!(
        !missing.status.success(),
        "a name absent from the manifest exited 0 — the bump job would commit an empty sha256 and brew would stop verifying anything"
    );
    assert!(
        String::from_utf8_lossy(&missing.stdout).trim().is_empty(),
        "a failed lookup still printed something on stdout, which the caller would capture as a digest"
    );

    // CONTROL: the same helper, same manifest, a name that IS present. Without
    // this, the assertion above passes just as well when `digest_for` is broken
    // for every input.
    let present = bash("digest_for SHA256SUMS claude-status-darwin-arm64.tar.gz", dir.path());
    assert!(
        present.status.success(),
        "the helper failed on a name that is present, so its failure above proves nothing: {}",
        String::from_utf8_lossy(&present.stderr)
    );
}

/// A formula in the shape the tap actually carries.
///
/// Deliberately not a minimal stub: the rewrite has to leave `desc`, `license`,
/// the `depends_on` pair, `caveats` and `test` untouched, and a stub with only
/// the two rewritten fields would not notice if it did not.
fn formula_fixture() -> &'static str {
    // `r##` and not `r#`: the `test do` block below contains `"#{bin}`, and the
    // `"#` in that sequence would close an `r#"…"#` literal.
    r##"class ClaudeStatus < Formula
  desc "Status line for Claude Code"
  homepage "https://claude-status-site.pages.dev"
  url "https://github.com/virajp/claude-status/releases/download/v0.1.0/claude-status-darwin-arm64.tar.gz"
  sha256 "af64e2a6ed8c0b27d6d9a0473ab5b8c9b7ce1cf123d720f4612c68ae58a9a044"
  license "MIT"

  depends_on arch: :arm64
  depends_on :macos

  def install
    bin.install "claude-status"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/claude-status --version")
  end
end
"##
}

/// **The bump rewrites exactly two fields and disturbs nothing else.**
///
/// `version` is deliberately not one of them. A `version` line beside a
/// version-bearing url is a hard `brew audit` failure — brew scans the version
/// out of the url — measured this cycle against a scratch tap, where adding one
/// took `brew audit` from exit 0 to exit 1 with "redundant with version scanned
/// from URL".
#[test]
fn the_formula_rewrite_moves_the_url_and_digest_and_leaves_the_rest_alone() {
    let dir = tempfile::TempDir::new().expect("a temp dir");
    std::fs::write(dir.path().join("claude-status.rb"), formula_fixture()).expect("fixture formula");

    let new_url = "https://github.com/virajp/claude-status/releases/download/v1.0.0/claude-status-darwin-arm64.tar.gz";
    let new_digest = "1111111111111111111111111111111111111111111111111111111111111111";

    let out = bash(
        &format!("rewrite_formula claude-status.rb '{new_url}' '{new_digest}'"),
        dir.path(),
    );
    assert!(
        out.status.success(),
        "rewrite_formula failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let after = std::fs::read_to_string(dir.path().join("claude-status.rb")).expect("rewritten formula");

    assert!(after.contains(new_url), "the new url is not in the formula");
    assert!(after.contains(new_digest), "the new digest is not in the formula");
    assert!(
        !after.contains("v0.1.0"),
        "the old url survived the rewrite — the tap would keep serving the previous release"
    );
    assert!(
        !after.contains("af64e2a6"),
        "the old digest survived the rewrite — brew would fail the checksum on the new tarball"
    );

    // A `version` line is never introduced. Nothing adds one today; this pins
    // that, because the failure is invisible until `brew audit` runs.
    assert!(
        !after.lines().any(|l| l.trim_start().starts_with("version ")),
        "the rewrite introduced a `version` line, which is a hard `brew audit` failure beside a version-bearing url"
    );

    // Everything that is not those two fields is untouched.
    for kept in [
        "desc \"Status line for Claude Code\"",
        "license \"MIT\"",
        "depends_on arch: :arm64",
        "depends_on :macos",
        "bin.install \"claude-status\"",
    ] {
        assert!(after.contains(kept), "the rewrite lost `{kept}`");
    }
    assert_eq!(
        after.lines().count(),
        formula_fixture().lines().count(),
        "the rewrite changed the formula's line count, so it did more than replace two values"
    );
}

/// **An ambiguous or unrecognisable formula stops the bump.**
///
/// `awk` exits 0 whether or not a pattern ever matched, so a formula that
/// changed shape would sail through a rewrite that silently did nothing, and
/// the tap would keep pinning the previous release with the run green — the
/// quiet failure mode this job was always most likely to have.
#[test]
fn the_formula_rewrite_refuses_a_formula_it_cannot_place() {
    let dir = tempfile::TempDir::new().expect("a temp dir");
    let url = "https://example.com/x.tar.gz";
    let digest = "2222222222222222222222222222222222222222222222222222222222222222";

    // No `url` line at all — nothing to rewrite.
    std::fs::write(dir.path().join("no-url.rb"), "class ClaudeStatus < Formula\n  sha256 \"abc\"\nend\n")
        .expect("fixture");
    let no_url = bash(&format!("rewrite_formula no-url.rb '{url}' '{digest}'"), dir.path());
    assert!(
        !no_url.status.success(),
        "a formula with no `url` line was rewritten successfully — the bump would report success having changed nothing"
    );

    // Two `url` lines — the rewrite would be ambiguous.
    std::fs::write(
        dir.path().join("two-urls.rb"),
        "class ClaudeStatus < Formula\n  url \"a\"\n  url \"b\"\n  sha256 \"abc\"\nend\n",
    )
    .expect("fixture");
    let two_urls = bash(&format!("rewrite_formula two-urls.rb '{url}' '{digest}'"), dir.path());
    assert!(
        !two_urls.status.success(),
        "a formula with two `url` lines was accepted — the bump cannot know which one the tarball is"
    );

    // CONTROL: the well-formed fixture must succeed with the same helper and
    // the same arguments. Without it, both assertions above pass just as well
    // when `rewrite_formula` rejects everything it is given.
    std::fs::write(dir.path().join("good.rb"), formula_fixture()).expect("fixture");
    let good = bash(&format!("rewrite_formula good.rb '{url}' '{digest}'"), dir.path());
    assert!(
        good.status.success(),
        "the helper rejected a well-formed formula, so its refusals above prove nothing: {}",
        String::from_utf8_lossy(&good.stderr)
    );
}

/// **The bump job never spells an asset name — it reads the published release.**
///
/// A formula whose `url` names an asset that does not exist is clean at every
/// gate that exists. Plain `brew audit` does not fetch the URL, and
/// `brew audit --strict` was measured this cycle exiting 0 against a URL
/// returning 404. `brew bump-formula-pr` treats the url as an opaque string
/// too. So a mistyped or drifted asset name surfaces first as a failed
/// `brew install`, after the tag is cut, in front of the first user — with no
/// CI signal anywhere behind it.
///
/// The fix is structural rather than another assertion: if the job takes both
/// the name and the url from what GitHub actually published, there is no name
/// to get wrong. This guard pins that it keeps doing so.
///
/// Note `job()` strips comments before matching. The prose above and in the
/// workflow names every construct being banned, so a scan that read comments
/// would pass on the comment alone.
#[test]
fn the_bump_job_takes_the_asset_from_the_published_release() {
    let workflow = read(".github/workflows/release.yml");
    let bump = job(&workflow, "bump-tap");

    assert!(
        bump.contains("gh release view"),
        "the bump job does not query the release, so it must be reconstructing the asset name locally"
    );
    assert!(
        bump.contains(".assets[]"),
        "the bump job does not read the release's asset list"
    );

    // The two shapes that would mean a name was written down rather than read.
    assert!(
        !bump.contains("releases/download"),
        "the bump job builds a download URL itself; it must use the `url` the release reports, which cannot 404"
    );
    assert!(
        !bump.contains("claude-status-"),
        "the bump job spells an asset name literally; nothing offline catches a wrong one, so it must come from the published release"
    );

    // The digest and the rewrite go through the tested helpers rather than
    // being re-implemented in YAML, where no test could reach them.
    assert!(
        bump.contains("digest_for"),
        "the bump job does not use `digest_for`, so its digest lookup is unanchored and untested"
    );
    assert!(
        bump.contains("rewrite_formula"),
        "the bump job does not use `rewrite_formula`, so its rewrite is untested"
    );

    // Ordering is load-bearing: reading a release that does not exist yet
    // cannot give ground truth.
    let header = bump.lines().take(4).collect::<String>();
    assert!(
        header.contains("publish"),
        "the bump job does not declare `needs: publish`, so it could run before the release it reads from exists"
    );
}

/// **Pushing to the tap uses a minted App token, not a credential at rest.**
///
/// `GITHUB_TOKEN` cannot reach another repository, so this job needs a
/// credential of its own — and the three candidates differ entirely in what is
/// sitting in this repository's secrets between releases. A PAT is
/// account-level and can reach every repo the account can. A deploy key is
/// scoped to the tap but can push to it forever. A GitHub App leaves nothing at
/// rest that can push anything: the secrets authorise *minting*, and the token
/// is installation-scoped, short-lived, and revoked when the job ends.
///
/// This guard pins the choice rather than the reasoning, because the two
/// rejected options are each one line away and both would work.
#[test]
fn the_tap_push_uses_a_minted_app_token() {
    let workflow = read(".github/workflows/release.yml");
    let bump = job(&workflow, "bump-tap");

    assert!(
        bump.contains("actions/create-github-app-token"),
        "the tap credential is not a minted App token"
    );
    assert!(
        bump.contains("steps.tap-token.outputs.token"),
        "the tap checkout does not use the minted token, so minting it achieved nothing"
    );
    assert!(
        bump.contains("permission-contents: write"),
        "the minted token does not narrow its permissions, so it carries whatever the installation was granted"
    );

    // `app-id` is deprecated in favour of `client-id`. The secret name is the
    // same either way, so nothing else would catch a silent swap back.
    assert!(
        bump.contains("client-id:"),
        "the App is identified by the deprecated `app-id` input rather than `client-id`"
    );

    // The two rejected credentials, neither of which may creep back.
    assert!(
        !bump.contains("ssh-key:"),
        "the tap checkout uses a deploy key — a long-lived private key that can push to the tap forever"
    );

    // Every secret this job touches, by name. Substring-matching for "pat"
    // does not work here and the first draft of this test proved it: `path:
    // tap` contains it. Enumerating the references is exact, and it also
    // catches a credential nobody thought to ban.
    let secrets: Vec<&str> = bump
        .match_indices("secrets.")
        .map(|(i, _)| {
            let rest = &bump[i + "secrets.".len()..];
            let end = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(rest.len());
            &rest[..end]
        })
        .collect();

    assert!(
        !secrets.is_empty(),
        "the bump job references no secrets at all — it cannot be authenticating to another repository, so this test is reading the wrong job"
    );
    for name in &secrets {
        assert!(
            matches!(*name, "APP_ID" | "APP_KEY"),
            "the bump job uses `secrets.{name}`; the only credentials it may hold are the App's client id and private key, which mint a short-lived scoped token rather than being able to push themselves"
        );
    }
}

/// Splits a workflow into everything before `jobs:` and each job under it.
///
/// A job opens at exactly two spaces of indent and runs to the next line at
/// that indent, or EOF. The same shape `tests/site.rs` uses, and duplicated
/// rather than shared because each `tests/*.rs` is its own crate.
fn split_jobs(workflow: &str) -> (String, Vec<(&str, String)>) {
    let jobs_at = workflow.find("\njobs:").expect("release.yml has no `jobs:` block");
    let preamble = workflow[..jobs_at].to_string();

    let mut jobs: Vec<(&str, String)> = Vec::new();
    let mut current: Option<(&str, Vec<&str>)> = None;
    for line in workflow[jobs_at..].lines() {
        let is_header = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('#');
        if is_header {
            if let Some((name, body)) = current.take() {
                jobs.push((name, body.join("\n")));
            }
            current = Some((line.trim().trim_end_matches(':'), vec![line]));
        } else if let Some((_, body)) = current.as_mut() {
            body.push(line);
        }
    }
    if let Some((name, body)) = current.take() {
        jobs.push((name, body.join("\n")));
    }
    (preamble, jobs)
}

/// **The tap credential is reachable from exactly one job.**
///
/// `site.yml` has had a containment test since it gained a deploy secret;
/// `release.yml` had none, because until this cycle it had no secret worth
/// containing. It does now.
///
/// The threat is not a pull request — this workflow does not run on one. It is
/// that `verify`, `test`, `build` and `publish` all run `cargo`, and a cargo
/// build executes **build scripts from every dependency in the tree**. A
/// credential in one of those jobs' environments is a credential offered to
/// arbitrary third-party code on every release. `bump-tap` compiles nothing, so
/// keeping the App there is what makes the supply-chain surface not overlap the
/// credential surface.
///
/// Asserted for every job that is not `bump-tap` rather than for the four by
/// name, so adding a fifth cannot quietly open a path.
#[test]
fn only_the_bump_job_can_reach_the_tap_credential() {
    let workflow = read(".github/workflows/release.yml");
    let (preamble, jobs) = split_jobs(&workflow);

    // A workflow-level `env:` sits above `jobs:` and reaches every job in the
    // file, so a secret there defeats every per-job assertion below.
    assert!(
        !preamble.contains("secrets."),
        "a secret is referenced above `jobs:`; a workflow-level `env` reaches every job, including the ones that run cargo:\n{preamble}"
    );

    let names: Vec<&str> = jobs.iter().map(|(n, _)| *n).collect();
    assert!(names.contains(&"bump-tap"), "release.yml lost its `bump-tap` job; found {names:?}");
    assert!(
        names.iter().any(|n| *n == "publish"),
        "release.yml lost its `publish` job; found {names:?}"
    );

    for (name, body) in &jobs {
        if *name == "bump-tap" {
            continue;
        }
        assert!(
            !body.contains("secrets."),
            "job `{name}` names a secret. Only `bump-tap` may, because it is the only job that does not run cargo — every other job executes dependency build scripts:\n{body}"
        );
    }

    // Every `secrets.` in the file is inside `bump-tap`, wherever it was
    // written. Counting closes the gap the per-job loop leaves if a reference
    // ends up somewhere `split_jobs` does not attribute to a job at all.
    let bump = jobs
        .iter()
        .find(|(n, _)| *n == "bump-tap")
        .map(|(_, b)| b.as_str())
        .expect("checked above");
    assert_eq!(
        workflow.matches("secrets.").count(),
        bump.matches("secrets.").count(),
        "a `secrets.` reference lives outside the `bump-tap` job"
    );
}
