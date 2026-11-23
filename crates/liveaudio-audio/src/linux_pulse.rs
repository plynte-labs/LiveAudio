// SPDX-License-Identifier: MIT

use std::collections::HashSet;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::error::AudioError;
use crate::ring_buffer::AudioRingProducer;
use crate::types::AudioDeviceInfo;

#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;
#[cfg(target_os = "linux")]
use std::process::{Child, Command, Stdio};
#[cfg(target_os = "linux")]
use std::thread::JoinHandle;
#[cfg(target_os = "linux")]
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PactlInfo {
    pub default_source: Option<String>,
    pub default_sink: Option<String>,
}

fn clean_pactl_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("n/a") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn parse_pactl_info(stdout: &str) -> PactlInfo {
    let mut info = PactlInfo::default();
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix("Default Source:") {
            info.default_source = clean_pactl_value(val);
        } else if let Some(val) = trimmed.strip_prefix("Default Sink:") {
            info.default_sink = clean_pactl_value(val);
        }
    }
    info
}

fn parse_sample_spec(spec: &str) -> (Option<u16>, Option<u32>) {
    let mut channels = None;
    let mut rate = None;
    for token in spec.split_whitespace() {
        if let Some(ch) = token.strip_suffix("ch") {
            if let Ok(parsed) = ch.parse::<u16>() {
                if parsed > 0 {
                    channels = Some(parsed);
                }
            }
        } else if let Some(hz) = token.strip_suffix("Hz") {
            if let Ok(parsed) = hz.parse::<u32>() {
                if parsed > 0 {
                    rate = Some(parsed);
                }
            }
        }
    }
    (channels, rate)
}

#[derive(Default)]
struct RawSourceBlock {
    name: Option<String>,
    description: Option<String>,
    monitor_of_sink: Option<String>,
    channels: Option<u16>,
    sample_rate: Option<u32>,
}

impl RawSourceBlock {
    fn into_device_info(self, info: &PactlInfo) -> Option<AudioDeviceInfo> {
        let name = self.name.filter(|n| !n.is_empty())?;
        let is_loopback = self
            .monitor_of_sink
            .as_deref()
            .and_then(clean_pactl_value)
            .is_some()
            || name.ends_with(".monitor");

        let display_base = self
            .description
            .as_deref()
            .and_then(clean_pactl_value)
            .unwrap_or_else(|| name.clone());

        let (id, display_name, is_default) = if is_loopback {
            let is_default = info
                .default_sink
                .as_deref()
                .is_some_and(|sink| name == format!("{sink}.monitor"));
            (
                format!("loopback:{name}"),
                format!("🔊 {display_base} (Loopback)"),
                is_default,
            )
        } else {
            let is_default = info.default_source.as_deref() == Some(name.as_str());
            (
                format!("pulse:{name}"),
                format!("🎤 {display_base}"),
                is_default,
            )
        };

        let channels = self.channels.unwrap_or(if is_loopback { 2 } else { 1 });
        let sample_rates = match self.sample_rate {
            Some(rate) => vec![rate],
            None if is_loopback => vec![44100, 48000],
            None => vec![16000, 44100, 48000],
        };

        Some(AudioDeviceInfo::new(
            id,
            display_name,
            is_default,
            channels,
            sample_rates,
            is_loopback,
        ))
    }
}

pub(crate) fn parse_pactl_list_sources(stdout: &str, info: &PactlInfo) -> Vec<AudioDeviceInfo> {
    let mut mics = Vec::new();
    let mut loopbacks = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut current: Option<RawSourceBlock> = None;

    let mut flush_block = |block: Option<RawSourceBlock>| {
        if let Some(raw) = block {
            if let Some(dev) = raw.into_device_info(info) {
                if seen_ids.insert(dev.id.clone()) {
                    if dev.is_loopback {
                        loopbacks.push(dev);
                    } else {
                        mics.push(dev);
                    }
                }
            }
        }
    };

    for line in stdout.lines() {
        if line.starts_with("Source #") {
            flush_block(current.take());
            current = Some(RawSourceBlock::default());
            continue;
        }

        // Only parse top-level source attributes (single tab indent), ignoring nested Properties/Ports.
        if !line.starts_with('\t') || line.starts_with("\t\t") {
            continue;
        }
        let Some(block) = current.as_mut() else {
            continue;
        };

        let field = &line[1..];
        if let Some(val) = field.strip_prefix("Name:") {
            block.name = clean_pactl_value(val);
        } else if let Some(val) = field.strip_prefix("Description:") {
            block.description = clean_pactl_value(val);
        } else if let Some(val) = field.strip_prefix("Monitor of Sink:") {
            block.monitor_of_sink = clean_pactl_value(val);
        } else if let Some(val) = field.strip_prefix("Sample Specification:") {
            let (ch, rate) = parse_sample_spec(val);
            block.channels = ch;
            block.sample_rate = rate;
        }
    }
    flush_block(current.take());

    mics.extend(loopbacks);
    mics
}

const NOISY_ALSA_EXACT: &[&str] = &[
    "null",
    "lavrate",
    "samplerate",
    "speexrate",
    "jack",
    "oss",
    "upmix",
    "vdownmix",
];

const NOISY_ALSA_PREFIXES: &[&str] = &[
    "dsnoop:",
    "dmix:",
    "surround",
    "iec958",
    "hdmi",
    "front:",
    "rear:",
    "center_lfe:",
    "side:",
    "sysdefault:",
    "hw:",
    "plughw:",
];

pub(crate) fn is_noisy_alsa_device(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    if matches!(lower.as_str(), "default" | "pipewire" | "pulse") {
        return false;
    }
    if NOISY_ALSA_EXACT.contains(&lower.as_str()) {
        return true;
    }
    NOISY_ALSA_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

pub(crate) fn filter_linux_alsa_devices(devices: Vec<AudioDeviceInfo>) -> Vec<AudioDeviceInfo> {
    let filtered: Vec<AudioDeviceInfo> = devices
        .iter()
        .filter(|dev| !is_noisy_alsa_device(&dev.id))
        .cloned()
        .collect();
    if filtered.is_empty() {
        devices
    } else {
        filtered
    }
}

pub(crate) fn validate_pulse_source_name(name: &str) -> Result<&str, AudioError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AudioError::InvalidParameter(
            "PulseAudio/PipeWire source name must not be empty".to_string(),
        ));
    }
    if trimmed.starts_with('-') || trimmed.contains(['\0', '\n', '\r']) {
        return Err(AudioError::InvalidParameter(format!(
            "Invalid PulseAudio/PipeWire source name: {trimmed}"
        )));
    }
    Ok(trimmed)
}

fn raw_pulse_name(device: &AudioDeviceInfo) -> Option<&str> {
    device
        .id
        .strip_prefix("loopback:")
        .or_else(|| device.id.strip_prefix("pulse:"))
}

pub(crate) fn resolve_pulse_device_id(
    device_id: Option<&str>,
    info: Option<&PactlInfo>,
    known_sources: &[AudioDeviceInfo],
) -> Result<Option<(String, bool)>, AudioError> {
    let Some(id) = device_id else {
        return Ok(None);
    };

    if id == "default_loopback" || id == "wasapi_loopback" {
        let default_sink = info
            .and_then(|i| i.default_sink.as_deref())
            .ok_or_else(|| {
                AudioError::DeviceNotFound(
                    "PulseAudio/PipeWire default output sink unavailable for system audio loopback"
                        .to_string(),
                )
            })?;
        let monitor = format!("{default_sink}.monitor");
        validate_pulse_source_name(&monitor)?;
        return Ok(Some((monitor, true)));
    }

    if let Some(raw) = id.strip_prefix("loopback:") {
        let source = validate_pulse_source_name(raw)?;
        if !known_sources.is_empty()
            && !known_sources
                .iter()
                .any(|dev| raw_pulse_name(dev) == Some(source))
        {
            return Err(AudioError::DeviceNotFound(id.to_string()));
        }
        return Ok(Some((source.to_string(), true)));
    }

    if let Some(raw) = id.strip_prefix("pulse:") {
        let source = validate_pulse_source_name(raw)?;
        if !known_sources.is_empty()
            && !known_sources
                .iter()
                .any(|dev| raw_pulse_name(dev) == Some(source))
        {
            return Err(AudioError::DeviceNotFound(id.to_string()));
        }
        return Ok(Some((source.to_string(), false)));
    }

    // Raw Pulse/PipeWire source name (without prefix), if it matches an active source.
    if let Some(matched) = known_sources
        .iter()
        .find(|dev| raw_pulse_name(dev) == Some(id))
    {
        let source = validate_pulse_source_name(id)?;
        return Ok(Some((source.to_string(), matched.is_loopback)));
    }

    Ok(None)
}

pub(crate) fn decode_f32le_stream(input: &[u8], carry: &mut Vec<u8>, out: &mut Vec<f32>) {
    out.clear();
    if input.is_empty() {
        return;
    }

    let mut slice = input;
    if !carry.is_empty() {
        let needed = 4usize.saturating_sub(carry.len());
        let take = needed.min(slice.len());
        carry.extend_from_slice(&slice[..take]);
        slice = &slice[take..];
        if carry.len() == 4 {
            let bytes: [u8; 4] = [carry[0], carry[1], carry[2], carry[3]];
            out.push(f32::from_le_bytes(bytes));
            carry.clear();
        }
    }

    let full_chunks = slice.chunks_exact(4);
    let remainder = full_chunks.remainder();
    out.reserve(full_chunks.len());
    for chunk in full_chunks {
        let bytes: [u8; 4] = [chunk[0], chunk[1], chunk[2], chunk[3]];
        out.push(f32::from_le_bytes(bytes));
    }
    if !remainder.is_empty() {
        carry.extend_from_slice(remainder);
    }
}

pub(crate) fn read_f32le_loop<R: Read>(
    mut reader: R,
    mut producer: AudioRingProducer,
    stop: Arc<AtomicBool>,
    capture_failed: Arc<AtomicBool>,
) {
    let mut byte_buf = [0u8; 4096];
    let mut carry = Vec::with_capacity(4);
    let mut samples = Vec::with_capacity(1024);

    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }
        match reader.read(&mut byte_buf) {
            Ok(0) => {
                if !stop.load(Ordering::Acquire) {
                    capture_failed.store(true, Ordering::SeqCst);
                }
                break;
            }
            Ok(n) => {
                decode_f32le_stream(&byte_buf[..n], &mut carry, &mut samples);
                if !samples.is_empty() {
                    producer.push_samples(&samples);
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => {
                if !stop.load(Ordering::Acquire) {
                    capture_failed.store(true, Ordering::SeqCst);
                }
                break;
            }
        }
    }
}

#[cfg(target_os = "linux")]
const PACTL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

#[cfg(target_os = "linux")]
fn run_command_with_timeout(cmd: &mut Command, timeout: std::time::Duration) -> Option<String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });

    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                let now = std::time::Instant::now();
                if now >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return None;
                }
                std::thread::sleep((deadline - now).min(std::time::Duration::from_millis(20)));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return None;
            }
        }
    };

    let stdout_bytes = reader.join().ok()?;
    if !status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&stdout_bytes).into_owned())
}

#[cfg(target_os = "linux")]
fn run_pactl(args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("pactl");
    cmd.env("LC_ALL", "C").args(args);
    run_command_with_timeout(&mut cmd, PACTL_TIMEOUT)
}

#[cfg(target_os = "linux")]
pub(crate) fn query_pactl_info() -> Option<PactlInfo> {
    let stdout = run_pactl(&["info"])?;
    Some(parse_pactl_info(&stdout))
}

#[cfg(target_os = "linux")]
pub(crate) fn enumerate_pulse_devices() -> Option<Vec<AudioDeviceInfo>> {
    let info = query_pactl_info()?;
    let sources_stdout = run_pactl(&["list", "sources"])?;
    let devices = parse_pactl_list_sources(&sources_stdout, &info);
    if devices.is_empty() {
        None
    } else {
        Some(devices)
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn resolve_linux_pulse_source(
    device_id: Option<&str>,
) -> Result<Option<(String, bool)>, AudioError> {
    let Some(id) = device_id else {
        return Ok(None);
    };
    let info = query_pactl_info();
    let sources = info
        .as_ref()
        .and_then(|i| {
            run_pactl(&["list", "sources"]).map(|out| parse_pactl_list_sources(&out, i))
        })
        .unwrap_or_default();
    resolve_pulse_device_id(Some(id), info.as_ref(), &sources)
}

/// Make the child receive SIGKILL when this process dies (`PR_SET_PDEATHSIG`).
#[cfg(target_os = "linux")]
pub fn configure_linux_parent_death_signal(cmd: &mut Command) {
    let parent_pid = unsafe { libc::getpid() };
    unsafe {
        cmd.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != parent_pid {
                libc::_exit(1);
            }
            Ok(())
        });
    }
}

#[cfg(target_os = "linux")]
pub(crate) struct PulseCaptureHandle {
    child: Option<Child>,
    reader_thread: Option<JoinHandle<()>>,
    stop_signal: Arc<AtomicBool>,
}

#[cfg(target_os = "linux")]
impl PulseCaptureHandle {
    pub(crate) fn stop(&mut self) {
        self.stop_signal.store(true, Ordering::Release);
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(handle) = self.reader_thread.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for PulseCaptureHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn start_pulse_capture(
    source_name: &str,
    sample_rate: u32,
    producer: AudioRingProducer,
    stop_signal: Arc<AtomicBool>,
    capture_failed: Arc<AtomicBool>,
) -> Result<PulseCaptureHandle, AudioError> {
    let source = validate_pulse_source_name(source_name)?;

    let mut cmd = Command::new("parec");
    cmd.arg("--raw")
        .arg("--format=float32le")
        .arg(format!("--rate={sample_rate}"))
        .arg("--channels=1")
        .arg("--latency-msec=40")
        .arg(format!("--device={source}"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    configure_linux_parent_death_signal(&mut cmd);

    info!(
        "Starting Linux PulseAudio/PipeWire capture via parec: source={}, rate={}Hz",
        source, sample_rate
    );

    let mut child = cmd.spawn().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            AudioError::StreamError(
                "parec (pulseaudio-utils) not found; cannot capture PulseAudio/PipeWire source"
                    .to_string(),
            )
        } else {
            AudioError::StreamError(format!("Failed to spawn parec for {source}: {err}"))
        }
    })?;

    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AudioError::StreamError(
                "parec stdout pipe unavailable".to_string(),
            ));
        }
    };

    let reader_stop = Arc::clone(&stop_signal);
    let reader_failed = Arc::clone(&capture_failed);
    let reader_thread = std::thread::Builder::new()
        .name("liveaudio-pulse-reader".to_string())
        .spawn(move || {
            read_f32le_loop(stdout, producer, reader_stop, Arc::clone(&reader_failed));
            if reader_failed.load(Ordering::SeqCst) {
                warn!("PulseAudio/PipeWire capture stream ended unexpectedly");
            } else {
                debug!("PulseAudio/PipeWire capture reader stopped cleanly");
            }
        })
        .map_err(|err| {
            let _ = child.kill();
            let _ = child.wait();
            AudioError::StreamError(format!("Failed to spawn parec reader thread: {err}"))
        })?;

    Ok(PulseCaptureHandle {
        child: Some(child),
        reader_thread: Some(reader_thread),
        stop_signal,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ring_buffer::create_audio_ring_buffer;
    use std::io::Cursor;

    const SAMPLE_PACTL_INFO: &str = "\
Server String: /run/user/1000/pulse/native
Server Name: PulseAudio (on PipeWire 1.5.85)
Default Sink: alsa_output.pci-0000_00_1f.3.analog-stereo
Default Source: alsa_input.usb-Blue_Microphones-00.mono-fallback
";

    const SAMPLE_PACTL_SOURCES: &str = "\
Source #55
\tState: SUSPENDED
\tName: alsa_output.pci-0000_00_1f.3.analog-stereo.monitor
\tDescription: Monitor of Built-in Audio Analog Stereo
\tSample Specification: s32le 2ch 48000Hz
\tMonitor of Sink: alsa_output.pci-0000_00_1f.3.analog-stereo
\tProperties:
\t\tName: nested-should-be-ignored
\t\tDescription: nested-should-be-ignored

Source #56
\tState: RUNNING
\tName: alsa_input.usb-Blue_Microphones-00.mono-fallback
\tDescription: Yeti Stereo Microphone Mono
\tSample Specification: float32le 1ch 44100Hz
\tMonitor of Sink: n/a

Source #57
\tState: IDLE
\tName: virt_sink.monitor
\tDescription:
\tSample Specification: float32le 2ch 48000Hz
\tMonitor of Sink: n/a
";

    #[test]
    fn parses_pactl_info_and_sources_with_mics_monitors_and_defaults() {
        let info = parse_pactl_info(SAMPLE_PACTL_INFO);
        assert_eq!(
            info.default_sink.as_deref(),
            Some("alsa_output.pci-0000_00_1f.3.analog-stereo")
        );
        assert_eq!(
            info.default_source.as_deref(),
            Some("alsa_input.usb-Blue_Microphones-00.mono-fallback")
        );

        let devices = parse_pactl_list_sources(SAMPLE_PACTL_SOURCES, &info);
        assert_eq!(devices.len(), 3);

        // Microphones come first, then loopbacks.
        let mic = &devices[0];
        assert_eq!(mic.id, "pulse:alsa_input.usb-Blue_Microphones-00.mono-fallback");
        assert_eq!(mic.name, "🎤 Yeti Stereo Microphone Mono");
        assert!(mic.is_default);
        assert!(!mic.is_loopback);
        assert_eq!(mic.channels, 1);
        assert_eq!(mic.supported_sample_rates, vec![44100]);

        let default_mon = &devices[1];
        assert_eq!(
            default_mon.id,
            "loopback:alsa_output.pci-0000_00_1f.3.analog-stereo.monitor"
        );
        assert_eq!(
            default_mon.name,
            "🔊 Monitor of Built-in Audio Analog Stereo (Loopback)"
        );
        assert!(default_mon.is_default);
        assert!(default_mon.is_loopback);
        assert_eq!(default_mon.channels, 2);
        assert_eq!(default_mon.supported_sample_rates, vec![48000]);

        // Empty description falls back to raw source name; .monitor suffix marks loopback even if Monitor of Sink is n/a.
        let fallback_mon = &devices[2];
        assert_eq!(fallback_mon.id, "loopback:virt_sink.monitor");
        assert_eq!(fallback_mon.name, "🔊 virt_sink.monitor (Loopback)");
        assert!(!fallback_mon.is_default);
        assert!(fallback_mon.is_loopback);
    }

    #[test]
    fn handles_empty_and_malformed_pactl_output() {
        let info = parse_pactl_info("Default Sink: n/a\nDefault Source:   \n");
        assert_eq!(info, PactlInfo::default());
        assert!(parse_pactl_list_sources("", &info).is_empty());
        assert!(parse_pactl_list_sources("Source #1\n\tDescription: Orphan\n", &info).is_empty());
    }

    #[test]
    fn alsa_noise_filter_keeps_friendly_entries_and_preserves_non_empty_fallback() {
        let make = |id: &str| AudioDeviceInfo::new(id, id, false, 1, vec![48000], false);
        let devices = vec![
            make("default"),
            make("pipewire"),
            make("pulse"),
            make("null"),
            make("lavrate"),
            make("dsnoop:CARD=PCH,DEV=0"),
            make("dmix:CARD=PCH,DEV=0"),
            make("surround51:CARD=PCH"),
            make("iec958:CARD=PCH"),
            make("hdmi:CARD=PCH,DEV=0"),
            make("hw:CARD=PCH,DEV=0"),
            make("plughw:CARD=PCH,DEV=0"),
            make("USB Audio Mic"),
        ];

        let filtered = filter_linux_alsa_devices(devices);
        let ids: Vec<&str> = filtered.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, vec!["default", "pipewire", "pulse", "USB Audio Mic"]);

        // If every device would be filtered out, preserve the original list.
        let only_noisy = vec![make("null"), make("hw:0,0")];
        let preserved = filter_linux_alsa_devices(only_noisy.clone());
        assert_eq!(preserved, only_noisy);
    }

    #[test]
    fn resolves_linux_loopback_and_pulse_device_ids_and_rejects_flag_injection() {
        let info = parse_pactl_info(SAMPLE_PACTL_INFO);
        let sources = parse_pactl_list_sources(SAMPLE_PACTL_SOURCES, &info);

        assert_eq!(
            resolve_pulse_device_id(Some("default_loopback"), Some(&info), &sources).unwrap(),
            Some((
                "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor".to_string(),
                true
            ))
        );
        assert_eq!(
            resolve_pulse_device_id(Some("wasapi_loopback"), Some(&info), &sources).unwrap(),
            Some((
                "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor".to_string(),
                true
            ))
        );
        assert_eq!(
            resolve_pulse_device_id(
                Some("loopback:alsa_output.pci-0000_00_1f.3.analog-stereo.monitor"),
                Some(&info),
                &sources
            )
            .unwrap(),
            Some((
                "alsa_output.pci-0000_00_1f.3.analog-stereo.monitor".to_string(),
                true
            ))
        );
        assert_eq!(
            resolve_pulse_device_id(
                Some("pulse:alsa_input.usb-Blue_Microphones-00.mono-fallback"),
                Some(&info),
                &sources
            )
            .unwrap(),
            Some((
                "alsa_input.usb-Blue_Microphones-00.mono-fallback".to_string(),
                false
            ))
        );
        // Raw Pulse source name matches without prefix
        assert_eq!(
            resolve_pulse_device_id(
                Some("alsa_input.usb-Blue_Microphones-00.mono-fallback"),
                Some(&info),
                &sources
            )
            .unwrap(),
            Some((
                "alsa_input.usb-Blue_Microphones-00.mono-fallback".to_string(),
                false
            ))
        );
        // Unknown ALSA name returns None so caller falls back to CPAL
        assert_eq!(
            resolve_pulse_device_id(Some("default"), Some(&info), &sources).unwrap(),
            None
        );

        // Missing loopback target returns DeviceNotFound
        assert!(matches!(
            resolve_pulse_device_id(Some("loopback:__missing__"), Some(&info), &sources),
            Err(AudioError::DeviceNotFound(_))
        ));

        // Unavailable PulseAudio/PipeWire returns clear DeviceNotFound for default_loopback
        assert!(matches!(
            resolve_pulse_device_id(Some("default_loopback"), None, &[]),
            Err(AudioError::DeviceNotFound(_))
        ));

        // Leading '-' or empty name is rejected as InvalidParameter
        assert!(matches!(
            resolve_pulse_device_id(Some("loopback:--help"), Some(&info), &sources),
            Err(AudioError::InvalidParameter(_))
        ));
        assert!(matches!(
            resolve_pulse_device_id(Some("pulse:   "), Some(&info), &sources),
            Err(AudioError::InvalidParameter(_))
        ));
    }

    #[test]
    fn decodes_f32le_stream_across_unaligned_read_boundaries() {
        let expected = [0.25f32, -0.5, 1.0, 0.0];
        let mut raw = Vec::new();
        for s in expected {
            raw.extend_from_slice(&s.to_le_bytes());
        }

        let mut carry = Vec::new();
        let mut out = Vec::new();
        let mut collected = Vec::new();

        for chunk in [&raw[0..3], &raw[3..5], &raw[5..11], &raw[11..16]] {
            decode_f32le_stream(chunk, &mut carry, &mut out);
            collected.extend_from_slice(&out);
        }

        assert!(carry.is_empty());
        assert_eq!(collected, expected);
    }

    #[test]
    fn read_f32le_loop_signals_failure_on_unexpected_eof_and_not_on_clean_stop() {
        let mut bytes = Vec::new();
        for s in [0.1f32, 0.2, 0.3, 0.4] {
            bytes.extend_from_slice(&s.to_le_bytes());
        }

        // Unexpected EOF (stop == false) must set capture_failed = true
        let (producer, mut consumer) = create_audio_ring_buffer(64, 4);
        let stop = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        read_f32le_loop(Cursor::new(bytes.clone()), producer, stop, Arc::clone(&failed));
        assert!(failed.load(Ordering::SeqCst));
        let mut popped = [0.0f32; 4];
        assert_eq!(consumer.pop_samples(&mut popped), 4);
        assert_eq!(popped, [0.1f32, 0.2, 0.3, 0.4]);

        // Clean stop (stop == true) must NOT set capture_failed
        let (producer, _) = create_audio_ring_buffer(64, 4);
        let stop = Arc::new(AtomicBool::new(true));
        let failed = Arc::new(AtomicBool::new(false));
        read_f32le_loop(Cursor::new(bytes), producer, stop, Arc::clone(&failed));
        assert!(!failed.load(Ordering::SeqCst));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn run_command_with_timeout_captures_output_and_kills_hanging_child() {
        let mut fast = Command::new("sh");
        fast.args(["-c", "printf 'pulse-ok'"]);
        let out = run_command_with_timeout(&mut fast, std::time::Duration::from_secs(2));
        assert_eq!(out.as_deref(), Some("pulse-ok"));

        let started = std::time::Instant::now();
        let mut slow = Command::new("sleep");
        slow.arg("5");
        let timed_out = run_command_with_timeout(&mut slow, std::time::Duration::from_millis(100));
        assert_eq!(timed_out, None);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "hanging child should be killed promptly, elapsed: {:?}",
            started.elapsed()
        );
    }
}
