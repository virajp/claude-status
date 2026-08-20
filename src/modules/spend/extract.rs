//! Pulling the budget figures out of the usage endpoint's response.
//!
//! The endpoint has returned two different shapes over time and both are still
//! in the wild, so this is a ladder rather than a parse. An account with
//! **neither** shape has no budget block at all — a valid outcome for most
//! seats, and not an error.

use serde_json::Value;

/// The budget, in minor units, as the cache stores it.
#[derive(Debug, Clone, PartialEq)]
pub struct Spend {
    pub used_minor: f64,
    pub limit_minor: f64,
    pub exponent: i32,
    /// The endpoint's own percentage, when it supplies one. Preferred over
    /// dividing, so a `percent` of `0` is honoured rather than recomputed.
    pub percent: Option<f64>,
    pub enabled: Option<bool>,
}

/// Reads whichever shape this response carries.
///
/// `None` means the account has no budget block — not that anything failed.
pub fn extract(body: &Value) -> Option<Spend> {
    modern(body).or_else(|| legacy(body))
}

/// The current shape: a `spend` object with minor-unit amounts.
fn modern(body: &Value) -> Option<Spend> {
    let spend = body.get("spend")?;
    let limit = spend.get("limit")?;
    // The presence of the limit *amount* is what selects this rung.
    let limit_minor = limit.get("amount_minor")?.as_f64()?;

    Some(Spend {
        used_minor: spend.get("used").and_then(|u| u.get("amount_minor")).and_then(Value::as_f64).unwrap_or(0.0),
        limit_minor,
        exponent: limit.get("exponent").and_then(Value::as_i64).unwrap_or(2) as i32,
        percent: spend.get("percent").and_then(Value::as_f64),
        enabled: spend.get("enabled").and_then(Value::as_bool),
    })
}

/// The older shape: an `extra_usage` object in credits.
fn legacy(body: &Value) -> Option<Spend> {
    let extra = body.get("extra_usage")?;
    let limit_minor = extra.get("monthly_limit")?.as_f64()?;

    Some(Spend {
        used_minor: extra.get("used_credits").and_then(Value::as_f64).unwrap_or(0.0),
        limit_minor,
        exponent: extra.get("decimal_places").and_then(Value::as_i64).unwrap_or(2) as i32,
        percent: extra.get("utilization").and_then(Value::as_f64),
        enabled: extra.get("is_enabled").and_then(Value::as_bool),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn the_modern_shape_is_read_first() {
        let body = json!({
            "spend": {
                "used": { "amount_minor": 7593 },
                "limit": { "amount_minor": 15000, "exponent": 2 },
                "percent": 50.62,
                "enabled": true,
            },
            // Present too, and must lose.
            "extra_usage": { "monthly_limit": 999, "used_credits": 1 },
        });
        assert_eq!(extract(&body), Some(Spend {
            used_minor: 7593.0,
            limit_minor: 15000.0,
            exponent: 2,
            percent: Some(50.62),
            enabled: Some(true),
        }));
    }

    #[test]
    fn the_modern_shape_defaults_its_optional_fields() {
        let body = json!({ "spend": { "limit": { "amount_minor": 15000 } } });
        assert_eq!(extract(&body), Some(Spend {
            used_minor: 0.0,
            limit_minor: 15000.0,
            exponent: 2,
            percent: None,
            enabled: None,
        }));
    }

    #[test]
    fn the_legacy_shape_is_the_fallback() {
        let body = json!({
            "extra_usage": {
                "used_credits": 7593,
                "monthly_limit": 15000,
                "decimal_places": 2,
                "utilization": 50.62,
                "is_enabled": true,
            },
        });
        assert_eq!(extract(&body), Some(Spend {
            used_minor: 7593.0,
            limit_minor: 15000.0,
            exponent: 2,
            percent: Some(50.62),
            enabled: Some(true),
        }));
    }

    #[test]
    fn a_missing_limit_falls_through_to_the_next_rung() {
        // `spend` exists but carries no limit amount, so it does not select.
        let body = json!({
            "spend": { "used": { "amount_minor": 1 } },
            "extra_usage": { "monthly_limit": 15000, "used_credits": 7593 },
        });
        let got = extract(&body).expect("the legacy rung answers");
        assert_eq!(got.limit_minor, 15000.0);
        assert_eq!(got.used_minor, 7593.0);
    }

    #[test]
    fn neither_shape_is_none_rather_than_an_error() {
        // A perfectly valid response from an account with no budget block.
        assert_eq!(extract(&json!({ "five_hour": { "utilization": 12 } })), None);
        assert_eq!(extract(&json!({})), None);
        assert_eq!(extract(&json!(null)), None);
        assert_eq!(extract(&json!("a string")), None);
        assert_eq!(extract(&json!([1, 2, 3])), None);
    }

    #[test]
    fn a_zero_percent_survives_extraction() {
        // Distinguishing "the endpoint said 0" from "the endpoint said
        // nothing" is what lets the caller avoid recomputing it.
        let body = json!({ "spend": { "limit": { "amount_minor": 15000 }, "percent": 0 } });
        assert_eq!(extract(&body).unwrap().percent, Some(0.0));
    }

    #[test]
    fn a_zero_limit_extracts_and_is_left_for_the_gates_to_reject() {
        let body = json!({ "spend": { "limit": { "amount_minor": 0 } } });
        assert_eq!(extract(&body).unwrap().limit_minor, 0.0);
    }

    #[test]
    fn a_non_numeric_limit_does_not_select_the_rung() {
        let body = json!({
            "spend": { "limit": { "amount_minor": "15000" } },
            "extra_usage": { "monthly_limit": 900 },
        });
        assert_eq!(extract(&body).unwrap().limit_minor, 900.0, "the string did not qualify");
    }
}
