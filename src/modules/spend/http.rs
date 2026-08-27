//! The one network call in the whole binary.
//!
//! It runs in a detached child, never on the render path, so it may block
//! freely — which is why there is no async runtime here. Pulling in tokio for a
//! single request would work directly against the startup-time argument that
//! motivated the rewrite.
//!
//! The whole TLS tree is cold code during a render: a separate process faults
//! it in during a refresh and the rendering process never touches it.

use std::time::Duration;

use ureq::Agent;
use ureq::http::StatusCode;
use ureq::tls::RootCerts;
use ureq::tls::TlsConfig;

/// Overrides the endpoint. Renamed from the `ai-plugins` spelling.
pub const URL_ENV: &str = "CLAUDE_STATUS_SPEND_URL";

pub const DEFAULT_URL: &str = "https://api.anthropic.com/api/oauth/usage";

/// ureq's default is **no timeout**, and a child holding the lock for two
/// minutes on a hung connection is a real failure mode.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The response body is JSON of a few kilobytes; anything larger is a wrong
/// endpoint, not a big answer.
const MAX_BODY_BYTES: u64 = 1024 * 1024;

#[derive(Debug)]
pub enum Response {
    Ok(String),
    /// The token expired.
    Unauthorized,
    /// Accumulated account usage tripped the throttle.
    RateLimited,
    Unexpected(u16),
    /// Could not connect, resolve, or complete within the timeout.
    Transport(String),
}

pub fn url() -> String {
    match std::env::var(URL_ENV) {
        Ok(url) if !url.is_empty() => url,
        _ => DEFAULT_URL.to_string(),
    }
}

/// The agent every fetch goes through.
///
/// **`RootCerts::PlatformVerifier` is the load-bearing line.** ureq's rustls
/// default is `WebPkiRoots` — Mozilla's root set, compiled into the binary and
/// answerable to nothing on the machine. That is wrong for a tool a user
/// installs with `brew`: behind a TLS-intercepting corporate proxy, the chain
/// terminates at a root that MDM put in the login keychain, so `curl`, `git`
/// and Claude Code itself all succeed while this binary alone returns
/// `invalid peer certificate: UnknownIssuer` — with no flag, no environment
/// variable and no config key that could have fixed it. Delegating to the OS
/// verifier means this binary trusts exactly what the rest of the machine
/// trusts.
///
/// It is **not** a way to trust less — it is a different set of roots, not a
/// laxer check — and `https_is_negotiated_and_fails_closed` holds that, as
/// well as holding the feature flag itself on: ureq panics at *run time*, in
/// the detached child, if `RootCerts::PlatformVerifier` is set without its
/// feature, and nothing about that failure is visible at build time.
///
/// The extra cost is paid in cold code. This is a detached refresh child, never
/// the render path, so faulting in the verifier costs a render nothing —
/// which is the same argument the module doc above makes for the whole TLS
/// tree.
fn agent() -> Agent {
    // `http_status_as_error(false)`: ureq defaults to returning 4xx/5xx as
    // `Err(StatusCode)`, and this logic branches on 401 and 429, so a linear
    // match over statuses is clearer than unwrapping an error back into one.
    Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(TIMEOUT))
        .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
        .build()
        .into()
}

pub fn fetch(url: &str, token: &str) -> Response {
    // **A test must never reach the real endpoint.** The macOS keychain is not
    // scoped by `$HOME`, so a test with a fake home still finds a real token —
    // making a stray fetch a privacy leak and a 429 the user wears for half an
    // hour.
    //
    // **This covers the lib's own unit tests and nothing else.** `cfg(test)` is
    // false when this crate is compiled as the binary, and false again when it
    // is linked by an integration test under `tests/` — so an integration test
    // calling into this function gets a build without the assertion. The
    // in-process harness that can reach here, `refresh_against` in
    // `tests/spend_refresh.rs`, carries its own copy of this check for exactly
    // that reason. The harnesses that spawn the binary as a subprocess are
    // covered differently: they pass the endpoint as an environment variable on
    // the command they build.
    #[cfg(test)]
    assert_ne!(url, DEFAULT_URL, "a test reached the real spend endpoint — pin ${URL_ENV}");

    let result = agent()
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .call();

    let mut response = match result {
        Ok(response) => response,
        Err(e) => return Response::Transport(e.to_string()),
    };

    let status = response.status();
    let body = match response.body_mut().with_config().limit(MAX_BODY_BYTES).read_to_string() {
        Ok(body) => body,
        Err(e) => return Response::Transport(e.to_string()),
    };

    match status {
        StatusCode::UNAUTHORIZED => Response::Unauthorized,
        StatusCode::TOO_MANY_REQUESTS => Response::RateLimited,
        s if s.is_success() => Response::Ok(body),
        s => Response::Unexpected(s.as_u16()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_endpoint_is_overridable_for_tests() {
        // `cargo test` threads a binary's tests, so this shares the process
        // environment with every other test in it — including the ones that
        // unset `HOME`. The guard restores `URL_ENV` on drop.
        let mut env = crate::_shared::env_lock();
        env.unset(URL_ENV);
        assert_eq!(url(), DEFAULT_URL);

        env.set(URL_ENV, "http://127.0.0.1:1/stub");
        assert_eq!(url(), "http://127.0.0.1:1/stub");

        env.set(URL_ENV, "");
        assert_eq!(url(), DEFAULT_URL, "an empty override is not an override");
    }

    /// **The guard on `RootCerts::PlatformVerifier`.**
    ///
    /// Setting it is not self-enforcing: with ureq's `platform-verifier`
    /// feature off, `Cargo.toml` still builds, `agent()` still compiles, and
    /// every other test in this file still passes — because they all speak
    /// plain `http://` to a closed port and never reach TLS at all. The
    /// binary would quietly go back to the baked Mozilla roots and the office
    /// proxy would break again, with nothing red anywhere. That was measured
    /// by commenting the feature out, not assumed.
    ///
    /// So this test forces a real handshake. The listener accepts the
    /// connection and answers with bytes that are not TLS, which is enough to
    /// drive the client into its TLS stack and out the other side as a
    /// transport error. Two things are pinned at once: that `https` is
    /// genuinely negotiated rather than downgraded, and that a peer it cannot
    /// verify **fails closed** — the platform verifier is a different set of
    /// roots, never a laxer check.
    #[test]
    fn https_is_negotiated_and_fails_closed() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
        let port = listener.local_addr().expect("a bound address").port();

        // Answer the handshake with something that is definitively not TLS.
        // The thread is detached: the client hangs up first either way, and a
        // join here would only add a way for the test to block.
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                use std::io::Write;
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n\r\nnot tls at all");
            }
        });

        match fetch(&format!("https://127.0.0.1:{port}/never"), "irrelevant") {
            Response::Transport(_) => {}
            other => panic!("a non-TLS peer was not rejected — got {other:?}"),
        }
    }

    #[test]
    fn a_closed_port_is_a_transport_error_not_a_panic() {
        // Port 1 is reserved and nothing listens on it. This is also the shape
        // every test in the suite uses to guarantee no real call is made.
        match fetch("http://127.0.0.1:1/never", "irrelevant") {
            Response::Transport(_) => {}
            other => panic!("expected a transport error, got {other:?}"),
        }
    }
}
