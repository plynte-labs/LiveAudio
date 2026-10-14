// SPDX-License-Identifier: MIT

//! Capture supervision that keeps one downstream receiver alive across device
//! loss: standby without an input device, paced reconnects, and a stall watchdog.
//!
//! Timings mirror the Python `audio_producer` loop in `liveaudio/core/audio.py`.

use std::time::Duration;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{info, warn};

use crate::error::AudioError;
use crate::traits::{AudioChunkReceiver, AudioSource};
use crate::types::{AudioChunk, AudioSourceStatus, AudioStreamConfig};

/// Delay before re-opening a lost stream (`audio.py`: `shutdown_event.wait(3.0)`).
pub const RECONNECT_DELAY: Duration = Duration::from_secs(3);

/// Hot-plug poll while no input device exists (`audio.py`: `shutdown_event.wait(1.0)`).
pub const STANDBY_POLL: Duration = Duration::from_secs(1);

/// Watchdog check interval (`audio.py`: `sd.sleep(500)`).
pub const WATCHDOG_TICK: Duration = Duration::from_millis(500);

/// Stream age before the watchdog may fire (`audio.py`: `stream_age > 3.0`).
pub const WATCHDOG_GRACE: Duration = Duration::from_secs(3);

/// Silence from the hardware treated as a stalled stream (`audio.py`: `elapsed > 5.0`).
pub const WATCHDOG_LIMIT: Duration = Duration::from_secs(5);

/// Supervised capture handle; the paired [`AudioChunkReceiver`] survives reconnects.
pub struct ResilientCapture {
    state: watch::Receiver<AudioSourceStatus>,
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl ResilientCapture {
    /// Start supervised capture.
    ///
    /// A missing device or a stream failure does not fail the start: capture
    /// enters standby or reconnecting and resumes on its own. Only errors that a
    /// retry cannot fix (for example invalid parameters) are returned.
    pub async fn start<S, F>(
        source: S,
        config: AudioStreamConfig,
        input_device_count: F,
    ) -> Result<(Self, AudioChunkReceiver), AudioError>
    where
        S: AudioSource,
        F: Fn() -> usize + Send + Sync + 'static,
    {
        let (tx, rx) = mpsc::channel(config.ring_buffer_capacity.max(1));
        let (state_tx, state) = watch::channel(AudioSourceStatus::Initializing);
        let is_loopback = config
            .device_id
            .as_deref()
            .is_some_and(|id| id.starts_with("loopback:") || id == "default_loopback");
        let mut supervisor = Supervisor {
            source,
            config,
            input_device_count,
            is_loopback,
            tx,
            state: state_tx,
        };
        let first_step = match supervisor.attempt().await {
            Ok(step) => step,
            Err(error) if is_recoverable(&error) => {
                warn!("Audio capture could not open yet: {}", error);
                supervisor.after_loss()
            }
            Err(error) => return Err(error),
        };
        let (stop, stop_rx) = oneshot::channel();
        let task = tokio::spawn(supervisor.run(first_step, stop_rx));
        Ok((
            Self {
                state,
                stop: Some(stop),
                task: Some(task),
            },
            AudioChunkReceiver::new(rx),
        ))
    }

    /// Observe capture state changes (capturing, standby, reconnecting, stopped).
    pub fn state(&self) -> watch::Receiver<AudioSourceStatus> {
        self.state.clone()
    }

    /// Stop capture from any state (capturing, standby or waiting to reconnect).
    pub async fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

/// Device and stream failures heal on retry; parameter or backend errors do not.
fn is_recoverable(error: &AudioError) -> bool {
    matches!(
        error,
        AudioError::DeviceNotFound(_)
            | AudioError::StreamError(_)
            | AudioError::DeviceDisconnected
            | AudioError::BufferOverflow { .. }
    )
}

enum Step {
    Capture(AudioChunkReceiver),
    Wait(Duration),
}

struct Supervisor<S, F> {
    source: S,
    config: AudioStreamConfig,
    input_device_count: F,
    is_loopback: bool,
    tx: mpsc::Sender<AudioChunk>,
    state: watch::Sender<AudioSourceStatus>,
}

impl<S: AudioSource, F: Fn() -> usize> Supervisor<S, F> {
    fn set_state(&self, status: AudioSourceStatus) {
        if self.state.send_replace(status) == status {
            return;
        }
        match status {
            AudioSourceStatus::Standby => {
                warn!("No input audio device available. Waiting for hardware (standby)...")
            }
            AudioSourceStatus::Reconnecting => {
                warn!("Audio capture lost. Retrying every {:?}.", RECONNECT_DELAY)
            }
            AudioSourceStatus::Capturing => info!("Audio capture connected and listening."),
            _ => {}
        }
    }

    /// Loopback captures an output device, so it never waits for a microphone.
    fn input_device_missing(&self) -> bool {
        !self.is_loopback && (self.input_device_count)() == 0
    }

    /// Standby while no input device exists; otherwise a paced reconnect.
    fn after_loss(&self) -> Step {
        if self.input_device_missing() {
            self.set_state(AudioSourceStatus::Standby);
            Step::Wait(STANDBY_POLL)
        } else {
            self.set_state(AudioSourceStatus::Reconnecting);
            Step::Wait(RECONNECT_DELAY)
        }
    }

    async fn attempt(&mut self) -> Result<Step, AudioError> {
        if self.input_device_missing() {
            self.set_state(AudioSourceStatus::Standby);
            return Ok(Step::Wait(STANDBY_POLL));
        }
        let receiver = self.source.start(&self.config).await?;
        self.set_state(AudioSourceStatus::Capturing);
        Ok(Step::Capture(receiver))
    }

    async fn run(mut self, mut step: Step, mut stop: oneshot::Receiver<()>) {
        loop {
            step = match step {
                Step::Capture(mut receiver) => {
                    let outcome = self.forward(&mut receiver, &mut stop).await;
                    let _ = self.source.stop().await;
                    match outcome {
                        Ok(()) => break,
                        Err(reason) => {
                            warn!("Audio hardware lost or stream failed: {}", reason);
                            self.after_loss()
                        }
                    }
                }
                Step::Wait(delay) => {
                    tokio::select! {
                        _ = &mut stop => break,
                        _ = self.tx.closed() => break,
                        _ = tokio::time::sleep(delay) => {}
                    }
                    match self.attempt().await {
                        Ok(next) => next,
                        Err(error) => {
                            warn!("Audio capture reopen failed: {}", error);
                            self.after_loss()
                        }
                    }
                }
            };
        }
        self.set_state(AudioSourceStatus::Stopped);
    }

    /// Forward chunks until stop (`Ok`) or a loss, stream error, or stall (`Err`).
    async fn forward(
        &self,
        receiver: &mut AudioChunkReceiver,
        stop: &mut oneshot::Receiver<()>,
    ) -> Result<(), &'static str> {
        let opened = Instant::now();
        let mut last_chunk = opened;
        let mut watchdog = tokio::time::interval(WATCHDOG_TICK);
        loop {
            tokio::select! {
                _ = &mut *stop => return Ok(()),
                chunk = receiver.recv() => match chunk {
                    _ if receiver.capture_failed() => return Err("capture stream error"),
                    Some(chunk) => {
                        last_chunk = Instant::now();
                        match self.tx.try_send(chunk) {
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                warn!("Downstream audio queue saturated. Dropped incoming chunk.")
                            }
                            Err(mpsc::error::TrySendError::Closed(_)) => return Ok(()),
                            Ok(()) => {}
                        }
                    }
                    None => return Err("capture stream ended"),
                },
                // Loopback delivers no callbacks while nothing plays, so silence is not a stall.
                _ = watchdog.tick(), if !self.is_loopback => {
                    if opened.elapsed() > WATCHDOG_GRACE && last_chunk.elapsed() > WATCHDOG_LIMIT {
                        return Err("watchdog: no audio from hardware for over 5 s");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::AudioBoxFuture;
    use crate::types::AudioDeviceInfo;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::time::timeout;

    enum Attempt {
        Fail(AudioError),
        Open(mpsc::Receiver<AudioChunk>, Arc<AtomicBool>),
    }

    /// Hardware-free source whose successive `start` calls follow a script.
    #[derive(Clone, Default)]
    struct ScriptedSource {
        script: Arc<Mutex<VecDeque<Attempt>>>,
        starts: Arc<AtomicUsize>,
        stops: Arc<AtomicUsize>,
        status: Arc<Mutex<Option<AudioSourceStatus>>>,
    }

    impl ScriptedSource {
        fn push_fail(&self, error: AudioError) {
            self.script.lock().unwrap().push_back(Attempt::Fail(error));
        }

        /// Script one successful open; returns the hardware side of that stream.
        fn push_open(&self) -> (mpsc::Sender<AudioChunk>, Arc<AtomicBool>) {
            let (tx, rx) = mpsc::channel(16);
            let failed = Arc::new(AtomicBool::new(false));
            self.script
                .lock()
                .unwrap()
                .push_back(Attempt::Open(rx, failed.clone()));
            (tx, failed)
        }

        fn starts(&self) -> usize {
            self.starts.load(Ordering::SeqCst)
        }
    }

    impl AudioSource for ScriptedSource {
        fn enumerate_devices(
            &self,
        ) -> AudioBoxFuture<'_, Result<Vec<AudioDeviceInfo>, AudioError>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn start<'a>(
            &'a mut self,
            _config: &'a AudioStreamConfig,
        ) -> AudioBoxFuture<'a, Result<AudioChunkReceiver, AudioError>> {
            Box::pin(async move {
                self.starts.fetch_add(1, Ordering::SeqCst);
                let attempt = self.script.lock().unwrap().pop_front();
                match attempt {
                    Some(Attempt::Open(rx, failed)) => {
                        *self.status.lock().unwrap() = Some(AudioSourceStatus::Capturing);
                        Ok(AudioChunkReceiver::with_capture_error(rx, failed))
                    }
                    Some(Attempt::Fail(error)) => Err(error),
                    None => Err(AudioError::DeviceNotFound("scripted".to_string())),
                }
            })
        }

        fn stop(&mut self) -> AudioBoxFuture<'_, Result<(), AudioError>> {
            Box::pin(async move {
                self.stops.fetch_add(1, Ordering::SeqCst);
                *self.status.lock().unwrap() = Some(AudioSourceStatus::Stopped);
                Ok(())
            })
        }

        fn status(&self) -> AudioSourceStatus {
            self.status
                .lock()
                .unwrap()
                .unwrap_or(AudioSourceStatus::Stopped)
        }
    }

    fn devices(count: usize) -> (Arc<AtomicUsize>, impl Fn() -> usize + Send + Sync + 'static) {
        let shared = Arc::new(AtomicUsize::new(count));
        let probe = shared.clone();
        (shared, move || probe.load(Ordering::SeqCst))
    }

    fn chunk(sequence_id: u64) -> AudioChunk {
        AudioChunk::new(vec![0.0; 512], 16_000, 1, sequence_id)
    }

    async fn reaches(
        state: &mut watch::Receiver<AudioSourceStatus>,
        expected: AudioSourceStatus,
        within: Duration,
    ) -> bool {
        timeout(within, state.wait_for(|status| *status == expected))
            .await
            .map(|result| result.is_ok())
            .unwrap_or(false)
    }

    #[tokio::test(start_paused = true)]
    async fn no_input_device_enters_standby_and_resumes_on_hot_plug() {
        let source = ScriptedSource::default();
        let (hw_tx, _) = source.push_open();
        let (device_count, probe) = devices(0);

        let (mut capture, mut rx) =
            ResilientCapture::start(source.clone(), AudioStreamConfig::default(), probe)
                .await
                .expect("a missing device must not fail the start");
        let mut state = capture.state();
        assert_eq!(*state.borrow(), AudioSourceStatus::Standby);

        tokio::time::sleep(Duration::from_millis(2_500)).await;
        assert_eq!(source.starts(), 0, "standby must not hammer the backend");

        device_count.store(1, Ordering::SeqCst);
        assert!(reaches(&mut state, AudioSourceStatus::Capturing, STANDBY_POLL * 2).await);
        hw_tx.send(chunk(1)).await.unwrap();
        assert_eq!(rx.recv().await.map(|c| c.sequence_id), Some(1));
        capture.stop().await;
    }

    #[tokio::test(start_paused = true)]
    async fn device_loss_reconnects_after_three_seconds_on_the_same_receiver() {
        let source = ScriptedSource::default();
        let (first_tx, first_failed) = source.push_open();
        let (second_tx, _) = source.push_open();
        let (_devices, probe) = devices(1);

        let (mut capture, mut rx) =
            ResilientCapture::start(source.clone(), AudioStreamConfig::default(), probe)
                .await
                .unwrap();
        let mut state = capture.state();
        first_tx.send(chunk(1)).await.unwrap();
        assert_eq!(rx.recv().await.map(|c| c.sequence_id), Some(1));

        first_failed.store(true, Ordering::SeqCst);
        drop(first_tx);
        assert!(reaches(&mut state, AudioSourceStatus::Reconnecting, WATCHDOG_TICK).await);
        assert!(
            !reaches(
                &mut state,
                AudioSourceStatus::Capturing,
                RECONNECT_DELAY - Duration::from_millis(100)
            )
            .await,
            "reconnect must wait the Python retry delay"
        );
        assert!(
            reaches(
                &mut state,
                AudioSourceStatus::Capturing,
                Duration::from_millis(200)
            )
            .await
        );
        assert_eq!(source.starts(), 2);

        second_tx.send(chunk(2)).await.unwrap();
        assert_eq!(rx.recv().await.map(|c| c.sequence_id), Some(2));
        capture.stop().await;
    }

    #[tokio::test(start_paused = true)]
    async fn failed_reopen_keeps_retrying_every_three_seconds() {
        let source = ScriptedSource::default();
        source.push_fail(AudioError::DeviceNotFound("Selected mic".to_string()));
        source.push_fail(AudioError::StreamError("busy".to_string()));
        let (hw_tx, _) = source.push_open();
        let (_devices, probe) = devices(1);

        let (mut capture, mut rx) =
            ResilientCapture::start(source.clone(), AudioStreamConfig::default(), probe)
                .await
                .expect("a missing selected device retries instead of failing");
        let mut state = capture.state();
        assert_eq!(*state.borrow(), AudioSourceStatus::Reconnecting);

        tokio::time::sleep(RECONNECT_DELAY + Duration::from_millis(100)).await;
        assert_eq!(source.starts(), 2);
        assert!(reaches(&mut state, AudioSourceStatus::Capturing, RECONNECT_DELAY).await);
        assert_eq!(source.starts(), 3);
        hw_tx.send(chunk(7)).await.unwrap();
        assert_eq!(rx.recv().await.map(|c| c.sequence_id), Some(7));
        capture.stop().await;
    }

    #[tokio::test(start_paused = true)]
    async fn loss_with_no_input_device_left_enters_standby() {
        let source = ScriptedSource::default();
        let (first_tx, _) = source.push_open();
        let _second = source.push_open();
        let (device_count, probe) = devices(1);

        let (mut capture, _rx) =
            ResilientCapture::start(source.clone(), AudioStreamConfig::default(), probe)
                .await
                .unwrap();
        let mut state = capture.state();

        device_count.store(0, Ordering::SeqCst);
        drop(first_tx);
        assert!(reaches(&mut state, AudioSourceStatus::Standby, WATCHDOG_TICK).await);
        tokio::time::sleep(STANDBY_POLL * 3).await;
        assert_eq!(source.starts(), 1, "standby polls the device count only");

        device_count.store(1, Ordering::SeqCst);
        assert!(reaches(&mut state, AudioSourceStatus::Capturing, STANDBY_POLL * 2).await);
        capture.stop().await;
    }

    #[tokio::test(start_paused = true)]
    async fn watchdog_reconnects_a_stream_that_stops_delivering_audio() {
        let source = ScriptedSource::default();
        let (_silent_hw, _) = source.push_open();
        let _second = source.push_open();
        let (_devices, probe) = devices(1);

        let (mut capture, _rx) =
            ResilientCapture::start(source.clone(), AudioStreamConfig::default(), probe)
                .await
                .unwrap();
        let mut state = capture.state();

        assert!(
            !reaches(
                &mut state,
                AudioSourceStatus::Reconnecting,
                WATCHDOG_LIMIT - Duration::from_millis(100)
            )
            .await,
            "the watchdog must tolerate up to the Python limit"
        );
        assert!(
            reaches(
                &mut state,
                AudioSourceStatus::Reconnecting,
                WATCHDOG_TICK * 2
            )
            .await
        );
        assert_eq!(
            source.stops.load(Ordering::SeqCst),
            1,
            "stalled stream is released"
        );
        capture.stop().await;
    }

    #[tokio::test(start_paused = true)]
    async fn loopback_capture_skips_the_input_device_standby_probe() {
        let source = ScriptedSource::default();
        let _hw = source.push_open();
        let (_devices, probe) = devices(0);
        let config = AudioStreamConfig {
            device_id: Some("default_loopback".to_string()),
            ..AudioStreamConfig::default()
        };

        let (mut capture, _rx) = ResilientCapture::start(source.clone(), config, probe)
            .await
            .unwrap();
        assert_eq!(*capture.state().borrow(), AudioSourceStatus::Capturing);
        capture.stop().await;
    }

    #[tokio::test(start_paused = true)]
    async fn silent_loopback_is_not_restarted_by_the_watchdog() {
        let source = ScriptedSource::default();
        let (_silent_hw, _) = source.push_open();
        let (_devices, probe) = devices(1);
        let config = AudioStreamConfig {
            device_id: Some("loopback:Speakers".to_string()),
            ..AudioStreamConfig::default()
        };

        let (mut capture, _rx) = ResilientCapture::start(source.clone(), config, probe)
            .await
            .unwrap();
        let mut state = capture.state();
        assert!(
            !reaches(
                &mut state,
                AudioSourceStatus::Reconnecting,
                WATCHDOG_LIMIT * 3
            )
            .await
        );
        assert_eq!(source.starts(), 1);
        capture.stop().await;
    }

    #[tokio::test(start_paused = true)]
    async fn unrecoverable_start_error_is_returned() {
        let source = ScriptedSource::default();
        source.push_fail(AudioError::InvalidParameter("bad".to_string()));
        let (_devices, probe) = devices(1);

        let result =
            ResilientCapture::start(source.clone(), AudioStreamConfig::default(), probe).await;
        assert!(matches!(result, Err(AudioError::InvalidParameter(_))));
    }

    #[tokio::test(start_paused = true)]
    async fn stop_completes_promptly_in_standby_and_while_waiting_to_reconnect() {
        let standby = ScriptedSource::default();
        let (_devices, no_devices) = devices(0);
        let (mut capture, mut rx) =
            ResilientCapture::start(standby, AudioStreamConfig::default(), no_devices)
                .await
                .unwrap();
        assert!(timeout(Duration::from_millis(100), capture.stop())
            .await
            .is_ok());
        assert_eq!(*capture.state().borrow(), AudioSourceStatus::Stopped);
        assert!(
            rx.recv().await.is_none(),
            "stop closes the downstream receiver"
        );

        let reconnecting = ScriptedSource::default();
        reconnecting.push_fail(AudioError::StreamError("busy".to_string()));
        let (_devices, one_device) = devices(1);
        let (mut capture, _rx) = ResilientCapture::start(
            reconnecting.clone(),
            AudioStreamConfig::default(),
            one_device,
        )
        .await
        .unwrap();
        assert_eq!(*capture.state().borrow(), AudioSourceStatus::Reconnecting);
        assert!(timeout(Duration::from_millis(100), capture.stop())
            .await
            .is_ok());
        assert_eq!(reconnecting.starts(), 1);
    }
}
