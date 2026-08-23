//! The caps, read from the same three config layers everything else uses.
//!
//! They used to be a shipped constant that a repo could only *tighten*, scraped
//! out of `<cwd>/.config/vwf.yaml` by a narrow line scan. Both halves of that
//! are gone. Caps are now ordinary config: `caps.<key>` in
//! `claude-status.json`, resolved embedded → user → repo like every other key,
//! with the repo layer winning outright.
//!
//! **That is a deliberate loosening.** The tighten-only rule existed so a repo
//! could not raise its own limits, and dropping it means a repo-level config
//! can. The tradeoff was taken knowingly: layer 3 is a file you commit and
//! review in your own repository, at the same trust level as every other
//! setting it already controls, and a caps key that behaved differently from
//! its neighbours was a surprise of its own.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::config::Config;

/// The shipped caps, as percentages. The embedded layer carries these too — the
/// constant is the fallback for a `caps` block that is absent or malformed, so
/// a broken config file behaves like one that was never written.
pub const DEFAULTS: Caps = Caps { context: 65, five_hour: 90, seven_day: 80, spend: 90 };

/// The thresholds a breach is measured against, as percentages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    pub context: u32,
    pub five_hour: u32,
    pub seven_day: u32,
    /// The monthly budget cap, also a percentage — not an amount. A budget is
    /// an account-level figure in the account's own currency; a percentage is
    /// the only form that means the same thing on every seat.
    pub spend: u32,
}

impl Default for Caps {
    fn default() -> Self {
        DEFAULTS
    }
}

impl<'de> Deserialize<'de> for Caps {
    /// Read by hand rather than derived, because every key falls back
    /// **independently**: a config setting only `caps.context` keeps the
    /// shipped values for the other three, and a `caps` block that is not an
    /// object behaves like one that was never written rather than costing the
    /// whole config.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        Ok(Caps {
            context: cap(&v, "context", DEFAULTS.context),
            five_hour: cap(&v, "fiveHour", DEFAULTS.five_hour),
            seven_day: cap(&v, "sevenDay", DEFAULTS.seven_day),
            spend: cap(&v, "spend", DEFAULTS.spend),
        })
    }
}

/// Reads the `caps` block out of the merged config.
pub fn resolve(config: &Config) -> Caps {
    config.caps
}

/// A negative or absurd number is ignored rather than clamped: `as u32` on a
/// negative float is a trap, and a cap of `-1` is a typo, not an intent.
fn cap(caps: &Value, key: &str, fallback: u32) -> u32 {
    caps.get(key)
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && *v >= 0.0 && *v <= 1000.0)
        .map_or(fallback, |v| v as u32)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn caps_from(value: serde_json::Value) -> Caps {
        resolve(&Config::new(value))
    }

    #[test]
    fn an_absent_block_is_the_shipped_defaults() {
        assert_eq!(caps_from(json!({})), DEFAULTS);
    }

    #[test]
    fn each_key_falls_back_independently() {
        let caps = caps_from(json!({ "caps": { "context": 80 } }));
        assert_eq!(caps.context, 80);
        assert_eq!(caps.five_hour, DEFAULTS.five_hour, "an unset key keeps its default");
        assert_eq!(caps.seven_day, DEFAULTS.seven_day);
        assert_eq!(caps.spend, DEFAULTS.spend);
    }

    #[test]
    fn a_cap_may_be_raised_as_well_as_lowered() {
        // The tighten-only rule is gone on purpose; both directions are config.
        assert_eq!(caps_from(json!({ "caps": { "context": 90 } })).context, 90);
        assert_eq!(caps_from(json!({ "caps": { "context": 40 } })).context, 40);
    }

    #[test]
    fn all_four_keys_are_read() {
        let caps = caps_from(json!({
            "caps": { "context": 50, "fiveHour": 85, "sevenDay": 70, "spend": 60 },
        }));
        assert_eq!(caps, Caps { context: 50, five_hour: 85, seven_day: 70, spend: 60 });
    }

    #[test]
    fn a_nonsense_value_falls_back_rather_than_clamping() {
        for bad in [json!(-1), json!("80"), json!(null), json!({}), json!(5000)] {
            assert_eq!(
                caps_from(json!({ "caps": { "context": bad } })).context,
                DEFAULTS.context,
                "a config that failed to make sense behaves like one that was never written",
            );
        }
    }

    #[test]
    fn zero_is_a_real_cap_not_a_missing_one() {
        // `0` breaches on any usage at all, which is a legitimate way to say
        // "always warn me". It must not be mistaken for unset.
        assert_eq!(caps_from(json!({ "caps": { "context": 0 } })).context, 0);
    }
}
