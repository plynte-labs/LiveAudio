// SPDX-License-Identifier: MIT

use crate::error::AudioError;
use crate::types::AudioDeviceInfo;
use cpal::traits::{DeviceTrait, HostTrait};
use std::collections::HashSet;

/// Clean and normalize device name for deduplication.
///
/// Lowercases and drops punctuation and extra whitespace so label noise for the
/// same device merges. Parenthesized text is kept: on Windows it usually carries
/// the distinguishing hardware name (e.g. "Microphone (USB Audio Device)").
pub fn normalize_device_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|ch| ch.is_alphanumeric() || ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Enumerate all capture-capable audio devices on the system.
///
/// Returns physical microphones and, on Windows WASAPI, system output loopback capture devices.
pub fn enumerate_cpal_devices() -> Result<Vec<AudioDeviceInfo>, AudioError> {
    let host = cpal::default_host();
    let mut result = Vec::new();
    let mut seen_keys = HashSet::new();

    let default_input_name = host
        .default_input_device()
        .and_then(|d| d.description().ok().map(|desc| desc.name().to_string()));
    // 1. Enumerate standard input devices (microphones)
    if let Ok(devices) = host.input_devices() {
        for device in devices {
            let name = match device.description() {
                Ok(desc) => desc.name().to_string(),
                Err(_) => continue,
            };

            let norm_name = normalize_device_name(&name);
            let dedup_key = format!("input:{}", norm_name);
            if seen_keys.contains(&dedup_key) {
                continue;
            }
            seen_keys.insert(dedup_key);

            let is_default = default_input_name.as_deref() == Some(&name);
            let mut channels = 1u16;
            let mut sample_rates = Vec::new();

            if let Ok(configs) = device.supported_input_configs() {
                for cfg in configs {
                    channels = channels.max(cfg.channels());
                    sample_rates.push(cfg.min_sample_rate());
                    sample_rates.push(cfg.max_sample_rate());
                }
            }
            sample_rates.sort_unstable();
            sample_rates.dedup();
            if sample_rates.is_empty() {
                sample_rates = vec![16000, 44100, 48000];
            }

            result.push(AudioDeviceInfo::new(
                name.clone(),
                format!("🎤 {}", name),
                is_default,
                channels,
                sample_rates,
                false,
            ));
        }
    }

    // 2. Enumerate WASAPI Loopback devices (output devices as input capture)
    // On Windows, output devices can be captured via WASAPI Loopback.
    #[cfg(target_os = "windows")]
    {
        let default_output_name = host
            .default_output_device()
            .and_then(|d| d.description().ok().map(|desc| desc.name().to_string()));

        if let Ok(devices) = host.output_devices() {
            for device in devices {
                let name = match device.description() {
                    Ok(desc) => desc.name().to_string(),
                    Err(_) => continue,
                };

                let norm_name = normalize_device_name(&name);
                let dedup_key = format!("loopback:{}", norm_name);
                if seen_keys.contains(&dedup_key) {
                    continue;
                }
                seen_keys.insert(dedup_key);

                let is_default = loopback_is_default(default_output_name.as_deref(), &name);
                let mut channels = 2u16;
                let mut sample_rates = Vec::new();

                if let Ok(configs) = device.supported_output_configs() {
                    for cfg in configs {
                        channels = channels.max(cfg.channels());
                        sample_rates.push(cfg.min_sample_rate());
                        sample_rates.push(cfg.max_sample_rate());
                    }
                }
                sample_rates.sort_unstable();
                sample_rates.dedup();
                if sample_rates.is_empty() {
                    sample_rates = vec![44100, 48000];
                }

                result.push(AudioDeviceInfo::new(
                    format!("loopback:{}", name),
                    format!("🔊 {} (Loopback)", name),
                    is_default,
                    channels,
                    sample_rates,
                    true,
                ));
            }
        }
    }

    Ok(result)
}

#[cfg(any(target_os = "windows", test))]
fn loopback_is_default(default_output_name: Option<&str>, candidate: &str) -> bool {
    default_output_name == Some(candidate)
}

/// Fast query to count available input microphones without initializing full streams.
pub fn get_input_device_count() -> usize {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices.count(),
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_device_name_keeps_distinguishing_parenthesized_text() {
        assert_eq!(
            normalize_device_name("Microphone (Realtek(R) Audio)"),
            "microphone realtekr audio"
        );
        assert_ne!(
            normalize_device_name("Microphone (Realtek High Definition Audio)"),
            normalize_device_name("Microphone (Realtek Audio)")
        );
    }

    #[test]
    fn loopback_default_matching_requires_the_os_default_name() {
        assert!(loopback_is_default(Some("Speakers"), "Speakers"));
        assert!(!loopback_is_default(Some("Speakers"), "Headphones"));
        assert!(!loopback_is_default(None, "Speakers"));
    }

    #[test]
    fn test_device_enumeration_does_not_panic() {
        let devices = enumerate_cpal_devices();
        assert!(devices.is_ok());
        let devs = devices.unwrap();
        println!("Enumerated {} devices", devs.len());
        for d in &devs {
            println!(
                "  Device: {} (is_default={}, is_loopback={})",
                d.name, d.is_default, d.is_loopback
            );
        }
    }
}
