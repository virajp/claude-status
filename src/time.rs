//! The clock and the timestamp normaliser.
//!
//! `resets_at` arrives as epoch seconds, epoch milliseconds, or an ISO 8601
//! string, and the three have to be told apart without a date crate.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

/// Wall-clock milliseconds since the epoch.
///
/// Called once per render, at the boundary. Everything downstream takes the
/// result as a parameter, so a golden test can pin the clock.
pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Normalises a `resets_at` value to epoch milliseconds.
///
/// - A **number** is milliseconds when `> 1e12`, otherwise seconds — including
///   `0` and negatives, which are scaled rather than rejected.
/// - A **string** parses as an RFC 3339 subset, or as a bare `YYYY-MM-DD`
///   (UTC midnight). A **numeric string** is `None`: `Date.parse("1774200000")`
///   is `NaN`, so the old implementation rejected it and so does this.
/// - Anything else — booleans, objects, arrays, `null` — is `None`.
pub fn to_epoch_ms(v: &Value) -> Option<i64> {
    match v {
        Value::Number(_) => {
            let n = v.as_f64()?;
            if !n.is_finite() {
                return None;
            }
            Some(if n > 1e12 { n as i64 } else { (n * 1000.0) as i64 })
        }
        Value::String(s) => parse_date(s),
        _ => None,
    }
}

/// The RFC 3339 subset the payload actually carries, plus a bare `YYYY-MM-DD`.
///
/// Accepted: `YYYY-MM-DD`, and `YYYY-MM-DDTHH:MM[:SS[.fff]]` followed by `Z`,
/// `±HH:MM`, `±HHMM`, or nothing. A `T` may be a space.
///
/// **Divergence:** a date-time with no offset is read as UTC. JavaScript reads
/// it as *local* time, which makes the result depend on the host's timezone.
/// Claude Code sends epoch numbers or `Z`-suffixed strings, so this is
/// unreachable in practice; determinism is worth more than the quirk here.
fn parse_date(s: &str) -> Option<i64> {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.len() < 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }

    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: u32 = s.get(5..7)?.parse().ok()?;
    let day: u32 = s.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let mut ms = days_from_civil(year, month, day) * 86_400_000;

    let rest = &s[10..];
    if rest.is_empty() {
        return Some(ms);
    }
    if !matches!(rest.as_bytes()[0], b'T' | b't' | b' ') {
        return None;
    }

    // Split the offset off the tail before reading the time of day.
    let time = &rest[1..];
    let (time, offset_ms) = match time.find(['Z', 'z']) {
        Some(i) if i == time.len() - 1 => (&time[..i], 0),
        Some(_) => return None,
        None => match time.rfind(['+', '-']) {
            Some(i) => (&time[..i], parse_offset(&time[i..])?),
            None => (time, 0),
        },
    };

    let mut parts = time.split(':');
    let hour: i64 = parts.next()?.parse().ok()?;
    let minute: i64 = parts.next()?.parse().ok()?;
    let (second, millis) = match parts.next() {
        None => (0, 0),
        Some(sec) => match sec.split_once('.') {
            None => (sec.parse::<i64>().ok()?, 0),
            Some((whole, frac)) => {
                let digits: String = frac.chars().take_while(char::is_ascii_digit).collect();
                if digits.is_empty() || digits.len() != frac.len() {
                    return None;
                }
                let scaled = format!("{digits:0<3}");
                (whole.parse::<i64>().ok()?, scaled.get(0..3)?.parse::<i64>().ok()?)
            }
        },
    };
    if parts.next().is_some() || !(0..=23).contains(&hour) || !(0..=59).contains(&minute) || !(0..=60).contains(&second) {
        return None;
    }

    ms += hour * 3_600_000 + minute * 60_000 + second * 1000 + millis;
    Some(ms - offset_ms)
}

/// `±HH:MM` or `±HHMM`, as milliseconds to subtract from the wall reading.
fn parse_offset(s: &str) -> Option<i64> {
    let sign = match s.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let body = &s[1..];
    let (hh, mm) = match body.split_once(':') {
        Some((h, m)) => (h, m),
        None if body.len() == 4 => body.split_at(2),
        None if body.len() == 2 => (body, "0"),
        None => return None,
    };
    let hours: i64 = hh.parse().ok()?;
    let minutes: i64 = mm.parse().ok()?;
    if !(0..=23).contains(&hours) || !(0..=59).contains(&minutes) {
        return None;
    }
    Some(sign * (hours * 3_600_000 + minutes * 60_000))
}

/// Days since 1970-01-01 for a proleptic Gregorian date — Howard Hinnant's
/// `days_from_civil`, which is exact and needs no table.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let (month, day) = (month as i64, day as i64);
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn numbers_discriminate_seconds_from_millis_at_1e12() {
        assert_eq!(to_epoch_ms(&json!(1_774_200_000i64)), Some(1_774_200_000_000));
        assert_eq!(to_epoch_ms(&json!(1_774_200_000_000i64)), Some(1_774_200_000_000));
        // Strictly greater, so 1e12 itself is still read as seconds.
        assert_eq!(to_epoch_ms(&json!(1_000_000_000_000i64)), Some(1_000_000_000_000_000));
    }

    #[test]
    fn zero_and_negative_numbers_are_scaled_not_rejected() {
        assert_eq!(to_epoch_ms(&json!(0)), Some(0));
        assert_eq!(to_epoch_ms(&json!(-5)), Some(-5000));
    }

    #[test]
    fn a_numeric_string_is_rejected() {
        // `Date.parse("1774200000")` is NaN, verified against node.
        assert_eq!(to_epoch_ms(&json!("1774200000")), None);
    }

    #[test]
    fn non_scalars_and_null_are_rejected() {
        for v in [json!(null), json!(true), json!(false), json!({}), json!([]), json!([1_774_200_000i64])] {
            assert_eq!(to_epoch_ms(&v), None, "{v} should not normalise");
        }
    }

    #[test]
    fn iso_strings_parse() {
        // Cross-checked against `node -e 'Date.parse(s)'`.
        assert_eq!(to_epoch_ms(&json!("2026-08-19")), Some(1_787_097_600_000));
        assert_eq!(to_epoch_ms(&json!("2026-08-19T12:00:00Z")), Some(1_787_140_800_000));
        assert_eq!(to_epoch_ms(&json!("2026-08-19T12:00:00.500Z")), Some(1_787_140_800_500));
        assert_eq!(to_epoch_ms(&json!("2026-08-19T12:00:00+02:00")), Some(1_787_133_600_000));
        assert_eq!(to_epoch_ms(&json!("2026-08-19T12:00:00-05:00")), Some(1_787_158_800_000));
        assert_eq!(to_epoch_ms(&json!("2026-08-19T12:00Z")), Some(1_787_140_800_000));
        // No offset: read as UTC here, local time in JS. See the doc comment.
        assert_eq!(to_epoch_ms(&json!("2026-08-19T12:00:00")), Some(1_787_140_800_000));
    }

    #[test]
    fn malformed_strings_are_rejected() {
        for s in ["", "not a date", "2026-08", "2026/08/19", "2026-13-01", "2026-08-19T25:00:00Z", "2026-08-19X12:00:00Z"] {
            assert_eq!(to_epoch_ms(&json!(s)), None, "{s:?} should not normalise");
        }
    }

    #[test]
    fn days_from_civil_anchors() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(1969, 12, 31), -1);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(2024, 2, 29), 19_782);
    }

    #[test]
    fn the_clock_is_plausible() {
        // 2020-01-01 in millis; a smoke test that the units are right.
        assert!(now_ms() > 1_577_836_800_000);
    }
}
