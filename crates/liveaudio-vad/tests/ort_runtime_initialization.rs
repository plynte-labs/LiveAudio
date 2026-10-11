use std::process::Command;

use liveaudio_vad::{initialize_onnx_runtime, SileroVad, VadConfig, VadEngine};

const CHILD_MODE: &str = "LIVEAUDIO_ORT_INITIALIZATION_TEST";

fn packaged_cpu_library() -> std::path::PathBuf {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/onnxruntime");
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return root.join("windows-x64/onnxruntime.dll");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return root.join("linux-x64/libonnxruntime.so.1.26.0");
    #[allow(unreachable_code)]
    root.join("unsupported-platform/onnxruntime")
}

fn bundled_silero_model() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("models/silero_vad_v6.onnx")
}

#[test]
fn missing_and_invalid_runtime_libraries_return_errors_without_panicking() {
    if let Ok(mode) = std::env::var(CHILD_MODE) {
        if mode == "valid" {
            let library = std::env::var_os("LIVEAUDIO_ORT_TEST_LIBRARY")
                .expect("explicit ORT library test path");
            let model = std::env::var_os("LIVEAUDIO_ORT_TEST_MODEL")
                .expect("explicit Silero model test path");
            initialize_onnx_runtime(library).expect("load explicit app-owned CPU library");
            let mut vad = SileroVad::from_model_path(model, VadConfig::default())
                .expect("load Silero model after explicit ORT initialization");
            let silence = vec![0.0; 512];
            let mut first_probability = None;
            let mut tail_probability = 0.0;
            for _ in 0..512 {
                let decision = vad
                    .evaluate_chunk(&silence)
                    .expect("evaluate a 512-sample silence frame");
                assert!(decision.speech_probability.is_finite());
                assert!((0.0..=1.0).contains(&decision.speech_probability));
                assert!(!decision.is_speech);
                first_probability.get_or_insert(decision.speech_probability);
                tail_probability = decision.speech_probability;
            }
            assert!(tail_probability < 0.1);
            vad.reset();
            let reset_probability = vad
                .evaluate_chunk(&silence)
                .expect("evaluate first frame after reset")
                .speech_probability;
            assert!((reset_probability - first_probability.unwrap()).abs() < 1e-5);
            return;
        }

        let root =
            std::env::temp_dir().join(format!("liveaudio-ort-test-{}-{mode}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create isolated test directory");
        let library = root.join("onnxruntime-invalid.dll");
        if mode == "invalid" {
            std::fs::write(&library, b"not an ONNX Runtime shared library")
                .expect("write invalid library fixture");
        }

        let error = initialize_onnx_runtime(&library).expect_err("invalid runtime must fail");
        assert!(error.to_string().contains("ONNX Runtime"));
        return;
    }

    for mode in ["missing", "invalid"] {
        let result = Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                "missing_and_invalid_runtime_libraries_return_errors_without_panicking",
                "--nocapture",
            ])
            .env(CHILD_MODE, mode)
            .status()
            .expect("spawn isolated ORT initialization test");
        assert!(
            result.success(),
            "{mode} runtime should return a recoverable error"
        );
    }
}

#[test]
fn verified_cpu_library_loads_silero_and_infers_without_audio_hardware() {
    let library = std::env::var_os("LIVEAUDIO_ORT_TEST_LIBRARY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(packaged_cpu_library);
    let model = std::env::var_os("LIVEAUDIO_ORT_TEST_MODEL")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(bundled_silero_model);
    let result = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "missing_and_invalid_runtime_libraries_return_errors_without_panicking",
            "--nocapture",
        ])
        .env(CHILD_MODE, "valid")
        .env("LIVEAUDIO_ORT_TEST_LIBRARY", library)
        .env("LIVEAUDIO_ORT_TEST_MODEL", model)
        .status()
        .expect("spawn isolated valid ORT initialization test");
    assert!(
        result.success(),
        "explicit CPU ORT inference subprocess must pass"
    );
}
