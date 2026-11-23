// SPDX-License-Identifier: MIT
//! Reads the language OpenCohost's co-host speaks so LiveAudio can follow it.
//! Read-only: LiveAudio never writes OpenCohost's files.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::config::LiveAudioConfig;

/// Pure resolver: `<env_root>/config/locale.json`, else `<local_data>/OpenCohostData/config/locale.json`.
/// Returns `Some("es" | "en")` only for a readable file whose primary subtag is one of those.
pub fn resolve_language(env_root: Option<&Path>, local_data: Option<&Path>) -> Option<String> {
    let root = match (env_root, local_data) {
        (Some(root), _) => root.to_path_buf(),
        (None, Some(local)) => local.join("OpenCohostData"),
        (None, None) => return None,
    };
    let path = root.join("config").join("locale.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => {
            tracing::debug!("OpenCohost locale not available at {}: {err}", path.display());
            return None;
        }
    };
    let locale = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v.get("locale")?.as_str().map(str::to_owned));
    let primary = locale
        .as_deref()
        .and_then(|l| l.split(['-', '_']).next())
        .map(str::to_ascii_lowercase);
    match primary.as_deref() {
        Some(lang @ ("es" | "en")) => Some(lang.to_string()),
        _ => {
            tracing::warn!("Ignoring unsupported or malformed OpenCohost locale in {}", path.display());
            None
        }
    }
}

/// Resolves from the real environment (`OPENCOHOST_DATA_ROOT`, then the platform local data dir).
pub fn resolve_language_from_env() -> Option<String> {
    let env_root = std::env::var_os("OPENCOHOST_DATA_ROOT")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    let local_data = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
        });
    resolve_language(env_root.as_deref(), local_data.as_deref())
}

/// In-memory language override: `Some` only when the toggle is on and OpenCohost's language resolves.
pub fn language_override(config: &LiveAudioConfig) -> Option<String> {
    if !config.follow_opencohost_language {
        return None;
    }
    let lang = resolve_language_from_env()?;
    static LAST_LOGGED: Mutex<Option<String>> = Mutex::new(None);
    let mut last = LAST_LOGGED.lock().unwrap_or_else(|p| p.into_inner());
    if last.as_deref() != Some(lang.as_str()) {
        tracing::info!("Following OpenCohost language: {lang}");
        *last = Some(lang.clone());
    }
    Some(lang)
}

/// Effective speech language: the OpenCohost override when active, else `asr_language`, then `language`, then `es`.
pub fn effective_asr_language(config: &LiveAudioConfig) -> String {
    language_override(config).unwrap_or_else(|| {
        if !config.asr_language.is_empty() {
            config.asr_language.clone()
        } else {
            config
                .language
                .as_deref()
                .filter(|language| !language.is_empty() && *language != "auto")
                .unwrap_or("es")
                .to_string()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("liveaudio-oc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_locale(root: &Path, body: &str) {
        let dir = root.join("config");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("locale.json"), body).unwrap();
    }

    #[test]
    fn env_root_wins_over_local_data() {
        let env = tmp("env");
        let local = tmp("local");
        write_locale(&env, r#"{"locale":"en"}"#);
        write_locale(&local.join("OpenCohostData"), r#"{"locale":"es"}"#);
        assert_eq!(resolve_language(Some(&env), Some(&local)).as_deref(), Some("en"));
    }

    #[test]
    fn falls_back_to_local_data_dir() {
        let local = tmp("fallback");
        write_locale(&local.join("OpenCohostData"), r#"{"locale":"es","set_at":"x"}"#);
        assert_eq!(resolve_language(None, Some(&local)).as_deref(), Some("es"));
    }

    #[test]
    fn takes_the_primary_subtag() {
        let env = tmp("subtag");
        write_locale(&env, r#"{"locale":"en-US"}"#);
        assert_eq!(resolve_language(Some(&env), None).as_deref(), Some("en"));
        write_locale(&env, r#"{"locale":"ES_mx"}"#);
        assert_eq!(resolve_language(Some(&env), None).as_deref(), Some("es"));
    }

    #[test]
    fn unsupported_malformed_or_missing_is_none() {
        let env = tmp("bad");
        assert_eq!(resolve_language(Some(&env), None), None);
        write_locale(&env, r#"{"locale":"fr"}"#);
        assert_eq!(resolve_language(Some(&env), None), None);
        write_locale(&env, "{not json");
        assert_eq!(resolve_language(Some(&env), None), None);
        write_locale(&env, r#"{"locale":42}"#);
        assert_eq!(resolve_language(Some(&env), None), None);
        assert_eq!(resolve_language(None, None), None);
    }

    #[test]
    fn effective_language_follows_opencohost_only_when_toggled_on() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let env = tmp("effective");
        write_locale(&env, r#"{"locale":"en"}"#);
        std::env::set_var("OPENCOHOST_DATA_ROOT", &env);

        let mut cfg = LiveAudioConfig::default(); // asr_language defaults to "es"
        assert_eq!(effective_asr_language(&cfg), "es");
        assert_eq!(language_override(&cfg), None);

        cfg.follow_opencohost_language = true;
        assert_eq!(effective_asr_language(&cfg), "en");

        std::fs::remove_file(env.join("config").join("locale.json")).unwrap();
        assert_eq!(effective_asr_language(&cfg), "es");
        std::env::remove_var("OPENCOHOST_DATA_ROOT");
    }
}
