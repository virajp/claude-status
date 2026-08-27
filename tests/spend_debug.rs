//! `--debug` end to end: the built binary as a subprocess, fetching for real
//! against a stub on loopback.
//!
//! # The spend hazard
//!
//! **No test here may reach the real endpoint**, and `--debug` *fetches* — that
//! is the whole point of the step. Two things make it safe, and both are
//! required: `CLAUDE_STATUS_SPEND_URL` points at a stub or a closed port, and
//! the fake `$HOME` is seeded with a credentials file. The second matters
//! because the macOS keychain is **not** scoped by `$HOME`, so a home without
//! credentials would fall through to the user's real token.
//!
//! No `ENV_LOCK` is needed here, unlike `spend_refresh.rs`: every case runs the
//! binary as a child process with `env_clear()`, so the environment is per-case
//! rather than process-global.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_claude-status");

/// Port 1 is reserved and nothing listens on it.
const CLOSED_PORT_URL: &str = "http://127.0.0.1:1/never";

/// Distinctive on purpose: every case asserts this string appears on neither
/// stream, so a future `{:?}` that leaks a token fails loudly.
const TOKEN: &str = "stub-token-must-never-be-printed";

const MODERN: &str = r#"{"spend":{"used":{"amount_minor":7593},"limit":{"amount_minor":15000,"exponent":2},"enabled":true}}"#;
const NEITHER: &str = r#"{"five_hour":{"utilization":12}}"#;

/// A throwaway `$HOME` with a config layer and seeded credentials.
fn home(config: &str, plan: &str) -> TempDir {
    let dir = TempDir::new().unwrap();

    // `~/.config/claude-status/config.json`. There is no fallback to the old
    // bare path, so a fixture left at it would silently stop applying and every
    // assertion below would be testing the embedded defaults instead.
    let config_dir = dir.path().join(".config").join("claude-status");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("config.json"), config).unwrap();

    seed_credentials(dir.path(), plan);
    dir
}

fn seed_credentials(home: &Path, plan: &str) {
    let claude = home.join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(
        claude.join(".credentials.json"),
        format!(r#"{{"claudeAiOauth":{{"accessToken":"{TOKEN}","subscriptionType":"{plan}"}}}}"#),
    )
    .unwrap();
}

fn config_with(show: &str) -> String {
    format!(r#"{{ "projectName": "debug-fixture", "lines": [["model", "spend"]], "spend": {{ "show": "{show}" }} }}"#)
}

/// Runs `--debug`, returning both streams separately.
fn debug(home: &TempDir, url: &str) -> Output {
    debug_with_path(home, url, &std::env::var("PATH").unwrap_or_default())
}

/// The same, with `PATH` under the caller's control.
///
/// Pointing it at a directory that does not exist is how a test makes "no
/// credentials" **true** rather than merely likely: redirecting `$HOME` removes
/// the credentials *file*, but the macOS keychain arm shells out to `security`,
/// which is not `$HOME`-scoped and on a logged-in machine returns a real token.
/// Without this the invariant could not be asserted at all, only hoped for.
///
/// Not `PATH=""` — that is one empty entry, which resolves as the current
/// directory. See `tests/spend_refresh.rs`'s `PathGuard` for the experiment.
fn debug_with_path(home: &TempDir, url: &str, path: &str) -> Output {
    // The endpoint is passed in, so assert it here rather than trusting each
    // caller to have remembered. `http::fetch`'s own `#[cfg(test)]` check does
    // not apply to a subprocess, and the macOS keychain is not `$HOME`-scoped —
    // so a missing override means a real token to the real endpoint.
    assert_ne!(url, claude_status::spend::http::DEFAULT_URL, "this would reach the real spend endpoint");

    Command::new(BINARY)
        .arg("--debug")
        .env_clear()
        .env("HOME", home.path())
        .env("CLAUDE_STATUS_SPEND_CACHE", home.path().join(".cache").join("claude-status").join("spend.json"))
        .env("PATH", path)
        .env("CLAUDE_STATUS_SPEND_URL", url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the binary runs")
}

fn streams(out: &Output) -> (String, String) {
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// Every branch must satisfy this, which is why it is a helper rather than a
/// case: the token appears on neither stream, ever.
fn assert_no_token(out: &Output, branch: &str) {
    let (stdout, stderr) = streams(out);
    assert!(!stdout.contains(TOKEN), "the token leaked onto stdout in the {branch} branch:\n{stdout}");
    assert!(!stderr.contains(TOKEN), "the token leaked onto stderr in the {branch} branch:\n{stderr}");
}

fn cache_path(home: &TempDir) -> std::path::PathBuf {
    home.path().join(".cache").join("claude-status").join("spend.json")
}

/// A one-shot HTTP server replying with a canned status and body.
fn stub(status: u16, reason: &str, body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();
    let reason = reason.to_string();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    format!("http://127.0.0.1:{port}/usage")
}

/// An `https://` endpoint whose peer answers with bytes that are not TLS.
///
/// Enough to drive the client through a real handshake and out the other side
/// as a transport failure, without a certificate authority or a TLS server in
/// the dev-dependencies — which this repository is deliberately sparing with.
fn tls_stub() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\nnot tls at all");
        }
    });

    format!("https://127.0.0.1:{port}/usage")
}

/// **A certificate failure has to say which trust store it is about.**
///
/// `invalid peer certificate: UnknownIssuer` names no store. Behind a
/// TLS-intercepting corporate proxy it means "install your proxy's root where
/// this binary looks"; against a genuinely bad certificate it means the
/// opposite, and the two are identical on screen. The office-network report of
/// 2026-08-27 needed somebody to read `Cargo.toml` to tell them apart, which is
/// the gap this line closes.
#[test]
fn the_fetch_report_names_the_trust_root_source() {
    let home = home(&config_with("always"), "team");
    let out = debug(&home, &tls_stub());
    let (stdout, _) = streams(&out);

    assert!(
        stdout.contains(claude_status::spend::http::ROOT_CERTS),
        "the report does not say where the trust roots came from:\n{stdout}"
    );
    // The line is only worth anything next to the failure it explains.
    assert!(stdout.contains("FAILED after"), "the non-TLS peer was not reported as a failure:\n{stdout}");
    assert_no_token(&out, "tls");
}

/// **The control for the test above, and the reason it is gated on the scheme.**
///
/// Asserting the line is *present* would pass just as well if it were printed
/// unconditionally — and printed over plain `http`, where nothing verified a
/// certificate, it would be a claim about something that never happened. Every
/// other case in this file points at an `http` stub, so this pins their output
/// too.
#[test]
fn plain_http_claims_no_trust_root_source() {
    let home = home(&config_with("always"), "team");
    let out = debug(&home, &stub(200, "OK", MODERN));
    let (stdout, _) = streams(&out);

    assert!(stdout.contains("200 in"), "the stub did not answer, so this proves nothing:\n{stdout}");
    assert!(
        !stdout.contains(claude_status::spend::http::ROOT_CERTS),
        "a plain-http fetch verified no certificate, but the report named a trust store:\n{stdout}"
    );
}

#[test]
fn on_an_empty_cache_debug_fetches_and_leaves_one_behind() {
    // The case the whole step exists for: a first install, where a passive
    // --debug could only have said "no cache yet".
    let home = home(&config_with("always"), "team");
    let out = debug(&home, &stub(200, "OK", MODERN));
    let (stdout, _) = streams(&out);

    assert!(stdout.contains("MISSING — first run"), "it says the cache was absent:\n{stdout}");
    assert!(stdout.contains("200 in"), "it reports the live status:\n{stdout}");
    for gate in ["gate 1", "gate 2", "gate 3", "gate 4"] {
        assert!(stdout.contains(gate), "missing {gate}:\n{stdout}");
    }
    assert!(stdout.contains("VERDICT"), "it ends in a verdict:\n{stdout}");

    let written = std::fs::read_to_string(cache_path(&home)).expect("the fetch populated the cache");
    assert!(written.contains("7593"), "the figures landed: {written}");
    assert_no_token(&out, "200");
}

#[test]
fn a_successful_fetch_under_show_always_will_render() {
    let home = home(&config_with("always"), "team");
    let out = debug(&home, &stub(200, "OK", MODERN));
    let (stdout, _) = streams(&out);

    assert!(stdout.contains("will render"), "got:\n{stdout}");
    assert!(stdout.contains("$75.93/$150"), "the verdict carries the figure:\n{stdout}");
    assert_no_token(&out, "will-render");
}

#[test]
fn gate_four_is_named_when_a_max_seat_is_hidden_under_auto() {
    // The distinction users cannot make today: a working token that is
    // nonetheless drawn by nothing.
    let home = home(&config_with("auto"), "max");
    let out = debug(&home, &stub(200, "OK", MODERN));
    let (stdout, _) = streams(&out);

    assert!(stdout.contains("hidden by gate 4"), "got:\n{stdout}");
    assert!(stdout.contains("$75.93/$150"), "it shows the figure it refused to draw:\n{stdout}");
    assert!(stdout.contains("max"), "it names the seat:\n{stdout}");
    assert!(stdout.contains("\"always\""), "it names the way out:\n{stdout}");
    assert_no_token(&out, "gate-4");
}

#[test]
fn gate_one_is_named_when_spend_is_in_no_row() {
    let config = r#"{ "lines": [["model", "cost"]], "spend": { "show": "always" } }"#;
    let home = home(config, "team");
    let out = debug(&home, &stub(200, "OK", MODERN));
    let (stdout, _) = streams(&out);

    assert!(stdout.contains("hidden by gate 1"), "got:\n{stdout}");
    assert_no_token(&out, "gate-1");
}

#[test]
fn a_response_with_no_budget_block_says_so_rather_than_blaming_the_token() {
    let home = home(&config_with("always"), "team");
    let out = debug(&home, &stub(200, "OK", NEITHER));
    let (stdout, _) = streams(&out);

    assert!(stdout.contains("no budget block"), "got:\n{stdout}");
    assert!(stdout.contains("spend.limit.amount_minor    ✗"), "the ladder is shown:\n{stdout}");
    assert!(stdout.contains("extra_usage.monthly_limit   ✗"), "both rungs missed:\n{stdout}");
    assert_no_token(&out, "no-budget");
}

#[test]
fn a_401_blames_the_token_and_names_where_it_came_from() {
    let home = home(&config_with("always"), "team");
    let out = debug(&home, &stub(401, "Unauthorized", "{}"));
    let (stdout, _) = streams(&out);

    assert!(stdout.contains("401"), "the status is reported:\n{stdout}");
    assert!(stdout.contains("expired"), "the verdict names the cause:\n{stdout}");
    assert!(stdout.contains(".credentials.json"), "and where the token came from:\n{stdout}");
    assert_no_token(&out, "401");
}

#[test]
fn a_429_reports_the_backoff_rather_than_a_generic_failure() {
    let home = home(&config_with("always"), "team");
    let out = debug(&home, &stub(429, "Too Many Requests", "{}"));
    let (stdout, _) = streams(&out);

    assert!(stdout.contains("429"), "got:\n{stdout}");
    assert!(stdout.contains("rate limited"), "the verdict names the cause:\n{stdout}");
    assert_no_token(&out, "429");
}

#[test]
fn a_refused_connection_reports_the_transport_error() {
    let home = home(&config_with("always"), "team");
    let out = debug(&home, CLOSED_PORT_URL);
    let (stdout, _) = streams(&out);

    assert!(stdout.contains("FAILED after"), "got:\n{stdout}");
    assert!(stdout.contains("the fetch failed"), "the verdict names the cause:\n{stdout}");
    assert_no_token(&out, "refused");
}

#[test]
fn no_credentials_names_both_places_it_looked() {
    // A home with a config and no credentials file, **and** a `PATH` with no
    // `security` on it — so both arms of `creds::load` fail on every machine.
    //
    // This used to assert only the shape of the answer, because the keychain
    // arm was live and might hold a real token: the invariant in the test's own
    // name went unverified precisely on the machines where it mattered, and a
    // real OAuth token was read into the test process on every developer run.
    let dir = TempDir::new().unwrap();
    // `~/.config/claude-status/config.json`. There is no fallback to the old
    // bare path, so a fixture left at it would silently stop applying and every
    // assertion below would be testing the embedded defaults instead.
    let config_dir = dir.path().join(".config").join("claude-status");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("config.json"), config_with("always")).unwrap();

    let out = debug_with_path(&dir, CLOSED_PORT_URL, "/nonexistent/claude-status-test-path");
    let (stdout, _) = streams(&out);

    assert!(stdout.contains("creds    NONE"), "both sources must have failed:\n{stdout}");
    // The point of the test's name: it says *where* it looked, both places.
    assert!(stdout.contains(".credentials.json"), "it names the file:\n{stdout}");
    assert!(stdout.contains("keychain"), "it names the keychain:\n{stdout}");
    assert!(stdout.contains("not attempted"), "no credentials means no fetch:\n{stdout}");
    assert!(stdout.contains("VERDICT"), "it still ends in a verdict:\n{stdout}");
    assert_no_token(&out, "no-credentials");
}

#[test]
fn a_held_lock_is_reported_rather_than_waited_on() {
    let home = home(&config_with("always"), "team");

    // Take the lock the way a live refresh child would, and leave it held.
    let cache = cache_path(&home);
    std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
    let mut lock = cache.clone().into_os_string();
    lock.push(".lock");
    std::fs::write(&lock, "").unwrap();

    let started = std::time::Instant::now();
    let out = debug(&home, &stub(200, "OK", MODERN));
    let (stdout, _) = streams(&out);

    assert!(stdout.contains("HELD"), "the lock is reported:\n{stdout}");
    assert!(stdout.contains("already running"), "and the verdict explains it:\n{stdout}");
    assert!(started.elapsed().as_secs() < 30, "it did not block on the holder");
    assert!(!cache.exists(), "and it did not fetch");
    assert_no_token(&out, "locked");
}

#[test]
fn an_unwritable_cache_directory_is_reported_as_a_lock_that_could_not_be_created() {
    // The `LockUnavailable` outcome, which was the only one this file did not
    // cover — so the wording was rewritten once on reasoning alone. It is
    // reached not by an unreadable lock but by `create_new` failing for any
    // reason other than "already exists": here `PermissionDenied` on a cache
    // directory the process cannot write to.
    let home = home(&config_with("always"), "team");
    let cache = cache_path(&home);
    let dir = cache.parent().unwrap();
    std::fs::create_dir_all(dir).unwrap();

    // Read+execute, no write. Restored below so the TempDir can be cleaned up.
    use std::os::unix::fs::PermissionsExt;
    let original = std::fs::metadata(dir).unwrap().permissions();
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o500)).unwrap();

    let out = debug(&home, CLOSED_PORT_URL);
    std::fs::set_permissions(dir, original).unwrap();

    let (stdout, _) = streams(&out);

    // Scoped to the lock row. A bare `contains("unreadable")` over the whole
    // report matches the CLAUDE WIRING line — `settings.json is missing or
    // unreadable` — which is a different and entirely correct use of the word.
    let lock_line = stdout.lines().find(|l| l.trim_start().starts_with("lock")).expect("the lock stage is reported");
    assert!(lock_line.contains("could not be created or read"), "the lock row says what happened: {lock_line:?}");
    assert!(!lock_line.contains("unreadable"), "the old wording sent users hunting a stale lock: {lock_line:?}");
    assert!(
        stdout.contains("directory exists and is writable"),
        "and the verdict names the thing to check — the point of the rewording:\n{stdout}",
    );
    assert_no_token(&out, "lock-unavailable");
}

#[test]
fn debug_bypasses_the_sixty_second_dedupe() {
    // A user typing --debug twice wants two answers.
    let home = home(&config_with("always"), "team");
    let url = stub(200, "OK", MODERN);

    let first = debug(&home, &url);
    assert!(streams(&first).0.contains("200 in"), "the first fetched");

    let second = debug(&home, &url);
    let (stdout, _) = streams(&second);
    assert!(stdout.contains("200 in"), "the second fetched too, seconds later:\n{stdout}");
    assert_no_token(&second, "dedupe-bypass");
}
