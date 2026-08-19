//! The formatting helpers. Pure, and visible on every render.
//!
//! Several of these reproduce behaviours that look like bugs, because the bar
//! they are replacing has them and the two must render identically:
//! `human_tokens(999_999)` is `"1000k"` rather than `"1M"`, and a `gauge` width
//! configured as `0` means ten.

/// JavaScript's `Number.prototype.toFixed`.
///
/// Rust's `{:.N}` rounds half **to even**; JS rounds half **away from zero**,
/// so `(0.25).toFixed(1)` is `"0.3"` in JS and `"0.2"` in Rust. Both round the
/// *exact* binary value of the double rather than its shortest decimal
/// spelling, which is why `(1.005).toFixed(2)` is `"1.00"` in both — 1.005 is
/// really 1.00499…
///
/// So: take the exact decimal expansion, then round half-up on the digits. A
/// finite `f64` has at most 1074 fractional digits, so 1074 places is always
/// the whole expansion and never an approximation of it.
pub fn to_fixed(n: f64, digits: usize) -> String {
    if !n.is_finite() {
        return if n.is_nan() {
            "NaN".to_string()
        } else if n > 0.0 {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }

    // A finite f64 has at most 1074 fractional digits, so asking for more would
    // only pad zeros — and would then underflow the point insertion below.
    let digits = digits.min(1074);

    let exact = format!("{:.*}", 1074, n.abs());
    let (int_part, frac) = exact.split_once('.').expect("a 1074-place format always has a point");

    let mut out: Vec<u8> = int_part.bytes().chain(frac.bytes().take(digits)).collect();
    // Half-up: a dropped `5` is either an exact tie (JS rounds it away from
    // zero) or the head of something larger. Both round up.
    if frac.as_bytes().get(digits).is_some_and(|d| *d >= b'5') {
        let mut i = out.len();
        loop {
            if i == 0 {
                out.insert(0, b'1');
                break;
            }
            i -= 1;
            if out[i] == b'9' {
                out[i] = b'0';
            } else {
                out[i] += 1;
                break;
            }
        }
    }

    let mut s = String::from_utf8(out).expect("digits are ASCII");
    if digits > 0 {
        s.insert(s.len() - digits, '.');
    }
    // `(-0.0001).toFixed(2)` is `"-0.00"` — the sign survives rounding to zero.
    if n.is_sign_negative() && n != 0.0 {
        s.insert(0, '-');
    }
    s
}

/// `1234567 → "1.2M"`, `259000 → "259k"`, below 1000 → the plain number,
/// unknown → `"?"`.
///
/// Note the megabyte threshold is tested against the raw value, not the rounded
/// thousands, so `999_999` renders `"1000k"` rather than `"1M"`. That is the
/// shipped behaviour and is deliberately preserved.
pub fn human_tokens(n: Option<f64>) -> String {
    let Some(n) = n.filter(|n| !n.is_nan()) else {
        return "?".to_string();
    };

    if n >= 1e6 {
        let mut s = to_fixed(n / 1e6, 1);
        if let Some(stripped) = s.strip_suffix(".0") {
            s = stripped.to_string();
        }
        format!("{s}M")
    } else if n >= 1e3 {
        format!("{}k", js_round(n / 1e3))
    } else {
        js_number_to_string(n)
    }
}

/// `> 1h → "9hr 19m"`, `> 1m → "4m 12s"`, else `"45s"`. Nothing is zero-padded,
/// and the hours branch drops seconds entirely.
pub fn human_duration(ms: Option<f64>) -> String {
    let Some(ms) = ms.filter(|ms| !ms.is_nan()) else {
        return "?".to_string();
    };

    let total = (ms / 1000.0).floor();
    let h = (total / 3600.0).floor();
    let rem = total - h * 3600.0;
    let m = (rem / 60.0).floor();
    let s = rem - m * 60.0;

    if h > 0.0 {
        format!("{h}hr {m}m")
    } else if m > 0.0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}

/// `> 1d → "5d0h"`, `> 1h → "4h36m"` (minutes zero-padded, hours not), else
/// `"12m"`; at or past the deadline → `"now"`.
///
/// `None` in, `None` out: the caller omits the reset half of the segment rather
/// than rendering a placeholder. Seconds never appear — 45 seconds remaining
/// renders `"0m"`, not `"45s"`.
///
/// `now_ms` is passed in rather than read, so a golden test can pin a clock.
pub fn human_reset_in(resets_at_ms: Option<i64>, now_ms: i64) -> Option<String> {
    let ms = resets_at_ms?;
    // Saturating: `to_epoch_ms` saturates an absurd double to `i64::MIN`, and a
    // plain subtraction there panics in debug and wraps in release. Saturating
    // lands on "now", which is what the old implementation rendered.
    let mut s = ms.saturating_sub(now_ms).div_euclid(1000);
    if s <= 0 {
        return Some("now".to_string());
    }

    let d = s / 86_400;
    s -= d * 86_400;
    let h = s / 3600;
    s -= h * 3600;
    let m = s / 60;

    Some(if d > 0 {
        format!("{d}d{h}h")
    } else if h > 0 {
        format!("{h}h{m:02}m")
    } else {
        format!("{m}m")
    })
}

/// A fixed-width bar: `filled = round(pct/100 × width)`, clamped to 0..=100.
///
/// A `width` of `0` means ten — the old implementation resolved it with `||`,
/// so every falsy width fell back to the default, and a config that sets zero
/// still gets a ten-wide gauge.
pub fn gauge(pct: Option<f64>, width: usize, filled_glyph: &str, empty_glyph: &str) -> String {
    let width = if width == 0 { 10 } else { width };
    let pct = pct.filter(|p| !p.is_nan()).unwrap_or(0.0).clamp(0.0, 100.0);
    let filled = js_round(pct / 100.0 * width as f64).clamp(0.0, width as f64) as usize;
    filled_glyph.repeat(filled) + &empty_glyph.repeat(width - filled)
}

/// Minor units plus an exponent → `"$75.93"`, with a whole-dollar amount
/// rendering as `"$150"`.
///
/// The amount is always formatted to **two** decimals whatever the exponent,
/// and only an exact trailing `.00` is stripped — `$75.90` never shortens to
/// `$75.9`.
pub fn money(minor: f64, exp: i32) -> String {
    let amount = minor / 10f64.powi(exp);
    let s = to_fixed(amount, 2);
    format!("${}", s.strip_suffix(".00").unwrap_or(&s))
}

/// JavaScript's `Math.round`: halves go toward positive infinity, not away from
/// zero. Rust's `f64::round` rounds `-0.5` to `-1.0` where JS gives `-0`.
fn js_round(n: f64) -> f64 {
    (n + 0.5).floor()
}

/// JavaScript's `String(n)` for the values this file reaches it with — an
/// integral double prints without a `.0` suffix.
fn js_number_to_string(n: f64) -> String {
    if n.fract() == 0.0 && n.is_finite() {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_fixed_rounds_half_away_from_zero_like_js() {
        // Every expectation here was read off `node -e '(x).toFixed(d)'`.
        assert_eq!(to_fixed(1.25, 1), "1.3", "Rust's own {{:.1}} gives 1.2");
        assert_eq!(to_fixed(0.25, 1), "0.3");
        assert_eq!(to_fixed(-1.25, 1), "-1.3");
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(1.5, 0), "2");
        assert_eq!(to_fixed(2.5, 0), "3");

        // Not ties at all: the double is below the midpoint, so both languages
        // round down. This is the case people expect to be wrong and isn't.
        assert_eq!(to_fixed(1.005, 2), "1.00");
        assert_eq!(to_fixed(2.675, 2), "2.67");

        assert_eq!(to_fixed(7.0, 1), "7.0");
        assert_eq!(to_fixed(46.51, 2), "46.51");
        assert_eq!(to_fixed(0.0, 2), "0.00");
        assert_eq!(to_fixed(-0.0001, 2), "-0.00", "the sign survives rounding to zero");
        assert_eq!(to_fixed(9.99, 1), "10.0", "the carry propagates into the integer part");
        assert_eq!(to_fixed(9.99, 0), "10");
    }

    #[test]
    fn human_tokens_boundaries() {
        assert_eq!(human_tokens(Some(999_999.0)), "1000k", "not \"1M\" — the threshold is on the raw value");
        assert_eq!(human_tokens(Some(1_000_000.0)), "1M");
        assert_eq!(human_tokens(Some(1_250_000.0)), "1.3M");
        assert_eq!(human_tokens(Some(1_234_567.0)), "1.2M");
        assert_eq!(human_tokens(Some(259_000.0)), "259k");
        assert_eq!(human_tokens(Some(1_000.0)), "1k");
        assert_eq!(human_tokens(Some(999.0)), "999");
        assert_eq!(human_tokens(Some(0.0)), "0");
        assert_eq!(human_tokens(None), "?");
        assert_eq!(human_tokens(Some(f64::NAN)), "?");
    }

    #[test]
    fn human_duration_boundaries() {
        assert_eq!(human_duration(Some(33_540_000.0)), "9hr 19m");
        assert_eq!(human_duration(Some(3_600_000.0)), "1hr 0m");
        assert_eq!(human_duration(Some(252_000.0)), "4m 12s");
        assert_eq!(human_duration(Some(45_000.0)), "45s");
        assert_eq!(human_duration(Some(999.0)), "0s");
        assert_eq!(human_duration(Some(0.0)), "0s");
        assert_eq!(human_duration(None), "?");
    }

    #[test]
    fn human_reset_in_boundaries() {
        const NOW: i64 = 1_787_000_000_000;
        let at = |secs: i64| human_reset_in(Some(NOW + secs * 1000), NOW);

        assert_eq!(at(0).as_deref(), Some("now"), "exactly at the deadline");
        assert_eq!(at(-60).as_deref(), Some("now"), "already past");
        assert_eq!(at(4 * 3600 + 36 * 60).as_deref(), Some("4h36m"));
        assert_eq!(at(3600 + 6 * 60).as_deref(), Some("1h06m"), "minutes pad, hours do not");
        assert_eq!(at(5 * 60).as_deref(), Some("5m"), "minutes are unpadded in the minutes branch");
        assert_eq!(at(45).as_deref(), Some("0m"), "seconds never appear");
        assert_eq!(at(5 * 86_400).as_deref(), Some("5d0h"), "the days branch drops minutes");
        assert_eq!(at(5 * 86_400 + 2 * 3600 + 59 * 60).as_deref(), Some("5d2h"));
        assert_eq!(human_reset_in(None, NOW), None, "an unparseable timestamp omits the reset half");
    }

    #[test]
    fn gauge_clamps_and_rounds() {
        let g = |pct: Option<f64>, width| gauge(pct, width, "#", ".");
        assert_eq!(g(Some(26.0), 10), "###.......", "26% of ten rounds to three");
        assert_eq!(g(Some(-5.0), 10), "..........");
        assert_eq!(g(Some(150.0), 10), "##########");
        assert_eq!(g(Some(100.0), 10), "##########");
        assert_eq!(g(Some(0.0), 10), "..........");
        assert_eq!(g(None, 10), "..........");
        assert_eq!(g(Some(50.0), 4), "##..");
        assert_eq!(g(Some(26.0), 0), "###.......", "a width of zero means ten");
    }

    #[test]
    fn money_strips_only_a_whole_dollar_amount() {
        assert_eq!(money(7593.0, 2), "$75.93");
        assert_eq!(money(15000.0, 2), "$150");
        assert_eq!(money(1234.0, 3), "$1.23", "always two decimals, whatever the exponent");
        assert_eq!(money(7590.0, 2), "$75.90", "only an exact .00 is stripped");
        assert_eq!(money(0.0, 2), "$0");
    }
}
