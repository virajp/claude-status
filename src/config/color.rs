//! Colour specs → 24-bit ANSI.
//!
//! Three accepted forms, resolved in order: a palette name, a hex string
//! (`#rgb` or `#rrggbb`), or a literal `[r, g, b]` triple. Anything
//! unresolvable falls back to `palette.white`, else to Gruvbox white.

use serde_json::{Map, Value};

pub type Rgb = [u8; 3];

/// Gruvbox `white`, used when even `palette.white` cannot be resolved.
pub const FALLBACK: Rgb = [251, 241, 199];

/// Resolves a colour spec against a palette.
///
/// **Deviation from the old implementation:** a malformed hex string such as
/// `"#12345g"` falls back to white. The JS parsed it leniently and rendered a
/// wrong colour, which is harder to notice than a wrong-but-consistent one.
pub fn resolve(spec: Option<&Value>, palette: Option<&Map<String, Value>>) -> Rgb {
    direct(spec, palette).unwrap_or_else(|| fallback(palette))
}

fn direct(spec: Option<&Value>, palette: Option<&Map<String, Value>>) -> Option<Rgb> {
    match spec? {
        // The palette is consulted *first*, so a palette that defines a key
        // spelled like a hex string still wins — matching the old resolution
        // order rather than the contract's prose ordering.
        Value::String(name) => match palette.and_then(|p| p.get(name)) {
            Some(entry) => triple(entry),
            None => name.starts_with('#').then(|| parse_hex(name)).flatten(),
        },
        arr @ Value::Array(_) => triple(arr),
        _ => None,
    }
}

fn fallback(palette: Option<&Map<String, Value>>) -> Rgb {
    palette.and_then(|p| p.get("white")).and_then(triple).unwrap_or(FALLBACK)
}

fn triple(v: &Value) -> Option<Rgb> {
    let arr = v.as_array()?;
    let [r, g, b] = arr.as_slice() else { return None };
    Some([channel(r)?, channel(g)?, channel(b)?])
}

fn channel(v: &Value) -> Option<u8> {
    let n = v.as_f64()?;
    if !n.is_finite() {
        return None;
    }
    Some(n.round().clamp(0.0, 255.0) as u8)
}

fn parse_hex(s: &str) -> Option<Rgb> {
    let body = &s[1..];
    if !body.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let pair = |i: usize| u8::from_str_radix(&body[i..i + 2], 16).ok();
    match body.len() {
        3 => {
            let d = |i: usize| u8::from_str_radix(&body[i..i + 1], 16).ok().map(|v| v * 17);
            Some([d(0)?, d(1)?, d(2)?])
        }
        6 => Some([pair(0)?, pair(2)?, pair(4)?]),
        _ => None,
    }
}

pub fn fg(rgb: Rgb) -> String {
    format!("\x1b[38;2;{};{};{}m", rgb[0], rgb[1], rgb[2])
}

pub fn bg(rgb: Rgb) -> String {
    format!("\x1b[48;2;{};{};{}m", rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn palette() -> Map<String, Value> {
        json!({ "blue": [69, 133, 136], "white": [251, 241, 199], "broken": "not-a-triple" })
            .as_object()
            .unwrap()
            .clone()
    }

    fn r(spec: Value) -> Rgb {
        resolve(Some(&spec), Some(&palette()))
    }

    #[test]
    fn a_palette_name_resolves() {
        assert_eq!(r(json!("blue")), [69, 133, 136]);
    }

    #[test]
    fn hex_resolves_in_both_widths() {
        assert_eq!(r(json!("#458588")), [69, 133, 136]);
        assert_eq!(r(json!("#abc")), [0xaa, 0xbb, 0xcc], "three digits expand by doubling");
        assert_eq!(r(json!("#ABC")), [0xaa, 0xbb, 0xcc]);
    }

    #[test]
    fn a_literal_triple_resolves() {
        assert_eq!(r(json!([69, 133, 136])), [69, 133, 136]);
    }

    #[test]
    fn the_palette_is_consulted_before_the_hex_branch() {
        let mut p = palette();
        p.insert("#458588".into(), serde_json::json!([1, 2, 3]));
        assert_eq!(resolve(Some(&json!("#458588")), Some(&p)), [1, 2, 3], "a palette entry wins over its hex reading");
    }

    #[test]
    fn a_malformed_hex_yields_white() {
        // Deviation: the JS parsed this leniently into a wrong colour.
        assert_eq!(r(json!("#12345g")), [251, 241, 199]);
        assert_eq!(r(json!("#12345")), [251, 241, 199], "an unsupported width is not guessed at");
        assert_eq!(r(json!("#")), [251, 241, 199]);
    }

    #[test]
    fn anything_unresolvable_yields_white() {
        for spec in [json!("nosuchname"), json!("broken"), json!([1, 2]), json!([1, 2, 3, 4]), json!(7), json!(null)] {
            assert_eq!(r(spec.clone()), [251, 241, 199], "{spec} should fall back");
        }
        assert_eq!(resolve(None, Some(&palette())), [251, 241, 199]);
    }

    #[test]
    fn without_a_usable_palette_the_fallback_is_gruvbox_white() {
        assert_eq!(resolve(Some(&json!("blue")), None), FALLBACK);
        let empty = Map::new();
        assert_eq!(resolve(Some(&json!("blue")), Some(&empty)), FALLBACK);
    }

    #[test]
    fn ansi_sequences_are_24_bit() {
        assert_eq!(fg([69, 133, 136]), "\x1b[38;2;69;133;136m");
        assert_eq!(bg([69, 133, 136]), "\x1b[48;2;69;133;136m");
    }
}
