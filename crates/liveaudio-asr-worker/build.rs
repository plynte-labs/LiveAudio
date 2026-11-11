#[cfg(windows)]
#[path = "../../packaging/windows_version_info.rs"]
mod windows_version_info;

fn main() {
    #[cfg(windows)]
    windows_version_info::embed("liveaudio-asr-worker.exe", "LiveAudio ASR worker (ONNX Runtime)");
}
