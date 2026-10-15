// SPDX-License-Identifier: MIT

//! Privacy scrubbing for diagnostics exports. Mirrors Python `liveaudio/core/diagnostics.py`:
//! sensitive key tokens are replaced wholesale, Windows absolute paths and URLs are redacted in
//! strings. POSIX home directories are redacted too, since exports are shared for support.

use serde_json::Value;

const REDACTED: &str = "<redacted>";
const SENSITIVE_KEY_TOKENS: [&str; 5] = ["secret", "token", "password", "transcript", "audio"];

/// Redacts sensitive keys and path/URL strings anywhere in an export value, in place.
pub(crate) fn scrub_diagnostics_value(value: &mut Value) {
    match value {
        Value::String(text) => *text = redact_string(text),
        Value::Array(items) => items.iter_mut().for_each(scrub_diagnostics_value),
        Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                if is_sensitive_key(key) {
                    *item = Value::String(REDACTED.to_owned());
                } else {
                    scrub_diagnostics_value(item);
                }
            }
        }
        _ => {}
    }
}

/// Path-like keys are kept, matching the Python rule.
fn is_sensitive_key(key: &str) -> bool {
    if key.contains('/') || key.contains('\\') {
        return false;
    }
    let lower = key.to_ascii_lowercase();
    SENSITIVE_KEY_TOKENS
        .iter()
        .any(|token| lower.contains(token))
}

fn redact_string(input: &str) -> String {
    let without_windows_paths = redact_runs(input, starts_windows_path);
    let without_urls = redact_runs(&without_windows_paths, starts_url);
    redact_runs(&without_urls, starts_posix_home)
}

/// Replaces each run that starts where `is_start` matches, up to the next whitespace.
fn redact_runs(input: &str, is_start: fn(&str) -> bool) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(position) = rest
        .char_indices()
        .map(|(index, _)| index)
        .find(|&index| is_start(&rest[index..]))
    {
        output.push_str(&rest[..position]);
        output.push_str(REDACTED);
        let tail = &rest[position..];
        let run_end = tail.find(char::is_whitespace).unwrap_or(tail.len());
        rest = &tail[run_end..];
    }
    output.push_str(rest);
    output
}

/// `[A-Za-z]:\` followed by a non-separator, as in the Python pattern.
fn starts_windows_path(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() > 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2] == b'\\'
        && !matches!(bytes[3], b'\\' | b' ' | b'\t' | b'\r' | b'\n')
}

fn starts_url(text: &str) -> bool {
    text.starts_with("http://") || text.starts_with("https://")
}

fn starts_posix_home(text: &str) -> bool {
    ["/home/", "/Users/", "/root/"]
        .iter()
        .any(|prefix| text.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::scrub_diagnostics_value;
    use serde_json::json;

    #[test]
    fn export_scrubs_paths_urls_and_sensitive_keys_but_keeps_plain_settings() {
        let mut report = json!({
            "config": {
                "diagnostics_export_dir": r"C:\Users\José\AppData\Local\LiveAudio\diagnostics",
                "obs_url": "https://obs.example.test/feed?token=abc",
                "model_cache": "/home/alice/.cache/huggingface",
                "ws_port": 8765,
                "api_token": "secret-value",
                "note": "see C:\\Users\\José\\notes.txt for details",
                "transcription_purpose": "transcription",
            },
            "service_status": {"log_path": "/Users/bob/log.txt", "items": ["/root/x"]},
        });

        scrub_diagnostics_value(&mut report);
        let text = report.to_string();

        for leaked in [
            "José",
            "alice",
            "bob",
            "example.test",
            "secret-value",
            "/root/x",
        ] {
            assert!(!text.contains(leaked), "leaked {leaked}: {text}");
        }
        assert!(text.contains(r#""ws_port":8765"#), "{text}");
        assert!(text.contains("see <redacted> for details"), "{text}");
        assert_eq!(report["config"]["api_token"], "<redacted>");
        assert_eq!(report["config"]["transcription_purpose"], "<redacted>");
    }
}
