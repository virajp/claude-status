//! Guards for `npm/` — the third install channel, and the only JavaScript this
//! repository ships.
//!
//! **The installer has no test suite of its own, deliberately.** The whole
//! argument for taking JavaScript back into the tree was that it arrives with
//! no build, no dependency graph and no second `test` command; a `vitest` here
//! would have given back every one of those. So the installer's contract lives
//! in this file, in Rust, and `mise run code:test` still runs everything.
//!
//! # Nothing here touches the network
//!
//! `install.mjs` downloads one asset from one hard-coded GitHub URL, so an
//! honest end-to-end test of it would either reach github.com from CI or prove
//! nothing at all. Neither happens. [`Sandbox`] runs the real file, as the real
//! entry point, under a `node --import` preload that replaces `globalThis.fetch`
//! with one that serves a tarball built here — which is the *one* impure
//! boundary the installer has, and swapping it leaves the digest check, the
//! extract, the atomic rename, the version check, the receipt and the wiring
//! all running for real.
//!
//! That also buys the cases a network could not: a digest that does not match,
//! an asset that 404s, and a host that is not Apple Silicon.
//!
//! # Two things are read from `_scripts/_rust` rather than written down here
//!
//! The version and the asset name. Both already exist in the shell the release
//! workflow runs, and a copy of either in this file would be a third place to
//! get them wrong. `the_asset_name_matches_the_one_the_release_uploads` is the
//! sharp one: it is the only thing between a renamed target and a channel that
//! 404s for every user, and nothing else in the repository compares those two
//! strings.
//!
//! # Skipping
//!
//! Every test that needs `node` skips without it locally and fails under CI,
//! the rule `tests/site.rs` set: a silent skip would retire this whole file
//! while the suite still reported green.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

/// What the crate says it is, for the tests that check the package agrees.
const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The real binary, for the one test that needs `--configure` to have actually
/// run rather than to have been described.
const BINARY: &str = env!("CARGO_BIN_EXE_claude-status");

/// A digest that is 64 hex characters and belongs to nothing.
const WRONG_DIGEST: &str = "deadbeef00000000000000000000000000000000000000000000000000000000";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let path = root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} is missing or unreadable: {e}", path.display()))
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn exit_code(out: &Output) -> i32 {
    out.status.code().unwrap_or(-1)
}

/// Run a bash snippet with `_scripts/_rust` sourced, from a scratch directory.
///
/// Lifted from `tests/release.rs`, which introduced it for the same reason: the
/// helper is the release workflow's own, and asking it a question is the only
/// way to compare against what a release will really do rather than against a
/// transcription of it.
fn bash(script: &str, cwd: &Path) -> Output {
    Command::new("bash")
        .arg("-c")
        .arg(format!("set -e\nsource {}/.config/mise/tasks/_scripts/_rust\n{script}", root().display()))
        .current_dir(cwd)
        .output()
        .expect("bash runs")
}

/// One line of a bash helper's stdout, with the run asserted to have succeeded.
fn ask_rust(script: &str) -> String {
    let out = bash(script, &root());
    assert!(out.status.success(), "`{script}` failed: {}", stderr(&out));
    let answer = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert!(!answer.is_empty(), "`{script}` printed nothing — this comparison would be vacuous");
    answer
}

/// Return one job's YAML body from `release.yml`, comments stripped.
///
/// `tests/release.rs`'s, unchanged, including why the comments go: these steps
/// explain what they stopped doing and name the construct to do it, so a scan
/// that read the prose would pass on a comment alone.
fn job(workflow: &str, name: &str) -> String {
    let after = workflow
        .split(&format!("\n  {name}:\n"))
        .nth(1)
        .unwrap_or_else(|| panic!("no `{name}` job in release.yml"));
    let mut body = String::new();
    for line in after.lines() {
        let is_next_job = !line.starts_with("    ") && line.starts_with("  ") && line.trim_end().ends_with(':');
        if is_next_job {
            break;
        }
        body.push_str(line.split('#').next().unwrap_or(""));
        body.push('\n');
    }
    body
}

/// `node` if it is on PATH, resolved the way a shell would.
///
/// `tests/site.rs`'s, and the absolute path matters more here: [`Sandbox`] runs
/// node with a PATH it controls, and a bare `node` would not be on it.
fn node_on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|dir| dir.join("node")).find(|candidate| candidate.is_file())
}

/// `node`, or a skip that is loud under CI.
///
/// The assertion is the point. `cargo test` captures a passing test's stdout,
/// so the `eprintln!` is invisible without `--nocapture` — which is fine on a
/// laptop and would be a silently retired channel on a runner.
macro_rules! node_or_skip {
    ($what:expr) => {
        match node_on_path() {
            Some(node) => node,
            None => {
                assert!(
                    std::env::var_os("CI").is_none(),
                    "no `node` on PATH under CI. This test is what holds {} — skipping it leaves the npm channel unguarded while the suite still reports green.",
                    $what
                );
                eprintln!("skipped: no `node` on PATH — {} was not exercised", $what);
                return;
            }
        }
    };
}

/// A file's SHA-256, asked of `shasum` rather than computed.
///
/// No hashing crate is a dependency of this repository and this is not a reason
/// to add one: `shasum` is what `release.yml` builds `SHA256SUMS` with, so
/// comparing against it compares against the release's own arithmetic.
fn sha256_of(path: &Path) -> String {
    let out = Command::new("shasum").args(["-a", "256"]).arg(path).output().expect("shasum runs");
    assert!(out.status.success(), "shasum failed on {}: {}", path.display(), stderr(&out));
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .unwrap_or_else(|| panic!("shasum printed nothing for {}", path.display()))
        .to_string()
}

// ---------------------------------------------------------------------------
// The sandbox — the real installer, a fake $HOME, and no network
// ---------------------------------------------------------------------------

/// A staged copy of `npm/`, a throwaway `$HOME`, a synthetic release asset and
/// a `fetch` that serves it.
///
/// **The staged copy is not a mock.** It is `npm/install.mjs` byte for byte,
/// beside `npm/package.json` byte for byte; only `asset.json` is rewritten —
/// which is exactly what `.config/mise/tasks/release/npm-package` does at
/// publish time, and for the same reason. The tracked `asset.json` pins a
/// digest for bytes that live on github.com, and there is no offline way to
/// obtain them.
///
/// The `$HOME` is throwaway because the installer writes a binary, a receipt
/// and `~/.claude/settings.json`, and a test that pointed any of that at the
/// developer's real home would be a test nobody could run twice.
struct Sandbox {
    dir: TempDir,
    node: PathBuf,
    /// The SHA-256 of the tarball currently being served, which is what the
    /// installer must compute for itself and compare against its pin.
    tarball_digest: String,
}

impl Sandbox {
    /// The default shape: a good tarball, a matching pin, and a `~/.local/bin`
    /// that is on PATH.
    fn new(node: &Path) -> Self {
        let dir = TempDir::new().expect("a temp dir");
        let mut sandbox = Sandbox { dir, node: node.to_path_buf(), tarball_digest: String::new() };

        std::fs::create_dir_all(sandbox.bin()).expect("~/.local/bin");
        std::fs::create_dir_all(sandbox.pkg()).expect("the staged package");
        std::fs::create_dir_all(sandbox.at("stage")).expect("the tar staging dir");
        for file in ["install.mjs", "package.json"] {
            std::fs::copy(root().join("npm").join(file), sandbox.pkg().join(file))
                .unwrap_or_else(|e| panic!("npm/{file} copies: {e}"));
        }
        std::fs::write(sandbox.pkg().join("probe.mjs"), PROBE).expect("the probe");

        sandbox.release(CRATE_VERSION);
        sandbox.serve_the_asset();
        sandbox
    }

    fn at(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn home(&self) -> PathBuf {
        self.at("home")
    }

    fn bin(&self) -> PathBuf {
        self.home().join(".local").join("bin")
    }

    fn pkg(&self) -> PathBuf {
        self.at("pkg")
    }

    /// Where a successful `--install` puts the binary, under the default PATH.
    fn placed(&self) -> PathBuf {
        self.bin().join("claude-status")
    }

    fn receipt_path(&self) -> PathBuf {
        self.home().join(".local").join("state").join("claude-status").join("install-receipt.json")
    }

    fn receipt(&self) -> serde_json::Value {
        let text = std::fs::read_to_string(self.receipt_path()).expect("a receipt was written");
        serde_json::from_str(&text).expect("the receipt is JSON")
    }

    /// Build the release asset: a shim that answers `--version` with `reports`
    /// and records that `--configure` reached it.
    ///
    /// A shell script rather than the real binary, following `tests/e2e.rs`:
    /// the installer only ever asks the thing it placed two questions, and a
    /// 12MB copy per test buys nothing but minutes.
    fn release(&mut self, reports: &str) {
        let shim = self.at("stage").join("claude-status");
        std::fs::write(
            &shim,
            format!(
                "#!/bin/sh\ncase \"$1\" in\n  --version) echo {reports} ;;\n  --configure) : > \"$HOME/configure-ran\" ;;\n  *) exit 3 ;;\nesac\n"
            ),
        )
        .expect("the shim");
        std::fs::set_permissions(&shim, std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("chmod");

        let tarball = self.at("asset.tar.gz");
        let out = Command::new("tar")
            .arg("-czf")
            .arg(&tarball)
            .arg("-C")
            .arg(self.at("stage"))
            .arg("claude-status")
            .output()
            .expect("tar runs");
        assert!(out.status.success(), "tar failed: {}", stderr(&out));

        self.tarball_digest = sha256_of(&tarball);
        self.pin(&self.tarball_digest.clone());
    }

    /// Rewrite the staged `asset.json`'s digest — the installer's whole pin.
    fn pin(&self, digest: &str) {
        std::fs::write(
            self.pkg().join("asset.json"),
            format!("{{\n  \"tag\": \"v{CRATE_VERSION}\",\n  \"name\": \"claude-status-darwin-arm64.tar.gz\",\n  \"sha256\": \"{digest}\"\n}}\n"),
        )
        .expect("the staged asset.json");
    }

    /// A `fetch` that hands back the tarball built above.
    fn serve_the_asset(&self) {
        self.harness("return new Response(readFileSync(ASSET));", None);
    }

    /// A `fetch` that 404s, which is what a renamed or deleted asset does.
    fn serve_a_404(&self) {
        self.harness("return new Response(\"\", { status: 404, statusText: \"Not Found\" });", None);
    }

    /// A host the release carries no build for.
    ///
    /// `process.platform` is overridden rather than the test being run on a
    /// Linux runner, because the whole suite runs on Apple Silicon — which is
    /// the only architecture this project releases for, and therefore the only
    /// one on which the gate can never fire by itself.
    fn pretend_host(&self, os: &str, cpu: &str) {
        self.harness("throw new Error(\"the installer reached the network on an unsupported host\");", Some((os, cpu)));
    }

    /// Write the `--import` preload: every fetch is recorded before it is
    /// answered, so "did not attempt" is checkable rather than inferred.
    fn harness(&self, body: &str, host: Option<(&str, &str)>) {
        let mut source = String::from("import { appendFileSync, readFileSync } from \"node:fs\";\n");
        source.push_str(&format!("const ASSET = {:?};\n", self.at("asset.tar.gz").display().to_string()));
        source.push_str(&format!("const LOG = {:?};\n", self.at("fetched.txt").display().to_string()));
        if let Some((os, cpu)) = host {
            source.push_str(&format!("Object.defineProperty(process, \"platform\", {{ value: {os:?} }});\n"));
            source.push_str(&format!("Object.defineProperty(process, \"arch\", {{ value: {cpu:?} }});\n"));
        }
        source.push_str("globalThis.fetch = async (url) => {\n  appendFileSync(LOG, `${url}\\n`);\n  ");
        source.push_str(body);
        source.push_str("\n};\n");
        std::fs::write(self.at("harness.mjs"), source).expect("the harness");
    }

    /// Every URL the installer asked for, in order.
    fn fetched(&self) -> Vec<String> {
        std::fs::read_to_string(self.at("fetched.txt"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// What is sitting in the install directory, sorted — the whole answer to
    /// "did a failed install leave anything behind", staging dirs included.
    fn bin_entries(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.bin())
            .expect("the install directory")
            .map(|e| e.expect("an entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn run(&self, args: &[&str]) -> Output {
        let path = format!("{}:/usr/bin:/bin", self.bin().display());
        self.run_as(args, &path, "/bin/zsh")
    }

    /// The installer, as the process entry point, with nothing inherited.
    ///
    /// `env_clear` is not hygiene: `PATH` is an *input* to three of the
    /// decisions under test, and `HOME` decides where every write lands.
    /// Inheriting either would make the answers depend on whose laptop ran it.
    ///
    /// Stdin is null, so `process.stdin.isTTY` is false — which is the CI case,
    /// and the one the consent rule turns on.
    fn run_as(&self, args: &[&str], path: &str, shell: &str) -> Output {
        Command::new(&self.node)
            .arg("--import")
            .arg(format!("file://{}", self.at("harness.mjs").display()))
            .arg(self.pkg().join("install.mjs"))
            .args(args)
            .env_clear()
            .env("HOME", self.home())
            .env("PATH", path)
            .env("SHELL", shell)
            .output()
            .expect("node runs")
    }

    /// Ask the installer's pure exports a question, in JavaScript, and get the
    /// answer back as JSON so every assertion lives in Rust.
    fn probe(&self, expr: &str) -> serde_json::Value {
        let out = Command::new(&self.node)
            .arg(self.pkg().join("probe.mjs"))
            .arg(expr)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .output()
            .expect("node runs");
        assert!(out.status.success(), "the probe failed on `{expr}`:\n{}", stderr(&out));
        serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("`{expr}` did not answer with JSON ({e}): {}", String::from_utf8_lossy(&out.stdout)))
    }
}

/// Evaluates one expression against the installer's exports.
///
/// Direct `eval`, so the expression can see the bindings imported above it —
/// and the expression is a literal in this test file, never anything a user
/// supplies. The alternative was a bespoke probe script per test, which is the
/// same code seven times with one line different.
///
/// The importing is what makes the module's shape load-bearing: an
/// `install.mjs` that did anything on import would download and place a binary
/// here.
const PROBE: &str = r#"import { ASSET, VERSION, chooseInstallDir, classifyExisting, helpText, parseArgs, unwireSettings } from "./install.mjs";

const answer = eval(process.argv[2]);
process.stdout.write(JSON.stringify(answer === undefined ? null : answer));
"#;

/// The staged package's exports, with no `$HOME` or asset behind them.
///
/// The pure functions need none of the machinery [`Sandbox`] builds, but they
/// do need `install.mjs` to sit beside a `package.json` and an `asset.json` —
/// it reads both at import time — so the staging is shared rather than
/// duplicated.
fn pure(node: &Path) -> Sandbox {
    Sandbox::new(node)
}

// ---------------------------------------------------------------------------
// The manifest
// ---------------------------------------------------------------------------

/// **One version, and it is the crate's.**
///
/// The npm package once carried a hand-set `0.x` while the binary reported
/// `1.0.0` — one artifact claiming two versions of itself — and that is the
/// concrete failure the channel came back with a guard against. The release
/// task stamps the staged manifest from `crate_version()` so nothing is typed;
/// this is what keeps the *tracked* manifest honest in between releases, where
/// no stamping has happened and a reader would otherwise be told the wrong
/// thing by the only file they can see.
///
/// Asked of `crate_version()` rather than of `Cargo.toml`, because
/// `crate_version()` is what `verify` checks the pushed tag against.
#[test]
fn the_package_version_equals_the_crate_version() {
    let from_shell = ask_rust("crate_version");
    assert_eq!(
        from_shell, CRATE_VERSION,
        "`crate_version()` and the compiled crate disagree — every other comparison in this file is built on it"
    );

    let manifest: serde_json::Value = serde_json::from_str(&read("npm/package.json")).expect("npm/package.json is JSON");
    assert_eq!(
        manifest["version"].as_str(),
        Some(from_shell.as_str()),
        "npm/package.json claims a version the crate does not — a user who runs `npx @askviraj/claude-status` gets an installer that names a release that is not the one it fetches"
    );
}

/// **The platform gate is derived, not transcribed.**
///
/// `os`/`cpu` in the manifest are what makes npm refuse with `EBADPLATFORM`
/// before a line of the installer runs. They are also the third and fourth
/// spellings of a fact `supported_targets()` already holds — and the first two
/// spellings (the release matrices) are already unguarded, which is precisely
/// why adding a target is expensive.
///
/// Adding a platform to the release without adding it here ships a package npm
/// refuses to install on a machine the release now serves; dropping one without
/// dropping it here ships a package that installs and then 404s.
#[test]
fn the_package_declares_the_only_platform_the_release_carries() {
    let rows = ask_rust("supported_targets");

    let mut oses = BTreeSet::new();
    let mut cpus = BTreeSet::new();
    for row in rows.lines() {
        let fields: Vec<&str> = row.split_whitespace().collect();
        assert_eq!(fields.len(), 3, "`supported_targets` row is not `<triple> <os> <cpu>`: {row}");
        oses.insert(fields[1].to_string());
        cpus.insert(fields[2].to_string());
    }
    assert!(!oses.is_empty(), "`supported_targets` returned no rows — this comparison would be vacuous");

    let manifest: serde_json::Value = serde_json::from_str(&read("npm/package.json")).expect("npm/package.json is JSON");
    let declared = |key: &str| -> BTreeSet<String> {
        manifest[key]
            .as_array()
            .unwrap_or_else(|| panic!("npm/package.json has no `{key}` array — npm would install this package anywhere"))
            .iter()
            .map(|v| v.as_str().expect("a string").to_string())
            .collect()
    };

    assert_eq!(declared("os"), oses, "the manifest's `os` and `supported_targets()` name different platforms");
    assert_eq!(declared("cpu"), cpus, "the manifest's `cpu` and `supported_targets()` name different architectures");
}

// ---------------------------------------------------------------------------
// The flag surface
// ---------------------------------------------------------------------------

/// **The help cannot advertise a flag the parser rejects.**
///
/// The two live in one file and drift anyway: the help is prose and gets edited
/// like prose, and a flag renamed in the `switch` leaves the old spelling in the
/// text below it. A user who copies a line out of `--help` and is told
/// `unrecognised argument` has been sent somewhere by the tool's own
/// documentation.
///
/// The flags are read back out of `helpText()` rather than listed here, so this
/// keeps holding when a flag is added — and the module doc says so at the
/// function, which is why the binary's own `--statusline` and `--caps-hook` are
/// described in prose there rather than written with their dashes.
///
/// **The last assertion is the control.** All-null answers also come from a
/// parser that never reports an error at all.
#[test]
fn every_flag_the_help_lists_is_a_flag_the_parser_accepts() {
    let node = node_or_skip!("the installer's help against its parser");
    let staged = pure(&node);

    let help = staged.probe("helpText()").as_str().expect("helpText returns a string").to_string();

    let mut flags: BTreeSet<String> = BTreeSet::new();
    for (at, _) in help.match_indices("--") {
        let rest = &help[at..];
        let end = rest.find(|c: char| !(c.is_ascii_lowercase() || c == '-')).unwrap_or(rest.len());
        let flag = &rest[..end];
        if flag.len() > 2 && !flag.ends_with('-') {
            flags.insert(flag.to_string());
        }
    }
    assert!(
        flags.len() >= 6,
        "only {} flags were found in the help text — the scan is reading the wrong thing: {flags:?}",
        flags.len()
    );

    let listed: Vec<&str> = flags.iter().map(String::as_str).collect();
    let errors = staged.probe(&format!(
        "{}.map((flag) => parseArgs([flag]).error)",
        serde_json::to_string(&listed).expect("a JSON array")
    ));

    for (flag, error) in listed.iter().zip(errors.as_array().expect("one answer per flag")) {
        assert!(
            error.is_null(),
            "`--help` lists {flag}, and the parser refuses it: {error}"
        );
    }

    let control = staged.probe("parseArgs([\"--instal\"]).error");
    assert!(
        control.as_str().is_some_and(|e| e.contains("--instal")),
        "the parser accepted a flag that does not exist, so the answers above prove nothing: {control}"
    );
}

/// **A host with no build is named, and nothing is downloaded.**
///
/// The manifest's `os`/`cpu` do this first, but only when npm is the one
/// resolving the package — `bunx` and a direct `node install.mjs` both walk
/// straight past it. This is the second half of the same gate, and it exists
/// because the alternative is a user watching a download finish and then being
/// handed a Mach-O binary their kernel will not run.
///
/// The window it closes was left knowingly open when the npm channel was
/// deleted, with no runtime host check anywhere in the tree.
///
/// **"Rather than attempted" is the load-bearing half**, and it is checked by
/// the fetch stub recording every call it receives: a gate that fires after the
/// download is a gate that has already spent the user's bandwidth and, on a
/// paid connection, their money.
#[test]
fn an_unsupported_host_is_named_rather_than_attempted() {
    let node = node_or_skip!("the installer's runtime host gate");
    let sandbox = Sandbox::new(&node);
    sandbox.pretend_host("linux", "x64");

    let out = sandbox.run(&["--install"]);
    assert_eq!(exit_code(&out), 1, "an unservable host is not a success: {}", stderr(&out));
    assert!(
        stderr(&out).contains("linux-x64"),
        "it does not name the host it will not serve: {}",
        stderr(&out)
    );
    assert!(
        stderr(&out).contains("darwin-arm64"),
        "it does not name the host it does serve, so the user learns nothing about why: {}",
        stderr(&out)
    );
    assert!(sandbox.fetched().is_empty(), "it downloaded before checking the host: {:?}", sandbox.fetched());
    assert!(sandbox.bin_entries().is_empty(), "it left something behind: {:?}", sandbox.bin_entries());

    // CONTROL: the same sandbox on the host it does serve installs. Without
    // this, a `--install` broken for any other reason would pass the three
    // assertions above.
    sandbox.serve_the_asset();
    let served = sandbox.run(&["--install", "--no-configure"]);
    assert_eq!(exit_code(&served), 0, "the sandbox cannot install at all, so the gate above proved nothing: {}", stderr(&served));
}

/// **`--configure` and `--no-configure` together is an error, not a ranking.**
///
/// There is no correct precedence. Whichever won, half the scripts that wrote
/// both would silently do the opposite of what they say — and the thing being
/// decided writes three keys into a file this tool does not own, replacing a
/// status line belonging to another tool with no undo.
///
/// The binary's own `--configure` is the one surface in this project that
/// already refuses an argument it does not understand, and this follows it
/// rather than inventing a second convention.
///
/// The single-flag answers are the control: without them, an implementation
/// that errored on *every* input would pass.
#[test]
fn configure_and_no_configure_together_is_refused_rather_than_ranked() {
    let node = node_or_skip!("the installer's consent contradiction");
    let sandbox = Sandbox::new(&node);

    let both = sandbox.probe("parseArgs([\"--install\", \"--configure\", \"--no-configure\"])");
    let error = both["error"].as_str().unwrap_or("");
    assert!(
        error.contains("--configure") && error.contains("--no-configure"),
        "the contradiction is not reported as itself: {both}"
    );

    for (args, expected) in [
        (r#"["--install", "--configure"]"#, "yes"),
        (r#"["--install", "--no-configure"]"#, "no"),
        (r#"["--install"]"#, "ask"),
    ] {
        let parsed = sandbox.probe(&format!("parseArgs({args})"));
        assert!(parsed["error"].is_null(), "{args} was refused, so the refusal above is not about the contradiction: {parsed}");
        assert_eq!(parsed["configure"].as_str(), Some(expected), "{args} decided the wrong consent state");
    }

    // Refused by the program, not only by the parser — and nothing downloaded
    // on the way to refusing.
    let out = sandbox.run(&["--install", "--configure", "--no-configure"]);
    assert_eq!(exit_code(&out), 1, "the contradiction was ranked rather than refused: {}", stderr(&out));
    assert!(sandbox.fetched().is_empty(), "it started downloading before reading its arguments");
    assert!(sandbox.bin_entries().is_empty(), "it installed on a contradiction: {:?}", sandbox.bin_entries());
}

// ---------------------------------------------------------------------------
// Which channel already owns the binary
// ---------------------------------------------------------------------------

/// **`Cellar` is a path segment, never a substring.**
///
/// Homebrew's binaries live under `.../Cellar/<formula>/<version>/bin`, and
/// matching that with `includes("/Cellar/")` also claims
/// `~/Projects/MyCellarThing/bin`. The cost is not cosmetic: a user whose
/// binary brew has never heard of is told to run `brew upgrade claude-status`,
/// which fails, and the installer exits 0 having done nothing.
///
/// The two directory prefixes are both real — Homebrew's prefix differs between
/// Apple Silicon and Intel, and the classifier reads neither.
#[test]
fn a_cellar_path_is_classified_as_homebrew() {
    let node = node_or_skip!("the Homebrew classification");
    let sandbox = pure(&node);

    for path in [
        "/opt/homebrew/Cellar/claude-status/1.1.0/bin/claude-status",
        "/usr/local/Cellar/claude-status/1.1.0/bin/claude-status",
    ] {
        let answer = sandbox.probe(&format!("classifyExisting({{ resolvedPath: {path:?}, miseWhich: null }})"));
        assert_eq!(answer.as_str(), Some("homebrew"), "{path} is Homebrew's and was not read as it");
    }

    for path in [
        "/Users/someone/Projects/MyCellarThing/bin/claude-status",
        "/Users/someone/Cellars/bin/claude-status",
        "/Users/someone/.local/bin/claude-status",
    ] {
        let answer = sandbox.probe(&format!("classifyExisting({{ resolvedPath: {path:?}, miseWhich: null }})"));
        assert_eq!(
            answer.as_str(),
            Some("unknown"),
            "{path} was called Homebrew's — the user would be told to run `brew upgrade` on a binary brew does not have"
        );
    }
}

/// **mise is identified by agreement, because it has no path to match.**
///
/// A mise shim lives wherever `MISE_DATA_DIR` points, so there is no shape to
/// recognise — the only evidence is that `mise which` and `which` resolve to the
/// same file. Getting this wrong in the permissive direction hands mise's shim
/// to the overwrite path, where the next `mise install` silently undoes the
/// install and the user has two tools fighting over one file.
///
/// The `null` and empty cases are `run()`'s two shapes for "mise is not
/// installed", and both must read as *not mise* rather than as an agreement
/// with a resolved path that also happens to be empty.
#[test]
fn a_mise_shim_is_classified_as_mise() {
    let node = node_or_skip!("the mise classification");
    let sandbox = pure(&node);

    let shim = "/Users/someone/.local/share/mise/installs/claude-status/1.1.0/bin/claude-status";
    let answer = sandbox.probe(&format!("classifyExisting({{ resolvedPath: {shim:?}, miseWhich: {shim:?} }})"));
    assert_eq!(answer.as_str(), Some("mise"), "mise's own shim was not recognised by agreement");

    for mise_which in ["null", "\"\"", "\"/opt/elsewhere/claude-status\""] {
        let answer = sandbox.probe(&format!("classifyExisting({{ resolvedPath: {shim:?}, miseWhich: {mise_which} }})"));
        assert_eq!(
            answer.as_str(),
            Some("unknown"),
            "mise: {mise_which} was read as agreement — the installer would print `mise upgrade` at a binary mise does not own"
        );
    }

    // Homebrew wins a tie. A Cellar path that mise also happens to resolve to
    // is brew's: mise can shim a brew install, and `brew upgrade` is the
    // command that actually moves those bytes.
    let cellar = "/opt/homebrew/Cellar/claude-status/1.1.0/bin/claude-status";
    let answer = sandbox.probe(&format!("classifyExisting({{ resolvedPath: {cellar:?}, miseWhich: {cellar:?} }})"));
    assert_eq!(answer.as_str(), Some("homebrew"), "a Cellar path both agree on stopped being Homebrew's");
}

/// **A `claude-status` this installer cannot prove it placed is not overwritten.**
///
/// The receipt is the whole of the evidence: the path says we installed
/// *somewhere*, and the digest says the file has not been replaced since. With
/// neither, the file on PATH belongs to someone — a hand-built binary, a fork, a
/// different tool with the same name — and replacing it is silent destruction
/// with no undo and no backup.
///
/// `--force` is the escape hatch, and it is asserted here rather than left to
/// the help text: a refusal a user cannot get past is a refusal that gets
/// worked around with `rm`, which loses the receipt too.
#[test]
fn an_unknown_binary_is_refused_rather_than_overwritten() {
    let node = node_or_skip!("the installer's refusal to overwrite a binary it did not place");
    let sandbox = Sandbox::new(&node);

    let theirs = sandbox.placed();
    std::fs::write(&theirs, "#!/bin/sh\necho somebody else's binary\n").expect("their binary");
    std::fs::set_permissions(&theirs, std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("chmod");
    let before = std::fs::read(&theirs).expect("read back");

    let out = sandbox.run(&["--install"]);
    assert_eq!(exit_code(&out), 1, "refusing is not a success: {}", stderr(&out));
    assert!(
        stderr(&out).contains(&theirs.display().to_string()),
        "the refusal does not name the file it refused: {}",
        stderr(&out)
    );
    assert!(stderr(&out).contains("--force"), "the refusal does not say how to get past it: {}", stderr(&out));
    assert_eq!(std::fs::read(&theirs).expect("still there"), before, "it overwrote a binary it could not prove was ours");
    assert!(sandbox.fetched().is_empty(), "it downloaded before deciding it had nowhere to put the result");

    // CONTROL: `--force` really is the override, so the refusal above is a
    // decision rather than an inability.
    let forced = sandbox.run(&["--install", "--no-configure", "--force"]);
    assert_eq!(exit_code(&forced), 0, "--force did not get past the refusal: {}", stderr(&forced));
    assert_ne!(std::fs::read(&theirs).expect("replaced"), before, "--force reported success and replaced nothing");
}

/// **A matching receipt turns the refusal into an upgrade.**
///
/// This is the other side of the same guard, and it is what makes the channel
/// usable at all: without it, the second `npx @askviraj/claude-status --install`
/// a user ever runs refuses the binary the first one placed.
///
/// Both halves of the proof are checked separately, because they fail
/// differently. A receipt with the right path and a stale digest means the file
/// was replaced after we wrote it — by another installer, or by the user — and
/// it stops being ours at that moment.
///
/// The old version is printed, so an upgrade that moves nothing is visible as
/// one.
#[test]
fn a_receipt_match_is_an_upgrade_rather_than_a_refusal() {
    let node = node_or_skip!("the receipt-guarded upgrade");
    let sandbox = Sandbox::new(&node);

    // The state a previous install leaves: the binary it placed, and a receipt
    // describing it.
    let ours = sandbox.placed();
    std::fs::copy(sandbox.at("stage").join("claude-status"), &ours).expect("a previously installed binary");
    let write_receipt = |sha: &str| {
        std::fs::create_dir_all(sandbox.receipt_path().parent().expect("a parent")).expect("the state dir");
        std::fs::write(
            sandbox.receipt_path(),
            serde_json::to_string_pretty(&serde_json::json!({
                "version": "0.9.0",
                "tag": "v0.9.0",
                "path": ours.display().to_string(),
                "sha256": sha,
                "configured": false,
            }))
            .expect("JSON"),
        )
        .expect("the receipt");
    };

    write_receipt(&sha256_of(&ours));
    let out = sandbox.run(&["--install", "--no-configure"]);
    assert_eq!(exit_code(&out), 0, "a proven upgrade was refused: {}", stderr(&out));
    assert!(
        stderr(&out).contains(&format!("upgrading 0.9.0 → {CRATE_VERSION}")),
        "an upgrade did not say what it moved from and to: {}",
        stderr(&out)
    );

    // CONTROL: the digest is doing work. Same path, same receipt, a file that
    // no longer hashes to what we recorded — and it stops being ours.
    write_receipt(WRONG_DIGEST);
    let changed = sandbox.run(&["--install", "--no-configure"]);
    assert_eq!(
        exit_code(&changed),
        1,
        "a receipt naming the right path was enough on its own — the file could have been replaced by anything: {}",
        stderr(&changed)
    );
}

// ---------------------------------------------------------------------------
// Where the binary goes
// ---------------------------------------------------------------------------

/// **Nothing outside `$HOME` is ever chosen.**
///
/// This is the whole difference between this installer and one that writes into
/// a directory it shares with a package manager. `/usr/local/bin` and
/// `/opt/homebrew/bin` are on a great many PATHs, are frequently writable, and
/// are Homebrew's — a binary dropped into either is one `brew` can trample, one
/// `brew doctor` reports, and one no receipt in this package can account for.
///
/// The refusal is unconditional: a usable, on-PATH `/usr/local/bin` is still
/// declined in favour of a `~/.local/bin` that is not on PATH at all, because
/// the user can fix a PATH and cannot easily un-fight a package manager.
///
/// `/Users/someone-else` is the substring trap, one directory up from the
/// `Cellar` one: `startsWith(home)` calls it a directory under `/Users/someone`.
/// Ownership would usually catch it, and "usually" is not what an invariant
/// means.
#[test]
fn the_install_directory_never_resolves_outside_home() {
    let node = node_or_skip!("the install directory's containment in $HOME");
    let sandbox = pure(&node);

    let home = "/home/u";
    let choose = |entries: &str, usable: &str| -> serde_json::Value {
        sandbox.probe(&format!(
            "chooseInstallDir({{ pathEntries: {entries}, home: {home:?}, isUsable: {usable} }})"
        ))
    };
    let all = "() => true";

    let cases: Vec<(&str, &str, &str, bool)> = vec![
        // (PATH entries, isUsable, expected dir, expected onPath)
        (r#"["/usr/local/bin", "/opt/homebrew/bin", "/home/u/.local/bin"]"#, all, "/home/u/.local/bin", true),
        // THE ONE. Every entry is usable, every entry is on PATH, and none of
        // them is under $HOME.
        (r#"["/usr/local/bin", "/opt/homebrew/bin", "/usr/bin"]"#, all, "/home/u/.local/bin", false),
        (r#"["/home/u/bin", "/usr/local/bin"]"#, all, "/home/u/bin", true),
        // The prefix trap: another account's home is not this one's.
        (r#"["/home/u2/bin", "/home/username/bin"]"#, all, "/home/u/.local/bin", false),
        // The third rule — any PATH entry under $HOME, once the two preferred
        // ones are not there.
        (r#"["/usr/local/bin", "/home/u/tools"]"#, all, "/home/u/tools", true),
        // On PATH but not usable is not a place to install. `~/bin` is the
        // fallback the user already declared by putting it on PATH.
        (
            r#"["/home/u/.local/bin", "/home/u/bin"]"#,
            "(dir) => dir !== \"/home/u/.local/bin\"",
            "/home/u/bin",
            true,
        ),
    ];

    for (entries, usable, dir, on_path) in cases {
        let chosen = choose(entries, usable);
        assert_eq!(chosen["dir"].as_str(), Some(dir), "PATH {entries} chose the wrong directory: {chosen}");
        assert_eq!(chosen["onPath"].as_bool(), Some(on_path), "PATH {entries} misreported whether the choice is on PATH: {chosen}");

        // The invariant itself, restated over every case rather than trusted to
        // the expectations above: whatever was chosen is under $HOME.
        let chosen_dir = chosen["dir"].as_str().expect("a directory");
        assert!(
            chosen_dir.starts_with(&format!("{home}/")),
            "PATH {entries} chose {chosen_dir}, which is outside $HOME"
        );
    }
}

/// **With nowhere on PATH, it installs anyway and exits non-zero.**
///
/// A binary the user cannot yet run is worth more than no binary and an
/// explanation: the fix is one line in a shell config, and the download,
/// verification and placement are all done. Exiting 0 would be the lie — a
/// script that installed and moved on would fail later, somewhere with no
/// connection to this.
///
/// The PATH line is printed **in the shell the user is running**, because
/// `export PATH=` pasted into fish is an error message rather than a fix.
#[test]
fn no_writable_path_entry_still_installs_and_exits_nonzero() {
    let node = node_or_skip!("the no-usable-PATH-entry install");
    let sandbox = Sandbox::new(&node);

    let out = sandbox.run_as(&["--install", "--no-configure"], "/usr/bin:/bin", "/bin/zsh");
    assert_eq!(exit_code(&out), 1, "it reported success while nothing could run what it placed: {}", stderr(&out));
    assert!(sandbox.placed().is_file(), "it refused instead of installing: {}", stderr(&out));
    assert!(
        stderr(&out).contains(&format!("export PATH=\"{}", sandbox.bin().display())),
        "it did not print the line that fixes it, for the shell in $SHELL: {}",
        stderr(&out)
    );

    // The same run under fish. Nothing else about the outcome changes; the one
    // line the user is asked to copy does.
    let fish = Sandbox::new(&node);
    let out = fish.run_as(&["--install", "--no-configure"], "/usr/bin:/bin", "/opt/homebrew/bin/fish");
    assert!(
        stderr(&out).contains(&format!("fish_add_path {}", fish.bin().display())),
        "a fish user is handed an `export` line, which fish rejects: {}",
        stderr(&out)
    );
}

// ---------------------------------------------------------------------------
// The asset, the digest, and what a failure leaves behind
// ---------------------------------------------------------------------------

/// **The installer and the release workflow cannot drift on the asset name.**
///
/// The package downloads one file, by name, from one tag. That name is built in
/// `asset_name` and used by the release's collect step, the checksum manifest
/// and the Homebrew formula — and written down a second time in `asset.json`,
/// where nothing else in the repository looks at it.
///
/// Rename a target and the release keeps working, the formula keeps working,
/// and this channel 404s for every user, on every platform, forever, with the
/// release run green behind it. A 404 is not a build failure anyone sees.
///
/// **The `.tar.gz` is not incidental.** `SHA256SUMS` carries both assets for a
/// target and the raw binary's name is a strict prefix of the tarball's, which
/// is the same collision `digest_for` is anchored against.
#[test]
fn the_asset_name_matches_the_one_the_release_uploads() {
    let uploaded = format!("{}.tar.gz", ask_rust("asset_name darwin arm64"));

    let asset: serde_json::Value = serde_json::from_str(&read("npm/asset.json")).expect("npm/asset.json is JSON");
    assert_eq!(
        asset["name"].as_str(),
        Some(uploaded.as_str()),
        "the package downloads an asset the release does not upload — every install on this channel would 404"
    );

    let digest = asset["sha256"].as_str().unwrap_or("");
    assert!(
        digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
        "npm/asset.json's pin is not a sha256: {digest:?}"
    );
    assert!(
        asset["tag"].as_str().is_some_and(|t| t.starts_with('v')),
        "npm/asset.json's tag is not a release tag: {}",
        asset["tag"]
    );
}

/// **A digest mismatch is fatal, and says so in those words.**
///
/// A release asset is mutable — it can be deleted and re-uploaded at the same
/// URL — and an npm version is not. That asymmetry is the entire reason the
/// digest is pinned inside the published package rather than fetched beside the
/// binary, and it is also why "try again" is the wrong instruction: the same URL
/// will serve the same wrong bytes, so retrying converts a caught substitution
/// into a user waiting for it to clear.
///
/// Both digests are printed. A mismatch reported as "verification failed" is
/// something a user files as flaky; a mismatch reported as two hex strings is
/// something they can take to whoever owns the release.
#[test]
fn a_digest_mismatch_is_fatal_and_says_not_to_retry() {
    let node = node_or_skip!("the digest check on the downloaded asset");
    let sandbox = Sandbox::new(&node);
    sandbox.pin(WRONG_DIGEST);

    let out = sandbox.run(&["--install"]);
    let said = stderr(&out);
    assert_eq!(exit_code(&out), 1, "substituted bytes were installed: {said}");
    assert_eq!(sandbox.fetched().len(), 1, "the asset was not actually downloaded, so nothing was verified: {said}");
    assert!(said.contains("DO NOT RETRY"), "it does not say the one thing that matters: {said}");
    assert!(said.contains(WRONG_DIGEST), "it does not print the digest it expected: {said}");
    assert!(said.contains(&sandbox.tarball_digest), "it does not print the digest it got: {said}");
}

/// **A verification that fails leaves the install directory as it found it.**
///
/// The binary is unpacked into a staging directory *inside* the install
/// directory and moved in with `rename`, which is atomic only within one
/// filesystem — the reason the staging is not in `$TMPDIR`, where `$HOME` on its
/// own volume gives `EXDEV`. What that buys is this: no window in which a
/// `claude-status` on the user's PATH is half a file, and no debris left when
/// the download or the digest fails.
///
/// A dot-prefixed staging directory left behind would be invisible in `ls` and
/// permanent, so the whole directory listing is compared rather than just the
/// binary's absence.
///
/// Two failure shapes, because they abort at different points: a 404 never
/// writes the tarball, and a digest mismatch writes it and then refuses.
#[test]
fn a_failed_verification_leaves_nothing_on_path() {
    let node = node_or_skip!("the cleanup after a failed install");

    let mismatched = Sandbox::new(&node);
    mismatched.pin(WRONG_DIGEST);
    let out = mismatched.run(&["--install"]);
    assert_eq!(exit_code(&out), 1, "{}", stderr(&out));
    assert!(
        mismatched.bin_entries().is_empty(),
        "a refused download left {:?} in the install directory",
        mismatched.bin_entries()
    );

    let missing = Sandbox::new(&node);
    missing.serve_a_404();
    let out = missing.run(&["--install"]);
    assert_eq!(exit_code(&out), 1, "a 404 was not a failure: {}", stderr(&out));
    assert!(stderr(&out).contains("404"), "it does not report what the server said: {}", stderr(&out));
    assert!(
        missing.bin_entries().is_empty(),
        "a failed download left {:?} in the install directory",
        missing.bin_entries()
    );

    // CONTROL: a successful install leaves exactly the binary, so "empty" above
    // is a cleaned-up directory and not a broken sandbox.
    let good = Sandbox::new(&node);
    let out = good.run(&["--install", "--no-configure"]);
    assert_eq!(exit_code(&out), 0, "{}", stderr(&out));
    assert_eq!(good.bin_entries(), vec!["claude-status".to_string()], "a successful install left debris too");
}

/// **What was placed must report the version this package installs.**
///
/// `--version` prints the bare version and nothing else, which is the one output
/// shape in this project safe to match on exactly — and the release workflow's
/// "Verify the built binary" step does the same check on the same bytes.
///
/// It catches the case the digest cannot: a *correct* asset attached to the
/// wrong tag. The digest proves the bytes are the ones this package was
/// published against; only executing them proves those bytes are the version it
/// claims.
///
/// **The binary is left where it was placed**, which is deliberate and asserted:
/// this failure cannot tell which of the two is wrong, and deleting the
/// evidence is not how a user finds out.
#[test]
fn a_version_mismatch_after_install_is_a_failure() {
    let node = node_or_skip!("the post-install version check");
    let mut sandbox = Sandbox::new(&node);
    sandbox.release("9.9.9");
    sandbox.serve_the_asset();

    let out = sandbox.run(&["--install"]);
    let said = stderr(&out);
    assert_eq!(exit_code(&out), 1, "a binary claiming a different version was accepted: {said}");
    assert!(said.contains("9.9.9"), "it does not say what the binary reported: {said}");
    assert!(said.contains(CRATE_VERSION), "it does not say what this package installs: {said}");
    assert!(sandbox.placed().is_file(), "it deleted the one piece of evidence a user could look at");
    assert!(
        !sandbox.receipt_path().exists(),
        "it wrote a receipt for an install it had just declared wrong — the next run would call it an upgrade"
    );
}

/// **The receipt records the digest of the binary, not of the tarball.**
///
/// The receipt exists to answer one question on the next run: is the file on
/// PATH still the file we put there? That comparison is against the extracted
/// binary, so a receipt holding the archive's digest would never match anything
/// and would turn every upgrade into a refusal.
///
/// It lands in `~/.local/state/`, and neither of the other two directories.
/// Not `~/.config/claude-status/`, which is the one people commit to a dotfiles
/// repo — a receipt naming this machine's install path would arrive on the
/// second machine claiming a binary that is not there. Not `~/.cache/`, which
/// holds regenerable things, and clearing a cache must not strand the uninstall.
#[test]
fn the_receipt_records_the_digest_of_what_was_actually_placed() {
    let node = node_or_skip!("the install receipt");
    let sandbox = Sandbox::new(&node);

    let out = sandbox.run(&["--install", "--no-configure"]);
    assert_eq!(exit_code(&out), 0, "{}", stderr(&out));

    let receipt = sandbox.receipt();
    let placed = sha256_of(&sandbox.placed());
    assert_eq!(receipt["sha256"].as_str(), Some(placed.as_str()), "the receipt does not describe the file on PATH: {receipt}");
    assert_ne!(
        receipt["sha256"].as_str(),
        Some(sandbox.tarball_digest.as_str()),
        "the receipt holds the archive's digest, so no future run could ever match it"
    );
    assert_eq!(receipt["version"].as_str(), Some(CRATE_VERSION), "{receipt}");
    assert_eq!(receipt["tag"].as_str(), Some(format!("v{CRATE_VERSION}").as_str()), "{receipt}");
    assert_eq!(receipt["path"].as_str(), Some(sandbox.placed().display().to_string().as_str()), "{receipt}");
    assert_eq!(receipt["configured"].as_bool(), Some(false), "a declined wiring was recorded as one that ran: {receipt}");

    // The directory, not just the file: the choice between `state`, `config`
    // and `cache` is the decision, and only the path records it.
    assert!(
        sandbox.home().join(".config").read_dir().map(|mut d| d.next().is_none()).unwrap_or(true),
        "something was written under ~/.config — that directory goes into dotfiles repos"
    );
    assert!(
        !sandbox.home().join(".cache").exists(),
        "the receipt is reachable from ~/.cache, so clearing a cache would strand the uninstall"
    );
}

// ---------------------------------------------------------------------------
// Consent
// ---------------------------------------------------------------------------

/// **Wiring happens on an explicit yes, or on a terminal. Never by default.**
///
/// `--configure` writes three keys into `~/.claude/settings.json` and replaces a
/// `statusLine` belonging to another tool without asking, with no undo. That is
/// not something to do to a CI job that never said so — and a script has no
/// terminal to be asked on, so silence there is the answer rather than an
/// omission.
///
/// **The TTY branch itself is not exercised here.** Testing it needs a pty, and
/// a pty in this suite would be the only one; the two states a script can reach
/// are what this pins. The prompt's absence off a terminal is checked directly,
/// which is the half that would actually hang a pipeline if it broke.
#[test]
fn configure_runs_only_on_explicit_consent_or_a_tty() {
    let node = node_or_skip!("the installer's consent rule");

    let consented = Sandbox::new(&node);
    let out = consented.run(&["--install", "--configure"]);
    assert_eq!(exit_code(&out), 0, "{}", stderr(&out));
    assert!(
        consented.home().join("configure-ran").exists(),
        "an explicit `--configure` did not reach the binary: {}",
        stderr(&out)
    );
    assert_eq!(consented.receipt()["configured"].as_bool(), Some(true), "the receipt does not record that wiring ran");

    let silent = Sandbox::new(&node);
    let out = silent.run(&["--install"]);
    assert_eq!(exit_code(&out), 0, "{}", stderr(&out));
    assert!(
        !silent.home().join("configure-ran").exists(),
        "it wired Claude Code with nobody having said yes: {}",
        stderr(&out)
    );
    assert!(
        !stderr(&out).contains("Wire Claude Code"),
        "it asked a question on something with no terminal to answer it — a pipeline would hang here: {}",
        stderr(&out)
    );
    assert_eq!(silent.receipt()["configured"].as_bool(), Some(false), "the receipt claims wiring ran");
}

/// **`--no-configure` is a decline, not a failure.**
///
/// It exists so a script can say *no* as explicitly as it can say yes — and the
/// difference between that and just omitting the flag is that omitting it means
/// "ask", which off a terminal is indistinguishable from an install that forgot.
///
/// Exit 0 is the assertion that carries the whole point. A non-zero exit would
/// make every CI pipeline that declines wiring fail, which is the same as not
/// offering the flag.
///
/// The command is printed because a user who declines now needs it later, and
/// the alternative is a trip to the site for one line.
#[test]
fn no_configure_declines_without_prompting_and_names_the_command() {
    let node = node_or_skip!("the explicit decline");
    let sandbox = Sandbox::new(&node);

    let out = sandbox.run(&["--install", "--no-configure"]);
    assert_eq!(exit_code(&out), 0, "a decline was reported as a failure: {}", stderr(&out));
    assert!(!sandbox.home().join("configure-ran").exists(), "it wired anyway: {}", stderr(&out));
    assert!(
        !stderr(&out).contains("Wire Claude Code"),
        "it asked despite being told: {}",
        stderr(&out)
    );
    assert!(
        stderr(&out).contains("claude-status --configure"),
        "it does not name the command the user now has to run themselves: {}",
        stderr(&out)
    );
    assert!(sandbox.placed().is_file(), "declining the wiring skipped the install too");
    assert_eq!(sandbox.receipt()["configured"].as_bool(), Some(false), "{}", sandbox.receipt());
}

// ---------------------------------------------------------------------------
// Uninstall, and the round trip
// ---------------------------------------------------------------------------

/// A `~/.claude/settings.json` in the bytes both writers produce.
///
/// `_shared/json.rs` writes `serde_json::to_vec_pretty` plus a trailing
/// newline, and `JSON.stringify(_, null, 2)` plus a newline is byte-identical
/// for these shapes. Building the *before* file this way is what makes the
/// round trip a byte comparison rather than a value comparison — the stronger
/// claim, and the one the plan asked for.
fn write_settings(home: &Path, value: &serde_json::Value) -> PathBuf {
    let dir = home.join(".claude");
    std::fs::create_dir_all(&dir).expect("~/.claude");
    let path = dir.join("settings.json");
    std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(value).expect("JSON"))).expect("settings.json");
    path
}

/// The real binary's `--configure`, in a throwaway `$HOME`.
fn configure_in(home: &Path) -> Output {
    Command::new(BINARY)
        .arg("--configure")
        .env_clear()
        .env("HOME", home)
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("CLAUDE_STATUS_SPEND_URL", "http://127.0.0.1:1/never")
        .output()
        .expect("the binary runs")
}

/// **The unwire is the exact byte-inverse of what `--configure` writes.**
///
/// This is what makes it defensible to edit `~/.claude/settings.json` from a
/// second language at all. The Rust writer and the JavaScript one are two
/// implementations of one file format, with no shared code and no shared types,
/// and until this ran the round trip was something the tree asserted and could
/// not check — the reason the binary has no `--unconfigure` to this day.
///
/// **The real `--configure` runs.** A hand-built "what configure writes" would
/// be a transcription, and a transcription that drifted would leave this test
/// green while the two files disagreed — which is the precise failure it exists
/// to catch.
///
/// Byte-equality is available here in a way it is not for `--configure` itself,
/// because the *before* file is written in the same pretty form both writers
/// emit. What that buys over comparing values: key order. `serde_json`'s
/// `preserve_order` and `structuredClone` both keep it, and a JSON round trip
/// through almost anything else would not.
///
/// # Two shapes are outside the claim, on purpose
///
/// A **foreign `statusLine`** does not come back. `--configure` replaced it,
/// says so on stderr, and has no undo; setting a key inverts to deleting it,
/// and inventing a restore would mean a receipt this channel deliberately does
/// not keep for someone else's settings.
///
/// A settings file whose only `hooks` content is an **empty container** —
/// `"hooks": {}` or `"hooks": {"PostToolUse": []}` — also does not come back:
/// `--configure` fills it, and the unwire prunes what its own removals emptied,
/// so the container leaves with the hook. Measured, and recorded here rather
/// than asserted, because both shapes are degenerate and the alternative is an
/// unwire that leaves `{"hooks": {}}` litter in every real file.
#[test]
fn the_unwire_is_the_exact_inverse_of_configure() {
    let node = node_or_skip!("the configure/unwire round trip");
    let staged = pure(&node);

    let shapes = [
        // A settings file with something of everybody's in it: keys we never
        // touch, a hooks map with another event in it, and a PostToolUse group
        // whose command is one letter away from ours.
        serde_json::json!({
            "model": "opus",
            "permissions": { "allow": ["Bash(ls:*)"], "deny": [] },
            "env": { "FOO": "bar" },
            "hooks": {
                "PreToolUse": [
                    { "matcher": "Bash", "hooks": [{ "type": "command", "command": "/usr/bin/audit" }] }
                ],
                "PostToolUse": [
                    { "matcher": "Edit|Write", "hooks": [{ "type": "command", "command": "claude-statusline --hook" }] }
                ]
            }
        }),
        // No hooks map at all — the common case, and the one where the unwire
        // has to remove a container `--configure` created.
        serde_json::json!({ "model": "opus", "permissions": { "allow": [] } }),
        // Nothing at all.
        serde_json::json!({}),
    ];

    for before in shapes {
        let home = TempDir::new().expect("a temp dir");
        let path = write_settings(home.path(), &before);
        let original = std::fs::read(&path).expect("read back");

        let out = configure_in(home.path());
        assert_eq!(out.status.code(), Some(0), "--configure failed on {before}: {}", stderr(&out));

        let wired = std::fs::read_to_string(&path).expect("configure wrote it");
        // CONTROL. An unwire that returns its input unchanged inverts anything,
        // so the wiring has to be shown to have happened first.
        assert_ne!(wired.as_bytes(), original.as_slice(), "--configure changed nothing, so the round trip below is vacuous");
        for key in ["statusLine", "subagentStatusLine", "claude-status --caps-hook"] {
            assert!(wired.contains(key), "--configure did not write {key}, so the unwire has less to undo than it should: {wired}");
        }

        let unwired = staged.probe(&format!(
            "JSON.stringify(unwireSettings(JSON.parse({wired:?})).settings, null, 2) + \"\\n\""
        ));
        assert_eq!(
            unwired.as_str().expect("a string").as_bytes(),
            original.as_slice(),
            "the unwire is not the inverse of --configure.\nbefore:\n{}\nafter:\n{}",
            String::from_utf8_lossy(&original),
            unwired.as_str().unwrap_or("")
        );
    }
}

/// **Another tool's `PostToolUse` hooks survive the unwire.**
///
/// This is the half that has to be right, because it is the path that
/// *deletes*. `hooks.PostToolUse` is a shared array — every tool that hooks
/// Claude Code appends to it — so an unwire that removed the array, or removed
/// a group, or matched on a substring, would silently take another project's
/// integration out with ours, in a file this tool owns three keys of.
///
/// Ownership is read as the command's first shell word reduced to a basename,
/// which is `settings.rs::program_of`'s rule and exists because
/// `"claude-status"` as a substring also claims `claude-statusline` and
/// `claude-status-pro`. That name family is not hypothetical, and it is checked
/// here in both directions: a quoted path with a space in it is ours, and a
/// prefix of our name is not.
///
/// A group emptied by our own removals goes; a group that merely contains our
/// hook keeps its `matcher` and everything else in it.
#[test]
fn the_unwire_keeps_another_tools_posttooluse_hooks() {
    let node = node_or_skip!("the unwire's ownership rule");
    let staged = pure(&node);

    let settings = serde_json::json!({
        "hooks": {
            "PostToolUse": [
                // Ours sitting beside somebody else's, inside their group.
                { "matcher": "Edit|Write", "hooks": [
                    { "type": "command", "command": "/usr/bin/fmt" },
                    { "type": "command", "command": "claude-status --caps-hook" }
                ]},
                // A name one letter from ours, and one that merely starts with it.
                { "hooks": [
                    { "type": "command", "command": "claude-statusline --hook" },
                    { "type": "command", "command": "/opt/bin/claude-status-pro --caps-hook" }
                ]},
                // Ours, alone in a group we appended.
                { "hooks": [{ "type": "command", "command": "claude-status --caps-hook" }] },
                // Ours, at a path with a space in it — ordinary on macOS.
                { "hooks": [{ "type": "command", "command": "\"/Users/a b/bin/claude-status\" --caps-hook" }] }
            ]
        }
    });

    let answer = staged.probe(&format!("unwireSettings(JSON.parse({}))", serde_json::to_string(&settings.to_string()).expect("JSON")));
    let groups = answer["settings"]["hooks"]["PostToolUse"].as_array().expect("the array survived");

    assert_eq!(groups.len(), 2, "the wrong number of groups survived: {}", answer["settings"]);
    assert_eq!(groups[0]["matcher"].as_str(), Some("Edit|Write"), "their group's matcher was rewritten: {}", groups[0]);
    assert_eq!(
        groups[0]["hooks"].as_array().map(Vec::len),
        Some(1),
        "their group did not keep exactly their hook: {}",
        groups[0]
    );
    assert_eq!(groups[0]["hooks"][0]["command"].as_str(), Some("/usr/bin/fmt"), "{}", groups[0]);

    let survivors: Vec<&str> = groups[1]["hooks"].as_array().expect("an array").iter().map(|e| e["command"].as_str().unwrap_or("")).collect();
    assert_eq!(
        survivors,
        vec!["claude-statusline --hook", "/opt/bin/claude-status-pro --caps-hook"],
        "a tool whose name merely starts with ours had its hook deleted"
    );

    assert!(
        answer["removed"].as_array().expect("an array").iter().any(|r| r == "hooks.PostToolUse"),
        "it removed our hooks and did not report doing so: {}",
        answer["removed"]
    );
}

/// **`--uninstall` refuses a binary it cannot prove it placed.**
///
/// Deleting a `claude-status` somebody else put on PATH is the same wrong as
/// overwriting one, so it takes the same proof — and the failure is worse here,
/// because an overwrite at least leaves a working binary behind.
///
/// **The settings file is not touched by the refusal.** The unwire runs after
/// the binary is removed, so a refusal that had already rewritten
/// `~/.claude/settings.json` would leave a user with a wired-nothing: the
/// binary still there, the wiring gone, and the bar silently absent.
///
/// The control matters as much as the refusal: an uninstall that could never
/// remove anything would pass the first half.
#[test]
fn the_uninstall_refuses_a_binary_it_did_not_place() {
    let node = node_or_skip!("the uninstall's receipt guard");
    let sandbox = Sandbox::new(&node);

    let theirs = sandbox.placed();
    std::fs::write(&theirs, "#!/bin/sh\necho somebody else's binary\n").expect("their binary");
    std::fs::set_permissions(&theirs, std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("chmod");

    // A really wired settings.json, so "unchanged" below is measured against
    // something the unwire would otherwise have plenty to do to.
    write_settings(&sandbox.home(), &serde_json::json!({ "model": "opus" }));
    assert_eq!(configure_in(&sandbox.home()).status.code(), Some(0), "the fixture could not be wired");
    let settings_path = sandbox.home().join(".claude").join("settings.json");
    let wired = std::fs::read(&settings_path).expect("read back");

    let out = sandbox.run(&["--uninstall"]);
    assert_eq!(exit_code(&out), 1, "it removed a binary it could not prove was ours: {}", stderr(&out));
    assert!(theirs.is_file(), "somebody else's binary was deleted");
    assert!(stderr(&out).contains("--force"), "the refusal does not say how to get past it: {}", stderr(&out));
    assert_eq!(
        std::fs::read(&settings_path).expect("still there"),
        wired,
        "it refused to remove the binary and unwired Claude Code anyway — the bar would vanish with the binary still on PATH"
    );

    // CONTROL: with the receipt that proves it, the same command does the work.
    std::fs::create_dir_all(sandbox.receipt_path().parent().expect("a parent")).expect("the state dir");
    std::fs::write(
        sandbox.receipt_path(),
        serde_json::to_string_pretty(&serde_json::json!({
            "version": CRATE_VERSION,
            "tag": format!("v{CRATE_VERSION}"),
            "path": theirs.display().to_string(),
            "sha256": sha256_of(&theirs),
            "configured": true,
        }))
        .expect("JSON"),
    )
    .expect("the receipt");

    let out = sandbox.run(&["--uninstall"]);
    assert_eq!(exit_code(&out), 0, "a proven uninstall failed: {}", stderr(&out));
    assert!(!theirs.exists(), "the binary is still on PATH: {}", stderr(&out));
    assert!(!sandbox.receipt_path().exists(), "the receipt outlived the install it describes");
    let after = std::fs::read_to_string(&settings_path).expect("still there");
    assert!(!after.contains("claude-status"), "the wiring survived the uninstall: {after}");
}

// ---------------------------------------------------------------------------
// The publish job
// ---------------------------------------------------------------------------

/// **The published digest comes from the release, and is written by a script.**
///
/// The npm package carries no binary. Its whole integrity story is three fields
/// in `asset.json`, injected at publish time — so a wrong digest there is not a
/// broken build, it is a channel that either refuses every install or verifies
/// nothing, with the release run green behind it.
///
/// Two things are checked, and they are different questions. The job must read
/// the *published release's own* `SHA256SUMS` rather than rebuilding one, which
/// is the only way this channel and the tap cannot pin different bytes for one
/// tag. And the injection itself has to work — which is why it lives in a task
/// rather than in the YAML: this suite can run a script and cannot run a
/// workflow step.
///
/// **The manifest below carries both assets for the target**, because the raw
/// binary's name is a strict prefix of the tarball's and `shasum` writes the
/// shorter one first. A lookup anchored on anything less than the whole name
/// returns the raw binary's digest for a URL pointing at the tarball:
/// well-formed, plausible, and wrong.
#[test]
fn the_publish_npm_job_pins_a_digest_from_the_published_release() {
    let workflow = read(".github/workflows/release.yml");
    let publish_npm = job(&workflow, "publish-npm");

    assert!(
        publish_npm.contains("gh release download") && publish_npm.contains("SHA256SUMS"),
        "the job does not read the published release's own checksum manifest, so this channel and the tap can pin different bytes for one tag"
    );
    assert!(
        publish_npm.contains("release/npm-package"),
        "the job does not call the staging task, so the injection lives somewhere nothing in this suite can run"
    );
    assert!(
        !publish_npm.contains("shasum"),
        "the job hashes something itself — the digest must come from the release, never from bytes rebuilt here"
    );

    let dir = TempDir::new().expect("a temp dir");
    let manifest = dir.path().join("SHA256SUMS");
    let tarball_digest = "1".repeat(64);
    let raw_digest = "2".repeat(64);
    let asset = ask_rust("asset_name darwin arm64");
    std::fs::write(
        &manifest,
        format!("{raw_digest}  {asset}\n{tarball_digest}  {asset}.tar.gz\n"),
    )
    .expect("the manifest");

    let out = bash(
        &format!(
            "VERSION=9.8.7 {}/.config/mise/tasks/release/npm-package {} {}/staged",
            root().display(),
            manifest.display(),
            dir.path().display()
        ),
        &root(),
    );
    assert!(out.status.success(), "the staging task failed: {}", stderr(&out));

    let staged: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("staged").join("asset.json")).expect("a staged asset.json"))
            .expect("it is JSON");
    assert_eq!(
        staged["sha256"].as_str(),
        Some(tarball_digest.as_str()),
        "the staged package pinned the wrong asset's digest — the raw binary's name is a prefix of the tarball's, and this is what that mistake looks like"
    );
    assert_eq!(staged["name"].as_str(), Some(format!("{asset}.tar.gz").as_str()), "the staged package names the wrong asset");
    assert_eq!(staged["tag"].as_str(), Some("v9.8.7"), "the staged package names the wrong tag");

    let manifest_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("staged").join("package.json")).expect("a staged package.json"))
            .expect("it is JSON");
    assert_eq!(
        manifest_json["version"].as_str(),
        Some("9.8.7"),
        "the staged manifest kept its own version — the previous release's package would publish under this tag's digest"
    );

    // The staging is a COPY. `npm/` itself must not carry a tag's values home
    // from a release run.
    let tracked: serde_json::Value = serde_json::from_str(&read("npm/asset.json")).expect("JSON");
    assert_ne!(tracked["sha256"].as_str(), Some(tarball_digest.as_str()), "the task rewrote the tracked npm/asset.json");
}

/// **A manual dispatch cannot publish to npm.**
///
/// The sibling of `a_manual_dispatch_cannot_publish_a_release`, and it needs to
/// be stricter than its sibling for one reason: a GitHub release can be deleted
/// and re-cut, and **an npm version cannot be republished or unpublished**. A
/// dispatch that reached this job would burn a version number permanently and
/// publicly.
///
/// `needs: publish` already makes a branch dispatch unreachable, and that is
/// not enough on its own — a job that relies on another job's guard is a job
/// that breaks silently the day that guard is edited. Both are asserted here,
/// so removing either is red.
#[test]
fn a_manual_dispatch_cannot_publish_to_npm() {
    let workflow = read(".github/workflows/release.yml");
    let publish_npm = job(&workflow, "publish-npm");

    assert!(
        publish_npm.contains("github.ref_type") || publish_npm.contains("REF_TYPE"),
        "the job never checks what kind of ref it is running against, so a workflow_dispatch from a branch could publish a version that can never be taken back"
    );
    assert!(
        publish_npm.contains("needs:") && publish_npm.contains("publish"),
        "the job does not depend on `publish`, so it could publish an installer for a release that was never cut"
    );
}

/// **`publish-npm` installs no tools it does not use.**
///
/// The sibling of `the_publish_job_installs_no_tools`, and it sits even later in
/// the run: after `verify`, `test`, `build` and `publish` have all spent their
/// minutes and the release is already out. A tool download dying here is a
/// shipped release with no npm package — which is the exact shape of the
/// 2026-08-22 failure, where `mise-action` died installing `pnpm` before any
/// repo command ran.
///
/// It needs nothing: the staging task is bash, `sed` and `awk`, and `gh`, `node`
/// and `npm` all ship in the runner image.
#[test]
fn the_publish_npm_job_installs_no_tools_it_does_not_use() {
    let workflow = read(".github/workflows/release.yml");
    let publish_npm = job(&workflow, "publish-npm");

    assert!(
        publish_npm.contains("mise-action"),
        "the job no longer pins a mise version at all — this test assumes the action is present with install disabled"
    );
    assert!(
        publish_npm.contains("install: false"),
        "the job installs tools it never uses; every one is a way for a shipped release to end up with no npm package"
    );
}
