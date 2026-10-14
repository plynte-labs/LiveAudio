// SPDX-License-Identifier: MIT

//! Rust-side helpers for the Python ASR worker protocol
//! (`liveaudio/service/asr_worker_protocol.py`) and the transcript text hygiene
//! ported from `liveaudio/core/engine.py`.

use serde_json::{json, Value};

use crate::config::LiveAudioConfig;

/// Subtitle presentation cap, in characters (`engine.py` `MAX_SUBTITLE_CHARS`).
pub const MAX_SUBTITLE_CHARS: usize = 600;
/// Segments more likely silence than speech are dropped (`engine.py`).
const MAX_NO_SPEECH_PROB: f64 = 0.6;
/// Segments this short or shorter are dropped as noise (`engine.py`).
const MIN_SEGMENT_CHARS: usize = 2;

/// Builds the worker `init` payload. The worker rejects unknown keys
/// (`InitPayload(**raw)`), so only fields that dataclass declares may be sent.
pub fn worker_init_payload(
    config: &LiveAudioConfig,
    model_name: &str,
    language: &str,
    initial_prompt: Option<&str>,
) -> Value {
    json!({
        "model_name": model_name,
        "device": config.device,
        "language": language,
        "initial_prompt": initial_prompt,
        "cpu_threads": config.cpu_threads,
        "auto_cpu_fallback": true,
    })
}

/// Unicode format (Cf) characters, which Python's `str.isprintable` rejects:
/// bidi overrides/isolates/marks, zero-width characters and invisible operators.
fn is_format_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{00AD}'
            | '\u{0600}'..='\u{0605}'
            | '\u{061C}'
            | '\u{06DD}'
            | '\u{070F}'
            | '\u{180E}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{FEFF}'
            | '\u{FFF9}'..='\u{FFFB}'
    )
}

/// Removes control and bidi/format characters and collapses whitespace
/// (`engine.py` `_sanitize_text`).
pub fn sanitize_transcript_text(text: &str) -> String {
    let printable: String = text
        .chars()
        .filter(|ch| ch.is_whitespace() || !(ch.is_control() || is_format_char(*ch)))
        .collect();
    printable.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Bounded projection used for live subtitles (`engine.py` `_presentation_text`).
pub fn presentation_text(text: &str) -> String {
    match text.char_indices().nth(MAX_SUBTITLE_CHARS) {
        Some((cut, _)) => format!("{}...", text[..cut].trim_end()),
        None => text.to_string(),
    }
}

/// Transcript text of a worker `transcription_result` payload: per-segment
/// sanitization and the `no_speech_prob`/length filters of `engine.py`. Falls back
/// to the payload `text` when the worker reports no segments.
pub fn transcript_text_from_result(payload: &Value) -> String {
    let segments = payload.get("segments").and_then(Value::as_array);
    match segments {
        Some(segments) if !segments.is_empty() => {
            let kept: Vec<String> = segments
                .iter()
                .filter_map(|segment| {
                    let text = sanitize_transcript_text(segment.get("text")?.as_str()?);
                    let no_speech = segment
                        .get("no_speech_prob")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0);
                    (no_speech <= MAX_NO_SPEECH_PROB && text.chars().count() > MIN_SEGMENT_CHARS)
                        .then_some(text)
                })
                .collect();
            sanitize_transcript_text(&kept.join(" "))
        }
        _ => sanitize_transcript_text(payload.get("text").and_then(Value::as_str).unwrap_or("")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fields declared by `InitPayload` in `asr_worker_protocol.py`.
    const INIT_PAYLOAD_FIELDS: &[&str] = &[
        "model_name",
        "device",
        "device_index",
        "compute_type",
        "cpu_threads",
        "cache_dir",
        "local_files_only",
        "language",
        "initial_prompt",
        "beam_size",
        "temperature",
        "vad_filter",
        "condition_on_previous_text",
        "audio_framing",
        "auto_cpu_fallback",
    ];

    #[test]
    fn init_payload_forwards_cpu_threads_and_only_declared_fields() {
        let mut config = LiveAudioConfig::default();
        config.cpu_threads = 3;
        let payload = worker_init_payload(&config, "small", "es", None);
        let object = payload.as_object().expect("init payload is an object");
        for key in object.keys() {
            assert!(
                INIT_PAYLOAD_FIELDS.contains(&key.as_str()),
                "worker would reject unknown init key {key}"
            );
        }
        assert_eq!(payload["cpu_threads"], json!(3));
    }

    #[test]
    fn sanitize_strips_bidi_and_control_characters() {
        assert_eq!(
            sanitize_transcript_text("palabra\u{202E}otra"),
            "palabraotra"
        );
        assert_eq!(
            sanitize_transcript_text("\u{202E}\u{202D}\u{200E}\u{200F}hello"),
            "hello"
        );
        assert_eq!(
            sanitize_transcript_text("a\u{0000}b\u{001b}c\u{2066}d"),
            "abcd"
        );
        assert_eq!(
            sanitize_transcript_text("  hola \n\t mundo  "),
            "hola mundo"
        );
        assert_eq!(
            sanitize_transcript_text("hola mundo ñ áéíóú 🎤"),
            "hola mundo ñ áéíóú 🎤"
        );
    }

    #[test]
    fn presentation_caps_long_text_at_600_characters() {
        let short = "ñ".repeat(MAX_SUBTITLE_CHARS);
        assert_eq!(presentation_text(&short), short);
        let long = format!("{} tail", "ñ".repeat(MAX_SUBTITLE_CHARS - 1));
        let capped = presentation_text(&long);
        assert_eq!(capped, format!("{}...", "ñ".repeat(MAX_SUBTITLE_CHARS - 1)));
    }

    #[test]
    fn result_text_drops_silent_and_tiny_segments() {
        let payload = json!({
            "text": "hola mundo ruido ok y adiós",
            "segments": [
                {"text": " hola mundo ", "no_speech_prob": 0.1},
                {"text": "ruido", "no_speech_prob": 0.9},
                {"text": "ok", "no_speech_prob": 0.0},
                {"text": "y adiós\u{202E}", "no_speech_prob": 0.6},
            ]
        });
        assert_eq!(transcript_text_from_result(&payload), "hola mundo y adiós");
    }

    #[test]
    fn result_text_without_segments_falls_back_to_sanitized_text() {
        let payload = json!({"text": " hola\u{202E}  mundo ", "segments": []});
        assert_eq!(transcript_text_from_result(&payload), "hola mundo");
        assert_eq!(transcript_text_from_result(&json!({})), "");
    }
}
