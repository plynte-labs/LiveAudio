// SPDX-License-Identifier: MIT

#[cfg(windows)]
#[path = "../../packaging/windows_version_info.rs"]
mod windows_version_info;

// ct2rs links cudnn64_9.dll as a regular import even with `cuda-dynamic-loading`, which would make
// the exe fail to start (e.g. `--self-test`) on machines without cuDNN. Delay-load it so the DLL
// is resolved (through PATH) only when CUDA is first used.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/DELAYLOAD:cudnn64_9.dll");
        println!("cargo:rustc-link-arg-bins=delayimp.lib");
    }
    #[cfg(windows)]
    windows_version_info::embed("liveaudio-asr-worker-cuda.exe", "LiveAudio ASR worker (NVIDIA CUDA)");
}
