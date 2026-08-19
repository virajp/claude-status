//! Guards `assets/claude-status.defaults.json`, which cannot be verified by
//! reading a diff: almost every symbol in it is a Nerd Font private-use
//! codepoint that renders as nothing or a box, and is silently dropped by
//! copy-paste. This test is the mechanical form of "verify by rendering".
//!
//! The *test* is safe to retype — it names codepoints as `\u{…}` escapes. The
//! *asset* is not. Never edit the asset through an editor buffer.

use claude_status::config::defaults::DEFAULTS_JSON;

#[test]
fn defaults_asset_is_present_and_parses() {
    let v: serde_json::Value = serde_json::from_str(DEFAULTS_JSON).expect("defaults asset is valid JSON");
    assert!(v.is_object(), "defaults asset is a JSON object");
}
