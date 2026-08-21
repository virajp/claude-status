//! The spend half of `--debug`, and the reason the subsystem is diagnosable.
//!
//! **This performs a live, synchronous, foreground fetch.** It does not merely
//! report the cache, and that is deliberate: on a fresh machine the cache does
//! not exist, so the first render draws nothing and only *then* spawns the
//! detached child — whose stdio is `/dev/null`. A passive `--debug` inspecting
//! the cache at that moment could only say "no cache yet", which is precisely
//! the useless answer the user already had.
//!
//! It respects the lock and the backoff but **reports them rather than
//! silently obeying**, and it bypasses the 60-second dedupe, because a user
//! typing `--debug` twice wants two answers.
//!
//! **The token is never printed, at any verbosity.** Only where it was found.

use std::fmt::Write as _;


use crate::config::Config;
use crate::fmt::human_duration;
use crate::modules::spend::refresh::{self, Outcome};
use crate::modules::spend::{self, Gate, SpendConfig, Verdict, cache, extract};

/// Builds the `SPEND` section, fetching as it goes.
pub fn spend_report(config: &Config, now_ms: i64) -> String {
    let mut out = String::new();
    let spend_config = SpendConfig::from_config(config);
    let lines = config.lines();
    // `--debug` exists to name what is wrong, so an unresolvable `$HOME` is
    // reported here rather than silently producing an empty report.
    let Some(path) = cache::path() else {
        let _ = writeln!(out, "  cache    UNAVAILABLE — $HOME is unset, so there is nowhere to cache");
        let _ = writeln!(out, "\n  VERDICT  spend cannot work without $HOME. Nothing was fetched.");
        return out;
    };

    let _ = writeln!(out, "  cache    {}", path.display());
    let before = cache::read_from(&path);
    match before.as_ref() {
        None => {
            let _ = writeln!(out, "           MISSING — first run");
        }
        Some(cached) => {
            let age = human_duration(Some((now_ms - cached.ts) as f64));
            let _ = writeln!(out, "           written {age} ago, failures={}", cached.failures);
        }
    }

    // Reported, not obeyed: a user asking what is wrong is not served by
    // being told nothing because a timer has not expired.
    match before.as_ref().map(|c| c.backoff_until) {
        Some(until) if until > now_ms => {
            let left = human_duration(Some((until - now_ms) as f64));
            let _ = writeln!(out, "  backoff  active, {left} left — fetching anyway to diagnose");
        }
        _ => {
            let _ = writeln!(out, "  backoff  none");
        }
    }

    let report = refresh::run_reported(&path, spend_config.refresh_minutes, now_ms, true);

    match &report.outcome {
        Outcome::Locked { holder_age_secs } => {
            let _ = writeln!(out, "  lock     HELD — holder started {holder_age_secs}s ago, not waiting");
        }
        Outcome::LockUnavailable => {
            let _ = writeln!(out, "  lock     unreadable — no fetch was attempted");
        }
        _ => {
            let _ = writeln!(out, "  lock     free");
            write_credentials(&mut out, &report);
            write_fetch(&mut out, &report);
            write_extract(&mut out, &report);
        }
    }

    let after = cache::read_from(&path);
    let verdict = spend::verdict(after.as_ref(), &spend_config, &lines, config.symbol("spend"));
    write_gates(&mut out, after.as_ref(), &spend_config, &verdict);

    let _ = writeln!(out, "\n  VERDICT  {}", verdict_of(&report, &verdict, after.as_ref(), &spend_config, config));

    out
}

fn write_credentials(out: &mut String, report: &refresh::Report) {
    let Some(source) = report.source else {
        let _ = writeln!(out, "  creds    NONE — checked {} and {}", creds_file(), keychain());
        return;
    };

    let _ = writeln!(out, "  creds    {} ✓", source.describe());
    let _ = writeln!(out, "           token ✓ (not shown)  plan={}", report.plan.as_deref().unwrap_or("<none>"));
}

fn write_fetch(out: &mut String, report: &refresh::Report) {
    let _ = writeln!(out, "  fetch    GET {}", report.url);
    match report.status {
        Some(status) => {
            let _ = writeln!(out, "           {status} in {}ms", report.elapsed_ms);
        }
        None => match &report.outcome {
            Outcome::Failed { reason } => {
                let _ = writeln!(out, "           FAILED after {}ms — {reason}", report.elapsed_ms);
            }
            _ => {
                let _ = writeln!(out, "           not attempted");
            }
        },
    }
}

/// Which rung of the extraction ladder matched, shown as the ladder itself —
/// a response carrying neither shape is the case users cannot otherwise tell
/// apart from a broken token.
fn write_extract(out: &mut String, report: &refresh::Report) {
    let Some(body) = report.body.as_ref() else {
        return;
    };

    let modern = body.pointer("/spend/limit/amount_minor").is_some();
    let legacy = body.pointer("/extra_usage/monthly_limit").is_some();
    let _ = writeln!(out, "  extract  spend.limit.amount_minor    {}", tick(modern));
    let _ = writeln!(out, "           extra_usage.monthly_limit   {}", tick(legacy));

    match extract::extract(body) {
        Some(data) => {
            let _ = writeln!(
                out,
                "           used={} limit={} exp={} pct={} enabled={}",
                data.used_minor,
                data.limit_minor,
                data.exponent,
                crate::fmt::to_fixed(spend::percent_of(&data), 1),
                data.enabled.map_or("<unset>".to_string(), |e| e.to_string()),
            );
        }
        None => {
            let _ = writeln!(out, "           no budget block on this account");
        }
    }
}

/// All four gates, always all four — the point is to show which one stopped
/// it, so a gate that never ran is marked rather than omitted.
fn write_gates(
    out: &mut String,
    cached: Option<&cache::SpendCache>,
    config: &SpendConfig,
    verdict: &Verdict,
) {
    let stopped_at = match verdict {
        Verdict::Hidden { gate } => Some(*gate),
        Verdict::WillRender { .. } => None,
    };
    let reached = |gate: Gate| match stopped_at {
        None => true,
        Some(stop) => order(gate) <= order(stop),
    };
    let mark = |gate: Gate| match stopped_at {
        Some(stop) if stop == gate => "✗ HIDDEN".to_string(),
        _ if reached(gate) => tick(true).to_string(),
        _ => "— not reached".to_string(),
    };

    let data = cached.and_then(|c| c.data.as_ref());
    let plan = cached.and_then(|c| c.plan.as_deref()).unwrap_or("<none>");

    let gate_3 = match data {
        Some(d) => {
            format!("enabled={}, limitMinor={}", d.enabled.map_or("<unset>".into(), |e| e.to_string()), d.limit_minor)
        }
        None => "enabled=?, limitMinor=?".to_string(),
    };

    // One padding width for all four, so the marks form a column the eye can
    // run down — the whole value of printing every gate rather than just the
    // failing one.
    for (n, label, gate) in [
        (1, "spend present in lines".to_string(), Gate::NotInLayout),
        (2, "data present".to_string(), Gate::NoData),
        (3, gate_3, Gate::Disabled),
        (4, format!("show={}, plan={plan}", config.show), Gate::NotATeamPlan),
    ] {
        let _ = writeln!(out, "  gate {n}   {label:<38}{}", mark(gate));
    }
}

/// Gate order, so a gate after the one that stopped the walk can be reported
/// as unreached rather than as passing.
fn order(gate: Gate) -> u8 {
    match gate {
        Gate::NotInLayout => 1,
        Gate::NoData => 2,
        Gate::Disabled => 3,
        Gate::NotATeamPlan => 4,
    }
}

/// The single most useful line the tool prints: it separates "your token is
/// broken" from "your plan does not get this segment".
fn verdict_of(
    report: &refresh::Report,
    verdict: &Verdict,
    cached: Option<&cache::SpendCache>,
    spend_config: &SpendConfig,
    config: &Config,
) -> String {
    match &report.outcome {
        Outcome::Locked { holder_age_secs } => format!(
            "a refresh is already running — its lock is {holder_age_secs}s old. No fetch was made; re-run once it finishes."
        ),
        Outcome::LockUnavailable => {
            // `spend_report` returns early when there is no path, so reaching
            // here with `None` is not possible; say so rather than unwrap.
            match cache::path() {
                Some(path) => format!("the lock at {} could not be read, so no fetch was made.", path.display()),
                None => "there is no $HOME, so there is no lock to read and no fetch was made.".into(),
            }
        }
        Outcome::NoCredentials => format!(
            "no credentials — neither {} nor {} yielded a token. Log in with Claude Code, then re-run.",
            creds_file(),
            keychain(),
        ),
        Outcome::Unauthorized => format!(
            "the token from {} has expired (HTTP 401). Re-authenticate in Claude Code, then re-run. The last good figure, if any, is preserved.",
            report.source.map_or("the credential store", |s| s.describe()),
        ),
        Outcome::RateLimited { backoff_until } => format!(
            "rate limited (HTTP 429). No refresh until {} from now. The last good figure, if any, is preserved.",
            human_duration(Some((backoff_until - report.previous.as_ref().map_or(0, |p| p.ts)) as f64)),
        ),
        Outcome::Failed { reason } => {
            format!("the fetch failed — {reason}. The last good figure, if any, is preserved.")
        }
        Outcome::NoBudget => {
            "the fetch worked, and this account has no budget block at all — there is nothing to draw. That is a normal answer for most seats.".to_string()
        }
        // The dedupe is bypassed, so this is unreachable in practice.
        Outcome::Deduped => "a sibling refresh wrote the cache moments ago.".to_string(),
        Outcome::Updated => match verdict {
            Verdict::WillRender { text } => format!("will render — {text}"),
            Verdict::Hidden { gate } => hidden_verdict(*gate, cached, spend_config, config),
        },
    }
}

fn hidden_verdict(gate: Gate, cached: Option<&cache::SpendCache>, spend_config: &SpendConfig, config: &Config) -> String {
    match gate {
        Gate::NotInLayout => {
            "hidden by gate 1 — the fetch worked, but \"spend\" is in no row of `lines`. Add it to draw it.".to_string()
        }
        Gate::NoData => "hidden by gate 2 — the fetch worked but left no data in the cache.".to_string(),
        Gate::Disabled => {
            "hidden by gate 3 — the account reports no usable budget (enabled=false, or a limit of zero).".to_string()
        }
        Gate::NotATeamPlan => {
            let plan = cached.and_then(|c| c.plan.as_deref()).unwrap_or("<none>");
            let figure = cached
                .and_then(|c| c.data.as_ref())
                .map_or_else(|| "the figure".to_string(), |d| spend::render_text(d, config.symbol("spend")));
            format!(
                "hidden by gate 4 — the fetch worked and the figure is {figure}. show={} draws this only on a team or enterprise seat, and this is a {plan} seat. Set spend.show to \"always\" to draw it anyway.",
                spend_config.show,
            )
        }
    }
}

fn tick(ok: bool) -> &'static str {
    if ok { "✓" } else { "✗" }
}

fn creds_file() -> &'static str {
    crate::modules::spend::creds::Source::File.describe()
}

fn keychain() -> &'static str {
    crate::modules::spend::creds::Source::Keychain.describe()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_gate_after_the_stopping_one_is_marked_unreached() {
        let mut out = String::new();
        let config = SpendConfig { refresh_minutes: 15.0, show: "auto".into() };
        write_gates(&mut out, None, &config, &Verdict::Hidden { gate: Gate::NotInLayout });

        assert!(out.contains("gate 1"), "got:\n{out}");
        assert!(out.lines().filter(|l| l.contains("not reached")).count() == 3, "gates 2-4 never ran:\n{out}");
        assert_eq!(out.matches("HIDDEN").count(), 1, "exactly one gate is named as the cause:\n{out}");
    }

    #[test]
    fn a_rendering_verdict_marks_all_four_gates_passed() {
        let mut out = String::new();
        let config = SpendConfig { refresh_minutes: 15.0, show: "always".into() };
        write_gates(&mut out, None, &config, &Verdict::WillRender { text: "x".into() });

        assert!(!out.contains("HIDDEN"), "got:\n{out}");
        assert!(!out.contains("not reached"), "got:\n{out}");
        assert_eq!(out.matches('✓').count(), 4);
    }

    #[test]
    fn the_extraction_ladder_shows_which_rung_matched() {
        let body = serde_json::json!({
            "extra_usage": { "used_credits": 7593, "monthly_limit": 15000, "decimal_places": 2, "is_enabled": true },
        });
        let report = refresh::Report {
            outcome: Outcome::Updated,
            url: "http://stub/usage".into(),
            source: None,
            plan: None,
            status: Some(200),
            body: Some(body),
            elapsed_ms: 12,
            previous: None,
        };

        let mut out = String::new();
        write_extract(&mut out, &report);
        assert!(out.contains("spend.limit.amount_minor    ✗"), "got:\n{out}");
        assert!(out.contains("extra_usage.monthly_limit   ✓"), "got:\n{out}");
        assert!(out.contains("used=7593 limit=15000"), "got:\n{out}");
    }

    #[test]
    fn the_credential_line_never_prints_a_token() {
        let report = refresh::Report {
            outcome: Outcome::Updated,
            url: "http://stub/usage".into(),
            source: Some(crate::modules::spend::creds::Source::Keychain),
            plan: Some("max".into()),
            status: Some(200),
            body: None,
            elapsed_ms: 1,
            previous: None,
        };

        let mut out = String::new();
        write_credentials(&mut out, &report);
        assert!(out.contains("token ✓ (not shown)"), "got:\n{out}");
        assert!(out.contains("plan=max"), "got:\n{out}");
    }

    #[test]
    fn a_held_lock_is_reported_rather_than_waited_on() {
        let report = refresh::Report {
            outcome: Outcome::Locked { holder_age_secs: 14 },
            url: String::new(),
            source: None,
            plan: None,
            status: None,
            body: None,
            elapsed_ms: 0,
            previous: None,
        };
        let config = SpendConfig { refresh_minutes: 15.0, show: "auto".into() };
        let verdict = Verdict::Hidden { gate: Gate::NoData };
        let line = verdict_of(&report, &verdict, None, &config, &Config::new(serde_json::json!({})));

        assert!(line.contains("14s"), "it names the holder's age: {line}");
        assert!(line.contains("already running"), "{line}");
    }

    #[test]
    fn gate_four_names_the_plan_and_the_way_out() {
        let cached = cache::SpendCache {
            ts: 0,
            plan: Some("max".into()),
            failures: 0,
            backoff_until: 0,
            data: Some(extract::Spend {
                used_minor: 7593.0,
                limit_minor: 15000.0,
                exponent: 2,
                percent: None,
                enabled: Some(true),
            }),
        };
        let config = SpendConfig { refresh_minutes: 15.0, show: "auto".into() };
        let line = hidden_verdict(Gate::NotATeamPlan, Some(&cached), &config, &Config::new(serde_json::json!({})));

        assert!(line.contains("gate 4"), "{line}");
        assert!(line.contains("$75.93/$150"), "it shows the figure it refused to draw: {line}");
        assert!(line.contains("max"), "it names the seat: {line}");
        assert!(line.contains("\"always\""), "it names the way out: {line}");
    }
}
