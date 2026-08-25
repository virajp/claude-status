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

/// **No release job installs zola.**
///
/// `verify`, `test` and `build` genuinely need cargo. None of them builds the
/// site. Installing zola on the release path adds a download that can fail for
/// reasons entirely unrelated to the release.
#[test]
fn no_release_job_installs_the_site_generator() {
    let workflow = read(".github/workflows/release.yml");

    for name in ["verify", "test", "build"] {
        let body = job(&workflow, name);
        assert!(
            body.contains("install_args:"),
            "the `{name}` job installs every tool in mise.toml, zola included; scope it with install_args"
        );
        assert!(
            body.contains("rust"),
            "the `{name}` job's install_args does not name rust, which it needs"
        );
        assert!(
            !body.contains("zola"),
            "the `{name}` job still installs zola, which nothing on the release path uses"
        );
    }
}

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
