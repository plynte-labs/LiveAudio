// SPDX-License-Identifier: MIT

use crate::error::AudioError;
use crate::types::AudioDeviceInfo;
use cpal::traits::{DeviceTrait, HostTrait};
use std::collections::HashSet;
use tracing::debug;

/// Run an audio operation on a dedicated OS thread where Windows COM is not pre-initialized.
///
/// On Windows, if the caller's thread initialized COM in STA mode (`COINIT_APARTMENTTHREADED`,
/// e.g. WebView2/Tauri/OLE), WASAPI calls in CPAL fail with `RPC_E_CHANGED_MODE` (0x80010106).
/// Spawning a fresh OS thread guarantees clean uninitialized COM so CPAL can initialize
/// `COINIT_MULTITHREADED` cleanly.
pub fn with_clean_audio_thread<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    std::thread::Builder::new()
        .name("liveaudio-cpal-enum".into())
        .spawn(f)
        .ok()
        .and_then(|handle| handle.join().ok())
}

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

fn enumerate_cpal_devices_inner(host: &cpal::Host) -> (Vec<AudioDeviceInfo>, bool) {
    let mut result = Vec::new();
    let mut seen_keys = HashSet::new();
    let mut has_error = false;

    let default_input_name = host
        .default_input_device()
        .and_then(|d| d.description().ok().map(|desc| desc.name().to_string()));

    // 1. Enumerate standard input devices (microphones)
    match host.input_devices() {
        Ok(devices) => {
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
        Err(err) => {
            debug!("host.input_devices() error during enumeration: {}", err);
            has_error = true;
        }
    }

    // 2. Enumerate WASAPI Loopback devices (output devices as input capture)
    // On Windows, output devices can be captured via WASAPI Loopback.
    #[cfg(target_os = "windows")]
    {
        let default_output_name = host
            .default_output_device()
            .and_then(|d| d.description().ok().map(|desc| desc.name().to_string()));

        match host.output_devices() {
            Ok(devices) => {
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
            Err(err) => {
                debug!("host.output_devices() error during loopback enumeration: {}", err);
                has_error = true;
            }
        }
    }

    (result, has_error)
}

/// Enumerate all capture-capable audio devices on the system.
///
/// Returns physical microphones and, on Windows WASAPI, system output loopback capture devices.
pub fn enumerate_cpal_devices() -> Result<Vec<AudioDeviceInfo>, AudioError> {
    let host = cpal::default_host();
    let (devices, has_error) = enumerate_cpal_devices_inner(&host);

    if !has_error {
        return Ok(devices);
    }

    debug!("Device enumeration on current thread encountered errors; retrying on clean audio thread.");
    if let Some((clean_devices, _)) = with_clean_audio_thread(|| {
        let clean_host = cpal::default_host();
        enumerate_cpal_devices_inner(&clean_host)
    }) {
        Ok(clean_devices)
    } else {
        Ok(devices)
    }
}

#[cfg(any(target_os = "windows", test))]
fn loopback_is_default(default_output_name: Option<&str>, candidate: &str) -> bool {
    default_output_name == Some(candidate)
}

fn count_input_devices(host: &cpal::Host) -> Option<usize> {
    if host.default_input_device().is_some() {
        return Some(1);
    }
    match host.input_devices() {
        Ok(devices) => Some(devices.count()),
        Err(err) => {
            debug!("Failed to query input devices on host: {}", err);
            None
        }
    }
}

/// Fast query to count available input microphones without initializing full streams.
pub fn get_input_device_count() -> usize {
    let host = cpal::default_host();
    if let Some(count) = count_input_devices(&host) {
        if count > 0 {
            return count;
        }
    }

    debug!("No input devices detected on current thread. Retrying on clean audio thread.");
    with_clean_audio_thread(|| {
        let clean_host = cpal::default_host();
        let count = match count_input_devices(&clean_host) {
            Some(c) if c > 0 => c,
            _ => {
                std::thread::sleep(std::time::Duration::from_millis(40));
                match count_input_devices(&clean_host) {
                    Some(c) => c,
                    None => usize::from(clean_host.default_input_device().is_some()),
                }
            }
        };

        if count > 0 {
            debug!(
                "Detected {} input device(s) on clean audio thread fallback",
                count
            );
        }
        count
    })
    .unwrap_or(0)
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
        if let Ok(devs) = devices {
            println!("Enumerated {} devices", devs.len());
            for d in &devs {
                println!(
                    "  Device: {} (is_default={}, is_loopback={})",
                    d.name, d.is_default, d.is_loopback
                );
            }
        }
    }

    #[test]
    fn test_with_clean_audio_thread_execution() {
        let thread_name = with_clean_audio_thread(|| {
            std::thread::current().name().unwrap_or("").to_string()
        });
        assert_eq!(thread_name, Some("liveaudio-cpal-enum".to_string()));
    }

    #[test]
    fn test_get_input_device_count_does_not_panic() {
        let count = get_input_device_count();
        println!("Input device count: {}", count);
    }

    #[test]
    fn test_count_input_devices_consistency_with_default_device() {
        let host = cpal::default_host();
        let result = count_input_devices(&host);
        if host.default_input_device().is_some() {
            assert!(result.is_some());
            assert!(result.unwrap_or(0) >= 1);
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_sta_thread_device_enumeration_and_count() {
        #[link(name = "ole32")]
        extern "system" {
            fn CoInitializeEx(pv_reserved: *mut std::ffi::c_void, dw_co_init: u32) -> i32;
            fn CoUninitialize();
        }

        const COINIT_APARTMENTTHREADED: u32 = 0x2;

        let handle = std::thread::spawn(|| {
            unsafe {
                let hr = CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED);
                assert!(hr >= 0, "CoInitializeEx in STA mode failed: hr={:#x}", hr);
            }

            // Both functions should survive and succeed on an STA thread via clean-thread fallback
            let count = get_input_device_count();
            println!("STA thread input device count: {}", count);

            let devices = enumerate_cpal_devices();
            assert!(devices.is_ok(), "enumerate_cpal_devices must succeed on STA thread");

            unsafe {
                CoUninitialize();
            }
        });
        let _ = handle.join();
    }
}
