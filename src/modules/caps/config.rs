//! The caps, read from the same three config layers everything else uses.
//!
//! They used to be a shipped constant that a repo could only *tighten*, scraped
//! out of `<cwd>/.config/vwf.yaml` by a narrow line scan. Both halves of that
//! are gone. Caps are now ordinary config: `caps.<key>` in
//! `claude-status.json`, resolved embedded → user → repo like every other key,
//! with the repo layer winning outright.
//!
//! **That was a deliberate loosening, and it has since been taken back.** The
//! tighten-only rule existed so a repo could not raise its own limits.
//! Dropping it let a repo-level config do exactly that, and the tradeoff was
//! taken knowingly: layer 3 is a file you commit and review in your own
//! repository, at the same trust level as every other setting it already
//! controls, and a caps key that behaved differently from its neighbours was a
//! surprise of its own.
//!
//! **Reversed by the `config-relocation` cycle**, which narrowed the repo layer
//! to `projectName`. Caps are not narrowed there — they are **removed from
//! layer 3 entirely**, so `caps` now resolves through embedded → user and
//! stops. The argument above still holds for a repo you wrote; it does not
//! hold for one you cloned, and the caps hook is not a rendering decision. A
//! repo raising its own context cap does not draw an odd bar — it suppresses
//! the directive that stops an agent running past its budget, which is not a
//! thing a file inside the repository should decide on the user's behalf.
//!
//! This is a **second reversal**, distinct from the styling one, and it is the
//! larger of the two: styling lost a capability nobody was using, while caps
//! lose one this module argued for on the record. Pinned by
//! `a_repo_config_can_no_longer_override_a_user_cap` in `tests/e2e.rs`.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::config::Config;

/// The shipped caps, as percentages. The embedded layer carries these too — the
/// constant is the fallback for a `caps` block that is absent or malformed, so
/// a broken config file behaves like one that was never written.
pub const DEFAULTS: Caps = Caps { context: 65, five_hour: 90, seven_day: 80, spend: 90 };

/// The thresholds a breach is measured against, as percentages.
///
/// [`serde::Serialize`] is derived where [`Deserialize`] is hand-written: reading has
/// to degrade each key independently, but writing has one shape and no
/// forgiveness to preserve. `rename_all` is what keeps the two halves agreeing
/// on `fiveHour`/`sevenDay`.
///
/// The `JsonSchema` derive is a **third** reading of the same shape, and the
/// only one that has to be told the bounds: [`cap`] enforces `0..=1000` in
/// code, where a derive cannot see it. `#[serde(default)]` is absent here — the
/// hand-written [`Deserialize`] already answers absence per key — so the
/// generated schema is told not to require any of the four.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(
    feature = "schema",
    schemars(inline, default, deny_unknown_fields, rename_all = "camelCase", description = "")
)]
#[serde(rename_all = "camelCase")]
pub struct Caps {
    #[cfg_attr(feature = "schema", schemars(range(min = 0, max = 1000)))]
    #[cfg_attr(feature = "schema", schemars(description = "Percent of the context window. Default 65."))]
    pub context: u32,
    #[cfg_attr(feature = "schema", schemars(range(min = 0, max = 1000)))]
    #[cfg_attr(feature = "schema", schemars(description = "Percent of the 5-hour rate-limit window. Default 90."))]
    pub five_hour: u32,
    #[cfg_attr(feature = "schema", schemars(range(min = 0, max = 1000)))]
    #[cfg_attr(feature = "schema", schemars(description = "Percent of the 7-day rate-limit window. Default 80."))]
    pub seven_day: u32,
    /// The monthly budget cap, also a percentage — not an amount. A budget is
    /// an account-level figure in the account's own currency; a percentage is
    /// the only form that means the same thing on every seat.
    #[cfg_attr(feature = "schema", schemars(range(min = 0, max = 1000)))]
    #[cfg_attr(feature = "schema", schemars(description = "Percent of the account's monthly budget. Default 90. Only ever breaches on a seat that has a budget block — team and enterprise — and the figure comes from the spend cache the refresh child maintains, never from a fetch on the hook path. Checked before the other three, because a budget does not reset on a timer."))]
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
