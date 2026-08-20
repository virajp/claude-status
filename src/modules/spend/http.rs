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

pub fn fetch(url: &str, token: &str) -> Response {
    // `http_status_as_error(false)`: ureq defaults to returning 4xx/5xx as
    // `Err(StatusCode)`, and this logic branches on 401 and 429, so a linear
    // match over statuses is clearer than unwrapping an error back into one.
    let agent: Agent = Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(TIMEOUT))
        .build()
        .into();

    let result = agent
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
        // SAFETY: single-threaded test.
        unsafe { std::env::remove_var(URL_ENV) };
        assert_eq!(url(), DEFAULT_URL);

        unsafe { std::env::set_var(URL_ENV, "http://127.0.0.1:1/stub") };
        assert_eq!(url(), "http://127.0.0.1:1/stub");

        unsafe { std::env::set_var(URL_ENV, "") };
        assert_eq!(url(), DEFAULT_URL, "an empty override is not an override");

        unsafe { std::env::remove_var(URL_ENV) };
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
