// SPDX-License-Identifier: MIT

//! Installed font family names for the Style tab font picker.
//!
//! Names are filtered to the charset the subtitle theme accepts (ASCII letters,
//! digits, space and hyphen), so a listed name always passes `--sub-font-family`
//! validation and cannot break out of a CSS value.

const MAX_FAMILY_LEN: usize = 200;

/// Keeps names the theme validator accepts, trimmed, deduplicated and sorted.
pub fn clean_family_names(raw: Vec<String>) -> Vec<String> {
    let mut names: Vec<String> = raw
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| {
            !name.is_empty()
                && name.len() <= MAX_FAMILY_LEN
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-')
        })
        .collect();
    names.sort_by_key(|name| name.to_ascii_lowercase());
    names.dedup_by_key(|name| name.to_ascii_lowercase());
    names
}

/// Parses `fc-list : family` stdout into theme-safe, sorted, deduplicated family names.
pub fn parse_fc_list_output(output: &str) -> Vec<String> {
    let raw = output
        .lines()
        .flat_map(|line| line.split(','))
        .map(|name| name.to_string())
        .collect();
    clean_family_names(raw)
}

/// Installed font families on this machine. Empty when platform discovery is unavailable or fails.
pub fn installed_font_families() -> Vec<String> {
    #[cfg(windows)]
    {
        clean_family_names(directwrite_families().unwrap_or_default())
    }
    #[cfg(target_os = "linux")]
    {
        fc_list_families().unwrap_or_default()
    }
    #[cfg(all(not(windows), not(target_os = "linux")))]
    {
        Vec::new()
    }
}

#[cfg(target_os = "linux")]
fn fc_list_families() -> Option<Vec<String>> {
    let output = std::process::Command::new("fc-list")
        .args([":", "family"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(parse_fc_list_output(&String::from_utf8_lossy(&output.stdout)))
}

#[cfg(windows)]
fn directwrite_families() -> windows::core::Result<Vec<String>> {
    use windows::core::w;
    use windows::Win32::Graphics::DirectWrite::{
        DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED,
    };

    // SAFETY: plain DirectWrite COM calls on a shared factory; every out value is checked.
    unsafe {
        let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let mut collection = None;
        factory.GetSystemFontCollection(&mut collection, false)?;
        let collection = collection.ok_or_else(windows::core::Error::empty)?;
        let mut families = Vec::new();
        for index in 0..collection.GetFontFamilyCount() {
            let names = collection.GetFontFamily(index)?.GetFamilyNames()?;
            let (mut position, mut exists) = (0u32, windows::core::BOOL(0));
            names.FindLocaleName(w!("en-us"), &mut position, &mut exists)?;
            if !exists.as_bool() {
                position = 0;
            }
            let mut buffer = vec![0u16; names.GetStringLength(position)? as usize + 1];
            names.GetString(position, &mut buffer)?;
            buffer.pop();
            families.push(String::from_utf16_lossy(&buffer));
        }
        Ok(families)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn keeps_only_theme_safe_names_sorted_and_deduplicated() {
        let cleaned = clean_family_names(names(&[
            "Verdana",
            "arial",
            "Arial",
            " Segoe UI ",
            "Bahnschrift",
            "MS Gothic",
            "Yu Gothic-Light",
            "Evil'; } body {",
            "Font (Beta)",
            "Fuente \u{00d1}and\u{00fa}",
            "",
        ]));
        assert_eq!(
            cleaned,
            names(&[
                "arial",
                "Bahnschrift",
                "MS Gothic",
                "Segoe UI",
                "Verdana",
                "Yu Gothic-Light"
            ])
        );
    }

    #[cfg(windows)]
    #[test]
    fn directwrite_lists_the_installed_families() {
        let families = installed_font_families();
        assert!(families.iter().any(|name| name == "Arial"), "{families:?}");
    }

    #[test]
    fn rejects_over_long_names() {
        assert!(clean_family_names(vec!["a".repeat(MAX_FAMILY_LEN + 1)]).is_empty());
    }

    #[test]
    fn parse_fc_list_output_splits_commas_dedups_and_filters() {
        let sample = "DejaVu Sans,DejaVu Sans Condensed\n  Ubuntu  \n\nDejaVu Sans\ndejavu sans\nNoto Sans,  Noto-Mono  \nBad\x07Font\nEvil'; } body {\n";
        assert_eq!(
            parse_fc_list_output(sample),
            names(&[
                "DejaVu Sans",
                "DejaVu Sans Condensed",
                "Noto Sans",
                "Noto-Mono",
                "Ubuntu",
            ])
        );
    }

    #[test]
    fn parse_fc_list_output_handles_empty_input() {
        assert!(parse_fc_list_output("").is_empty());
        assert!(parse_fc_list_output("  \n ,\n").is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_installed_font_families_does_not_panic() {
        let families = installed_font_families();
        assert!(families.iter().all(|name| !name.is_empty()));
    }
}
