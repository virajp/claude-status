//! The render path's side of the spend subsystem: what a render reads, what it
//! draws, and what it spawns.
//!
//! These are the contract's §7 acceptance criteria, and every one of them is
//! about a *negative* — that the render did **not** fetch, did **not** wait,
//! did **not** read a file it had no business reading. The observable used
//! throughout is a stub on loopback: the detached child inherits the parent's
//! environment, so a spawn that happens shows up as a request, and a spawn that
//! does not happen shows up as silence.
//!
//! # The spend hazard
//!
//! `CLAUDE_STATUS_SPEND_URL` points at a stub in every case, and the fake
//! `$HOME` is seeded with credentials — the macOS keychain is not scoped by
//! `$HOME`, so a home without them would fall through to the user's real token.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_claude-status");

const TOKEN: &str = "stub-token-must-never-be-printed";

const MODERN: &str = r#"{"spend":{"used":{"amount_minor":7593},"limit":{"amount_minor":15000,"exponent":2},"enabled":true}}"#;

/// Enough of a payload for the bar to render something.
const FIXTURE: &str = r#"{"model":{"display_name":"Opus 4.8"},"session_id":"abc123"}"#;

/// How long to wait for a detached child to reach the stub. Generously above
/// the ~3ms a local round trip costs, because a false negative here would
/// assert the opposite of what the test means.
const SPAWN_GRACE: Duration = Duration::from_secs(3);

struct Stub {
    url: String,
    hits: Arc<AtomicUsize>,
}

fn stub() -> Stub {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    let thread_hits = Arc::clone(&hits);

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{MODERN}",
                MODERN.len(),
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
            // Counted *after* the reply, so a hit implies a completed request
            // rather than a connection the client abandoned.
            thread_hits.fetch_add(1, Ordering::SeqCst);
        }
    });

    Stub { url: format!("http://127.0.0.1:{port}/usage"), hits }
}

/// A throwaway `$HOME` with a config, credentials, and optionally a cache.
fn home(config: &str, cache: Option<&str>) -> TempDir {
    let dir = TempDir::new().unwrap();

    // `~/.config/claude-status/config.json`. There is no fallback to the old
    // bare path, so a fixture left at it would silently stop applying and every
    // assertion below would be testing the embedded defaults instead.
    let config_dir = dir.path().join(".config").join("claude-status");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("config.json"), config).unwrap();

    let claude = dir.path().join(".claude");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::write(
        claude.join(".credentials.json"),
        format!(r#"{{"claudeAiOauth":{{"accessToken":"{TOKEN}","subscriptionType":"team"}}}}"#),
    )
    .unwrap();

    if let Some(cache) = cache {
        let cache_dir = dir.path().join(".cache").join("claude-status");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(cache_dir.join("spend.json"), cache).unwrap();
    }

    dir
}

fn cache_path(home: &TempDir) -> std::path::PathBuf {
    home.path().join(".cache").join("claude-status").join("spend.json")
}

/// A cache holding a drawable figure, stamped `age_ms` in the past.
fn cache_aged(age_ms: i64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    format!(
        r#"{{"ts":{},"plan":"team","failures":0,"backoffUntil":0,"data":{{"usedMinor":7593,"limitMinor":15000,"exponent":2,"enabled":true}}}}"#,
        now - age_ms,
    )
}

/// `--debug` against the same stub, through the same guard as [`render`].
///
/// This existed inline as a second `Command::new(BINARY)` — the fourth
/// hand-rolled builder across this repo's tests, and the one that bypassed the
/// `assert_ne!` its sibling had just gained. That is exactly the hazard the
/// shared helper in `tests/e2e.rs` was introduced to remove, reappearing one
/// file over.
fn debug(home: &TempDir, url: &str) -> Output {
    assert_ne!(url, claude_status::spend::http::DEFAULT_URL, "this would reach the real spend endpoint");

    Command::new(BINARY)
        .arg("--debug")
        .env_clear()
        .env("HOME", home.path())
        .env("CLAUDE_STATUS_SPEND_CACHE", home.path().join(".cache").join("claude-status").join("spend.json"))
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("CLAUDE_STATUS_SPEND_URL", url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the binary runs")
}

fn render(home: &TempDir, url: &str) -> Output {
    // The endpoint is passed in, so assert it here rather than trusting each
    // caller to have remembered. `http::fetch`'s own `#[cfg(test)]` check does
    // not apply to a subprocess, and the macOS keychain is not `$HOME`-scoped —
    // so a missing override means a real token to the real endpoint.
    assert_ne!(url, claude_status::spend::http::DEFAULT_URL, "this would reach the real spend endpoint");

    let mut child = Command::new(BINARY)
        .arg("--statusline")
        .env_clear()
        .env("HOME", home.path())
        .env("CLAUDE_STATUS_SPEND_CACHE", home.path().join(".cache").join("claude-status").join("spend.json"))
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("CLAUDE_STATUS_SPEND_URL", url)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    child.stdin.take().unwrap().write_all(FIXTURE.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Waits for the stub to be hit, up to the grace period.
fn wait_for_hit(stub: &Stub) -> bool {
    let deadline = Instant::now() + SPAWN_GRACE;
    while Instant::now() < deadline {
        if stub.hits.load(Ordering::SeqCst) > 0 {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

/// Gives any child that *would* have been spawned its full chance to arrive,
/// so a "nothing was spawned" assertion is meaningful rather than merely early.
fn settle() {
    std::thread::sleep(SPAWN_GRACE);
}

const WITH_SPEND: &str = r#"{ "lines": [["model", "spend"]], "spend": { "show": "always", "refreshMinutes": 15 } }"#;
const WITHOUT_SPEND: &str = r#"{ "lines": [["model", "cost"]], "spend": { "show": "always", "refreshMinutes": 15 } }"#;

#[test]
fn gate_one_reads_no_cache_and_spawns_no_child() {
    // A user without the segment pays nothing for it.
    let home = home(WITHOUT_SPEND, None);
    let stub = stub();

    let out = render(&home, &stub.url);
    assert!(stdout(&out).contains("Opus 4.8"), "the bar still rendered: {}", stdout(&out));

    settle();
    assert_eq!(stub.hits.load(Ordering::SeqCst), 0, "no refresh child was spawned");
    assert!(!cache_path(&home).exists(), "the cache was never created, so it was never opened");
}

#[test]
fn a_stale_cache_draws_immediately_and_spawns_a_child() {
    // Sixteen minutes against a fifteen-minute interval.
    let home = home(WITH_SPEND, Some(&cache_aged(16 * 60_000)));
    let stub = stub();

    let started = Instant::now();
    let out = render(&home, &stub.url);
    let render_took = started.elapsed();

    assert!(stdout(&out).contains("$75.93/$150"), "it drew the cached figure: {}", stdout(&out));
    assert!(render_took < SPAWN_GRACE, "it did not wait for the child, took {render_took:?}");
    assert!(wait_for_hit(&stub), "a detached refresh child reached the endpoint");
}

#[test]
fn a_fresh_cache_draws_without_spawning() {
    let home = home(WITH_SPEND, Some(&cache_aged(60_000)));
    let stub = stub();

    let out = render(&home, &stub.url);
    assert!(stdout(&out).contains("$75.93/$150"), "got: {}", stdout(&out));

    settle();
    assert_eq!(stub.hits.load(Ordering::SeqCst), 0, "a fresh cache needs no refresh");
}

#[test]
fn refresh_minutes_zero_draws_the_cache_but_never_spawns() {
    // The interval disables the *spawn*, not the segment.
    let config = r#"{ "lines": [["model", "spend"]], "spend": { "show": "always", "refreshMinutes": 0 } }"#;
    let home = home(config, Some(&cache_aged(48 * 60 * 60 * 1000)));
    let stub = stub();

    let out = render(&home, &stub.url);
    assert!(stdout(&out).contains("$75.93/$150"), "the stale value is still drawn: {}", stdout(&out));

    settle();
    assert_eq!(stub.hits.load(Ordering::SeqCst), 0, "refreshMinutes 0 spawns nothing, however stale");
}

#[test]
fn a_future_backoff_suppresses_the_spawn() {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    let cache = format!(
        r#"{{"ts":{},"plan":"team","failures":3,"backoffUntil":{},"data":{{"usedMinor":7593,"limitMinor":15000,"exponent":2,"enabled":true}}}}"#,
        now - 60 * 60_000,
        now + 30 * 60_000,
    );
    let home = home(WITH_SPEND, Some(&cache));
    let stub = stub();

    let out = render(&home, &stub.url);
    assert!(stdout(&out).contains("$75.93/$150"), "the last good figure survives a backoff: {}", stdout(&out));

    settle();
    assert_eq!(stub.hits.load(Ordering::SeqCst), 0, "a live backoff blocks the spawn");
}

#[test]
fn the_rendering_process_itself_never_fetches() {
    // The invariant behind every case above, asserted directly: with no cache
    // at all the render draws nothing for spend and returns immediately, and
    // any request that follows came from the child, not the parent.
    let home = home(WITH_SPEND, None);
    let stub = stub();

    let started = Instant::now();
    let out = render(&home, &stub.url);
    let render_took = started.elapsed();

    assert!(stdout(&out).contains("Opus 4.8"), "the bar rendered: {}", stdout(&out));
    assert!(!stdout(&out).contains("$75.93"), "with no cache there is nothing to draw yet");
    assert!(render_took < Duration::from_secs(1), "the render did not block on a fetch, took {render_took:?}");
}

#[test]
fn a_render_after_a_debug_fetch_draws_the_cache_debug_populated() {
    // The pay-off of making --debug fetch: it does not merely diagnose the
    // first-install case, it fixes it. One --debug, and the bar works.
    let home = home(WITH_SPEND, None);
    let stub = stub();

    let debugged = debug(&home, &stub.url);
    assert!(stdout(&debugged).contains("200 in"), "--debug fetched: {}", stdout(&debugged));

    let out = render(&home, &stub.url);
    assert!(stdout(&out).contains("$75.93/$150"), "the render drew what --debug left behind: {}", stdout(&out));
}

#[test]
fn the_token_never_reaches_either_stream_of_a_render() {
    let home = home(WITH_SPEND, Some(&cache_aged(16 * 60_000)));
    let stub = stub();

    let out = render(&home, &stub.url);
    assert!(!stdout(&out).contains(TOKEN), "stdout leaked the token");
    assert!(!String::from_utf8_lossy(&out.stderr).contains(TOKEN), "stderr leaked the token");
}
