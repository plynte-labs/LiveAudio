// SPDX-License-Identifier: MIT

//! Validation for user subtitle style tokens and named presets.
//!
//! Rust is the source of truth: values are checked here before they are saved
//! or sent to OBS. The overlay validates them again (defense in depth).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_SUBTITLE_PRESETS: usize = 20;
pub const MAX_PRESET_NAME_CHARS: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SubtitlePreset {
    pub name: String,
    /// Built-in base style the preset was saved with; `None` keeps the current style.
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub tokens: BTreeMap<String, String>,
}

const MAX_VALUE_LEN: usize = 200;

#[derive(Clone, Copy)]
enum Kind {
    Color,
    Px { min: f64, max: f64 },
    Seconds { min: f64, max: f64 },
    Weight,
    Shadow,
    Border,
    Padding,
    Animation,
    Transform,
    Spacing,
    Font,
}

const TOKENS: &[(&str, Kind)] = &[
    ("--sub-bg", Kind::Color),
    ("--sub-color", Kind::Color),
    (
        "--sub-font-size",
        Kind::Px {
            min: 12.0,
            max: 120.0,
        },
    ),
    ("--sub-font-weight", Kind::Weight),
    (
        "--sub-radius",
        Kind::Px {
            min: 0.0,
            max: 50.0,
        },
    ),
    ("--sub-shadow", Kind::Shadow),
    ("--sub-border", Kind::Border),
    ("--sub-padding", Kind::Padding),
    ("--sub-animation", Kind::Animation),
    (
        "--sub-animation-duration",
        Kind::Seconds { min: 0.1, max: 2.0 },
    ),
    (
        "--sub-lifetime",
        Kind::Seconds {
            min: 1.5,
            max: 15.0,
        },
    ),
    ("--sub-font-family", Kind::Font),
    ("--sub-text-transform", Kind::Transform),
    ("--sub-letter-spacing", Kind::Spacing),
    ("--sub-word-spacing", Kind::Spacing),
];

/// The token names the overlay accepts, in table order.
pub fn token_names() -> Vec<&'static str> {
    TOKENS.iter().map(|(name, _)| *name).collect()
}

/// Cleans a token map in place: unknown keys and invalid values are dropped,
/// numeric values are clamped. Returns `true` when the map changed.
pub fn sanitize_tokens(tokens: &mut BTreeMap<String, String>) -> bool {
    let cleaned: BTreeMap<String, String> = tokens
        .iter()
        .filter_map(|(key, value)| {
            let (_, kind) = TOKENS.iter().find(|(name, _)| *name == key.as_str())?;
            Some((key.clone(), clean_value(*kind, value)?))
        })
        .collect();
    let changed = cleaned != *tokens;
    *tokens = cleaned;
    changed
}

/// Cleans presets in place: trims names, drops empty, duplicate and over-long
/// names, caps the count and sanitizes each preset's tokens.
/// Returns `true` when the list changed.
pub fn sanitize_presets(presets: &mut Vec<SubtitlePreset>) -> bool {
    let mut cleaned: Vec<SubtitlePreset> = Vec::new();
    for preset in presets.iter() {
        if cleaned.len() == MAX_SUBTITLE_PRESETS {
            break;
        }
        let name = preset.name.trim();
        if name.is_empty()
            || name.chars().count() > MAX_PRESET_NAME_CHARS
            || cleaned.iter().any(|kept| kept.name == name)
        {
            continue;
        }
        let mut tokens = preset.tokens.clone();
        sanitize_tokens(&mut tokens);
        let style = preset
            .style
            .clone()
            .filter(|style| super::VALID_SUBTITLE_STYLES.contains(&style.as_str()));
        cleaned.push(SubtitlePreset {
            name: name.to_string(),
            style,
            tokens,
        });
    }
    let changed = cleaned != *presets;
    *presets = cleaned;
    changed
}

fn clean_value(kind: Kind, raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.len() > MAX_VALUE_LEN {
        return None;
    }
    match kind {
        Kind::Color => is_color(value).then(|| value.to_string()),
        Kind::Px { min, max } => clamp_unit(value, "px", min, max),
        Kind::Seconds { min, max } => clamp_unit(value, "s", min, max),
        Kind::Weight => is_weight(value).then(|| value.to_string()),
        Kind::Shadow => (value == "none" || is_safe(value)).then(|| value.to_string()),
        Kind::Border => clean_border(value),
        Kind::Padding => clean_padding(value),
        Kind::Animation => matches!(value, "pop" | "fade" | "none").then(|| value.to_string()),
        Kind::Transform => matches!(value, "none" | "uppercase" | "lowercase" | "capitalize")
            .then(|| value.to_string()),
        Kind::Spacing if value == "normal" => Some(value.to_string()),
        Kind::Spacing => clamp_unit(value, "px", -5.0, 20.0),
        Kind::Font => clean_font(value),
    }
}

/// Parses an optionally signed plain decimal; rejects exponents, NaN and inf.
fn number(text: &str) -> Option<f64> {
    // Same grammar as the overlay's CSS_NUMBER: optional sign, digits, optional fraction.
    let body = text.strip_prefix('-').unwrap_or(text);
    let digits = |part: &str| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit());
    let well_formed = match body.split_once('.') {
        Some((int, frac)) => digits(int) && digits(frac),
        None => digits(body),
    };
    if !well_formed {
        return None;
    }
    text.parse::<f64>().ok()
}

fn clamp_unit(value: &str, unit: &str, min: f64, max: f64) -> Option<String> {
    let n = number(value.strip_suffix(unit)?)?;
    Some(format!("{}{unit}", n.clamp(min, max)))
}

fn is_color(value: &str) -> bool {
    if let Some(hex) = value.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    let args = value
        .strip_prefix("rgba(")
        .or_else(|| value.strip_prefix("rgb("))
        .and_then(|rest| rest.strip_suffix(')'));
    match args {
        Some(args) => {
            let parts: Vec<&str> = args.split(',').map(str::trim).collect();
            (parts.len() == 3 || parts.len() == 4)
                && parts
                    .iter()
                    .all(|part| number(part.strip_suffix('%').unwrap_or(part)).is_some())
        }
        None => false,
    }
}

fn is_weight(value: &str) -> bool {
    matches!(value, "normal" | "bold")
        || (value.len() == 3
            && value
                .parse::<u16>()
                .is_ok_and(|w| (100..=900).contains(&w) && w % 100 == 0))
}

/// Generic guard for free CSS values: short, a restricted character set, no `url`.
fn is_safe(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_VALUE_LEN
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " #.,%()+-'\"".contains(c))
        && !value.to_ascii_lowercase().contains("url")
}

fn clean_font(value: &str) -> Option<String> {
    let safe_name = !value.is_empty()
        && value.len() <= MAX_VALUE_LEN
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " -,'\"".contains(c));
    safe_name.then(|| value.to_string())
}

fn clean_padding(value: &str) -> Option<String> {
    let parts: Vec<String> = value
        .split_whitespace()
        .map(|part| clamp_unit(part, "px", 0.0, 200.0))
        .collect::<Option<_>>()?;
    (1..=4).contains(&parts.len()).then(|| parts.join(" "))
}

fn clean_border(value: &str) -> Option<String> {
    if value == "none" {
        return Some(value.to_string());
    }
    let (width, rest) = value.split_once(' ')?;
    let (style, color) = rest.trim_start().split_once(' ')?;
    let color = color.trim();
    let valid_style = matches!(style, "solid" | "dashed" | "dotted" | "double");
    (valid_style && is_color(color)).then_some(())?;
    Some(format!(
        "{} {style} {color}",
        clamp_unit(width, "px", 0.0, 20.0)?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn clean_one(key: &str, value: &str) -> Option<String> {
        let mut map = tokens(&[(key, value)]);
        sanitize_tokens(&mut map);
        map.get(key).cloned()
    }

    #[test]
    fn valid_colors_are_kept_unchanged() {
        for color in [
            "#fff",
            "#ffcc00",
            "#ffcc0080",
            "rgba(0, 0, 0, 0.7)",
            "rgb(255, 204, 0)",
        ] {
            assert_eq!(clean_one("--sub-color", color).as_deref(), Some(color));
        }
    }

    #[test]
    fn invalid_colors_are_dropped() {
        for color in [
            "url(http://evil.test/x.png)",
            "red;background:url(x)",
            "#ffgg00",
            "rgb(1, 2)",
            "",
        ] {
            assert_eq!(clean_one("--sub-bg", color), None, "accepted {color}");
        }
    }

    #[test]
    fn unknown_tokens_are_dropped() {
        assert_eq!(clean_one("--evil", "#fff"), None);
        assert_eq!(clean_one("color", "#fff"), None);
    }

    #[test]
    fn sizes_are_clamped_to_their_range() {
        assert_eq!(
            clean_one("--sub-font-size", "500px").as_deref(),
            Some("120px")
        );
        assert_eq!(clean_one("--sub-font-size", "3px").as_deref(), Some("12px"));
        assert_eq!(
            clean_one("--sub-font-size", "48px").as_deref(),
            Some("48px")
        );
        assert_eq!(clean_one("--sub-font-size", "abc"), None);
        assert_eq!(clean_one("--sub-font-size", "48"), None);
        assert_eq!(clean_one("--sub-radius", "99px").as_deref(), Some("50px"));
    }

    #[test]
    fn durations_are_clamped_to_their_range() {
        assert_eq!(
            clean_one("--sub-animation-duration", "9s").as_deref(),
            Some("2s")
        );
        assert_eq!(
            clean_one("--sub-animation-duration", "0.3s").as_deref(),
            Some("0.3s")
        );
        assert_eq!(clean_one("--sub-animation-duration", "fast"), None);
    }

    #[test]
    fn font_family_rejects_urls_and_css_injection() {
        assert_eq!(
            clean_one("--sub-font-family", "'Segoe UI', Arial, sans-serif").as_deref(),
            Some("'Segoe UI', Arial, sans-serif")
        );
        for family in [
            "url(http://evil.test/x.woff)",
            "Arial; } body { display:none",
            "Arial\\",
            "Arial{",
            "",
        ] {
            assert_eq!(
                clean_one("--sub-font-family", family),
                None,
                "accepted {family}"
            );
        }
    }

    #[test]
    fn weight_transform_and_animation_use_fixed_sets() {
        assert_eq!(
            clean_one("--sub-font-weight", "900").as_deref(),
            Some("900")
        );
        assert_eq!(
            clean_one("--sub-font-weight", "bold").as_deref(),
            Some("bold")
        );
        assert_eq!(clean_one("--sub-font-weight", "950"), None);
        assert_eq!(
            clean_one("--sub-text-transform", "uppercase").as_deref(),
            Some("uppercase")
        );
        assert_eq!(clean_one("--sub-text-transform", "spin"), None);
        assert_eq!(
            clean_one("--sub-animation", "fade").as_deref(),
            Some("fade")
        );
        assert_eq!(clean_one("--sub-animation", "spin"), None);
    }

    #[test]
    fn padding_border_and_spacing_are_structural() {
        assert_eq!(
            clean_one("--sub-padding", "10px 20px").as_deref(),
            Some("10px 20px")
        );
        assert_eq!(clean_one("--sub-padding", "10px 20px 3px 4px 5px"), None);
        assert_eq!(clean_one("--sub-padding", "10px; x"), None);
        assert_eq!(clean_one("--sub-border", "none").as_deref(), Some("none"));
        assert_eq!(
            clean_one("--sub-border", "2px solid rgba(0, 0, 0, 0.5)").as_deref(),
            Some("2px solid rgba(0, 0, 0, 0.5)")
        );
        assert_eq!(clean_one("--sub-border", "2px url(x) red"), None);
        assert_eq!(
            clean_one("--sub-letter-spacing", "normal").as_deref(),
            Some("normal")
        );
        assert_eq!(
            clean_one("--sub-letter-spacing", "2px").as_deref(),
            Some("2px")
        );
        assert_eq!(
            clean_one("--sub-letter-spacing", "-9px").as_deref(),
            Some("-5px")
        );
        assert_eq!(
            clean_one("--sub-word-spacing", "normal").as_deref(),
            Some("normal")
        );
        assert_eq!(
            clean_one("--sub-word-spacing", "4px").as_deref(),
            Some("4px")
        );
        assert_eq!(
            clean_one("--sub-word-spacing", "-9px").as_deref(),
            Some("-5px")
        );
    }

    #[test]
    fn shadow_rejects_urls_and_block_characters() {
        assert_eq!(
            clean_one("--sub-shadow", "2px 2px 0 #000").as_deref(),
            Some("2px 2px 0 #000")
        );
        assert_eq!(clean_one("--sub-shadow", "0 0 url(x) #000"), None);
        assert_eq!(clean_one("--sub-shadow", "0 0 0 #000} body{"), None);
    }

    #[test]
    fn sanitize_tokens_reports_changes_and_is_idempotent() {
        let mut map = tokens(&[("--sub-color", "#ff0000"), ("--evil", "1")]);
        assert!(sanitize_tokens(&mut map));
        assert_eq!(map, tokens(&[("--sub-color", "#ff0000")]));
        assert!(!sanitize_tokens(&mut map));
    }

    fn preset(name: &str, pairs: &[(&str, &str)]) -> SubtitlePreset {
        SubtitlePreset {
            name: name.to_string(),
            style: None,
            tokens: tokens(pairs),
        }
    }

    #[test]
    fn preset_style_defaults_to_none_and_keeps_only_built_in_styles() {
        let old: SubtitlePreset = serde_json::from_str(r#"{"name":"a","tokens":{}}"#).unwrap();
        assert_eq!(old.style, None);
        let mut presets = vec![
            SubtitlePreset {
                style: Some("neon".into()),
                ..preset("ok", &[])
            },
            SubtitlePreset {
                style: Some("evil".into()),
                ..preset("bad", &[])
            },
        ];
        assert!(sanitize_presets(&mut presets));
        assert_eq!(presets[0].style.as_deref(), Some("neon"));
        assert_eq!(presets[1].style, None);
        assert!(!sanitize_presets(&mut presets));
    }

    #[test]
    fn lifetime_is_clamped_to_its_range() {
        assert_eq!(clean_one("--sub-lifetime", "5s").as_deref(), Some("5s"));
        assert_eq!(clean_one("--sub-lifetime", "99s").as_deref(), Some("15s"));
        assert_eq!(clean_one("--sub-lifetime", "0.2s").as_deref(), Some("1.5s"));
        assert_eq!(clean_one("--sub-lifetime", "long"), None);
    }

    #[test]
    fn presets_are_trimmed_deduplicated_and_sanitized() {
        let mut presets = vec![
            preset("  Night  ", &[("--sub-bg", "#000"), ("--evil", "x")]),
            preset("Night", &[("--sub-bg", "#111")]),
            preset("   ", &[]),
            preset(&"x".repeat(MAX_PRESET_NAME_CHARS + 1), &[]),
        ];
        assert!(sanitize_presets(&mut presets));
        assert_eq!(presets, vec![preset("Night", &[("--sub-bg", "#000")])]);
        assert!(!sanitize_presets(&mut presets));
    }

    #[test]
    fn presets_are_capped_in_count() {
        let mut presets: Vec<SubtitlePreset> = (0..MAX_SUBTITLE_PRESETS + 5)
            .map(|i| preset(&format!("p{i}"), &[]))
            .collect();
        assert!(sanitize_presets(&mut presets));
        assert_eq!(presets.len(), MAX_SUBTITLE_PRESETS);
        assert_eq!(presets[0].name, "p0");
    }

    #[test]
    fn shared_parity_cases_match_the_overlay_grammar() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/subtitle_theme_cases.json"
        ))
        .unwrap();
        for case in cases["valid"].as_array().unwrap() {
            let (key, value) = (case[0].as_str().unwrap(), case[1].as_str().unwrap());
            assert!(
                clean_one(key, value).is_some(),
                "rejected valid {key}: {value}"
            );
        }
        for case in cases["invalid"].as_array().unwrap() {
            let (key, value) = (case[0].as_str().unwrap(), case[1].as_str().unwrap());
            assert_eq!(
                clean_one(key, value),
                None,
                "accepted invalid {key}: {value}"
            );
        }
    }

    #[test]
    fn values_longer_than_the_overlay_cap_are_rejected() {
        let long_color = format!("rgb({}, 0, 0)", "0".repeat(195));
        assert!(long_color.len() > 200);
        assert_eq!(clean_one("--sub-color", &long_color), None);
    }
}
