# Bundled CPU ONNX Runtime assets

LiveAudio bundles ONNX Runtime 1.26.0 CPU libraries. The pinned Rust `ort`
dependency uses API 17, which this release supports. Runtime libraries are
loaded only from the absolute, application-owned resource path. The application
does not download or search for ONNX Runtime at startup.

## Pinned upstream artifacts

| Target | Official Microsoft release asset | Size | SHA-256 |
| --- | --- | ---: | --- |
| Windows x64 | [onnxruntime-win-x64-1.26.0.zip](https://github.com/microsoft/onnxruntime/releases/download/v1.26.0/onnxruntime-win-x64-1.26.0.zip) (release asset 415503188) | 75,675,381 bytes | `6ebe99b5564bf4d029b6e93eac9ff423682b6212eade769e9ca3f685eaf500b4` |
| Linux x64 | [onnxruntime-linux-x64-1.26.0.tgz](https://github.com/microsoft/onnxruntime/releases/download/v1.26.0/onnxruntime-linux-x64-1.26.0.tgz) (release asset 415501317) | 8,590,023 bytes | `1254da24fb389cf39dc0ff3451ab48301740ffbfcbaf646849df92f80ee92c57` |

The archive digest and release version were verified before extraction. Each
platform directory contains the CPU runtime library, the package's shared
provider companion, `VERSION_NUMBER`, `LICENSE`, and `ThirdPartyNotices.txt`.
Tauri's platform configuration explicitly packages these files. The runtime
licenses/notices are preserved alongside each platform asset set.

## Validation boundary

The Windows x64 library was loaded in an isolated process and used for 512
sequential 16 kHz Silero frames without audio hardware. Linux package assets
were verified by release digest and extracted, but this development host has
no installed Linux Rust target or Linux runner; Linux inference and packaged
launch therefore remain unverified locally. CI validates the native Linux
CPU inference path on its Ubuntu runner. Neither check proves a Linux desktop
installer launch or clean-machine installation.
