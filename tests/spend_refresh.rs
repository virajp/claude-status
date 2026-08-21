//! `refresh.rs` end to end, against a stub server on loopback.
//!
//! **No test here may reach the real endpoint.** The usage endpoint throttles
//! on accumulated account usage, so a stray call is both a privacy leak and a
//! 429 the user wears for half an hour. Every case points
//! `CLAUDE_STATUS_SPEND_URL` at a local socket, and the one "unreachable" case
//! points it at a closed port.
//!
//! Plain HTTP, no TLS: the endpoint is overridable and ureq speaks both, so a
//! certificate would buy nothing but flakiness.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use claude_status::spend::cache;
use claude_status::spend::refresh::{self, Outcome};

/// A one-shot HTTP server that replies with a canned status and body.
struct Stub {
    url: String,
    hits: Arc<AtomicUsize>,
    /// Every `Authorization` header it saw, so a test can assert the token was
    /// sent without the token ever being printed.
    auth: Arc<Mutex<Vec<String>>>,
}

fn stub(status: u16, reason: &str, body: &'static str) -> Stub {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    let auth = Arc::new(Mutex::new(Vec::new()));

    let (thread_hits, thread_auth) = (Arc::clone(&hits), Arc::clone(&auth));
    let reason = reason.to_string();

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            thread_hits.fetch_add(1, Ordering::SeqCst);

            let mut buf = [0u8; 2048];
            let read = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..read]).to_string();
            // Case-insensitively: HTTP header names are case-insensitive and
            // the client is free to send `authorization`.
            for line in request.lines() {
                let (name, value) = match line.split_once(':') {
                    Some(parts) => parts,
                    None => continue,
                };
                if name.eq_ignore_ascii_case("authorization") {
                    thread_auth.lock().unwrap().push(value.trim().to_string());
                }
            }

            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    Stub { url: format!("http://127.0.0.1:{port}/usage"), hits, auth }
}

/// Points the fetch at a stub and seeds credentials, then runs one refresh.
///
/// `HOME` is redirected too, so the credentials file is the fake one and the
/// macOS keychain fallback is never reached.
fn refresh_against(url: &str, cache_path: &Path, with_credentials: bool) -> Outcome {
    // **The guard for this harness.** `http::fetch`'s own `#[cfg(test)]`
    // assertion does not apply here: `cfg(test)` is false when the library is
    // linked by an integration test, so the lib code these tests call is
    // compiled without it. This is the only in-process harness that can reach
    // the network, so the check has to live here.
    assert_ne!(
        url,
        claude_status::spend::http::DEFAULT_URL,
        "a test would have reached the real spend endpoint with a real token",
    );

    let home = tempfile::TempDir::new().unwrap();
    if with_credentials {
        let claude = home.path().join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::write(
            claude.join(".credentials.json"),
            r#"{"claudeAiOauth":{"accessToken":"stub-token","subscriptionType":"team"}}"#,
        )
        .unwrap();
    }

    // `HOME` and the endpoint override are process-global, so two of these
    // running at once would read each other's values. The lock is held across
    // both the writes and the refresh, which is the whole critical section —
    // without it the suite passes only under `--test-threads=1`, which is a
    // broken test rather than a slow one.
    let _serialised = ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    // SAFETY: every writer of these variables in this binary holds ENV_LOCK.
    unsafe {
        std::env::set_var("HOME", home.path());
        std::env::set_var("CLAUDE_STATUS_SPEND_URL", url);
    }

    refresh::run(cache_path, 15.0, 1_000_000, true)
}

/// Serialises the process-global environment these tests depend on.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn temp_cache() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("spend.json");
    (dir, path)
}

/// Port 1 is reserved and nothing listens on it. Used where a test must reach
/// the fetch path but must **not** be able to deliver a token to anything.
const CLOSED_PORT_URL: &str = "http://127.0.0.1:1/never";

const MODERN: &str = r#"{"spend":{"used":{"amount_minor":7593},"limit":{"amount_minor":15000,"exponent":2},"percent":50.62,"enabled":true}}"#;
const LEGACY: &str = r#"{"extra_usage":{"used_credits":7593,"monthly_limit":15000,"decimal_places":2,"utilization":50.62,"is_enabled":true}}"#;
const NEITHER: &str = r#"{"five_hour":{"utilization":12}}"#;

#[test]
fn a_200_with_a_spend_block_populates_the_cache() {
    let (_dir, path) = temp_cache();
    let server = stub(200, "OK", MODERN);

    assert_eq!(refresh_against(&server.url, &path, true), Outcome::Updated);

    let cached = cache::read_from(&path).expect("a cache was written");
    let data = cached.data.expect("with data");
    assert_eq!(data.used_minor, 7593.0);
    assert_eq!(data.limit_minor, 15000.0);
    assert_eq!(cached.failures, 0);
    assert_eq!(cached.backoff_until, 0);
    assert_eq!(cached.plan.as_deref(), Some("team"));

    assert_eq!(server.hits.load(Ordering::SeqCst), 1, "exactly one request");
    assert_eq!(server.auth.lock().unwrap().first().map(String::as_str), Some("Bearer stub-token"));
}

#[test]
fn a_200_with_extra_usage_populates_the_cache_too() {
    let (_dir, path) = temp_cache();
    let server = stub(200, "OK", LEGACY);

    assert_eq!(refresh_against(&server.url, &path, true), Outcome::Updated);
    assert_eq!(cache::read_from(&path).unwrap().data.unwrap().limit_minor, 15000.0);
}

#[test]
fn a_200_with_neither_shape_caches_a_null_rather_than_failing() {
    let (_dir, path) = temp_cache();
    let server = stub(200, "OK", NEITHER);

    assert_eq!(refresh_against(&server.url, &path, true), Outcome::NoBudget);

    let cached = cache::read_from(&path).expect("still writes a cache");
    assert_eq!(cached.data, None, "no budget block is an answer, not an error");
    assert_eq!(cached.failures, 0, "and not a failure");
}

#[test]
fn a_401_records_a_failure_and_keeps_the_last_good_data() {
    let (_dir, path) = temp_cache();

    // Seed a good figure first.
    let good = stub(200, "OK", MODERN);
    refresh_against(&good.url, &path, true);
    let before = cache::read_from(&path).unwrap().data;

    let expired = stub(401, "Unauthorized", r#"{"error":"invalid token"}"#);
    assert_eq!(refresh_against(&expired.url, &path, true), Outcome::Unauthorized);

    let after = cache::read_from(&path).unwrap();
    assert_eq!(after.data, before, "a failed fetch never clears a good value");
    assert_eq!(after.failures, 1);
    assert_eq!(after.backoff_until, 0);
}

#[test]
fn a_429_sets_a_backoff_from_the_incremented_failure_count() {
    let (_dir, path) = temp_cache();
    let server = stub(429, "Too Many Requests", r#"{"error":"rate limited"}"#);

    let outcome = refresh_against(&server.url, &path, true);
    let Outcome::RateLimited { backoff_until } = outcome else { panic!("expected RateLimited, got {outcome:?}") };

    // 15 minutes with one failure is thirty, not fifteen.
    assert_eq!(backoff_until, 1_000_000 + 30 * 60_000);
    assert_eq!(cache::read_from(&path).unwrap().backoff_until, backoff_until);
}

#[test]
fn a_network_error_after_a_429_erases_the_backoff() {
    let (_dir, path) = temp_cache();

    let limited = stub(429, "Too Many Requests", "{}");
    refresh_against(&limited.url, &path, true);
    assert!(cache::read_from(&path).unwrap().backoff_until > 0);

    // Port 1 is reserved; nothing listens.
    let outcome = refresh_against("http://127.0.0.1:1/never", &path, true);
    assert!(matches!(outcome, Outcome::Failed { .. }), "got {outcome:?}");

    assert_eq!(
        cache::read_from(&path).unwrap().backoff_until,
        0,
        "faithful: only the 429 branch writes a backoff, so this erases it",
    );
    assert_eq!(cache::read_from(&path).unwrap().failures, 2, "but the failure count keeps climbing");
}

#[test]
fn a_connection_refusal_is_a_failure_not_a_panic() {
    let (_dir, path) = temp_cache();
    let outcome = refresh_against("http://127.0.0.1:1/never", &path, true);
    assert!(matches!(outcome, Outcome::Failed { .. }), "got {outcome:?}");
    assert_eq!(cache::read_from(&path).unwrap().failures, 1);
}

#[test]
fn without_credentials_nothing_is_fetched() {
    let (_dir, path) = temp_cache();

    // **The closed port, not a live stub.** On macOS the keychain is the
    // credential fallback and it is **not** scoped by `$HOME`, so on a machine
    // where the user is logged into Claude Code this path finds a real token
    // even with a fake home. Pointed at a listening stub, that token was sent
    // to it as an `Authorization` header and captured — and the old `if
    // outcome == NoCredentials` guard skipped every assertion in exactly the
    // case where that happened, so the leak was invisible and the test proved
    // nothing on the machines it mattered on.
    //
    // Against a closed port nothing can receive it. The two outcomes below are
    // the two real machines this runs on: a CI box with no keychain item, and
    // a developer's laptop with one.
    let outcome = refresh_against(CLOSED_PORT_URL, &path, false);

    match outcome {
        // No keychain item: the fetch was never attempted.
        Outcome::NoCredentials => {
            assert_eq!(cache::read_from(&path).unwrap().failures, 1, "a failure must still be recorded");
        }
        // A keychain item exists, so credentials resolved and the fetch was
        // attempted against a port nothing listens on.
        Outcome::Failed { .. } => {
            assert_eq!(cache::read_from(&path).unwrap().failures, 1, "a failure must still be recorded");
        }
        other => panic!("expected NoCredentials or Failed, got {other:?}"),
    }
}
