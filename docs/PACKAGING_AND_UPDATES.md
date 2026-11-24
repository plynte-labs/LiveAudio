# Packaging and Updates

Maintainer reference for how LiveAudio is built, released and auto-updated.
The sources of truth are `.github/workflows/release.yml`, `packaging/`,
`desktop/src-tauri/tauri.conf.json` and `desktop/src-tauri/tauri.release.conf.json`.
If this page and those files disagree, the files win; fix this page.

## 1. What a release contains

The release workflow runs on a `v*` tag and builds two platforms.

| Asset | Built on | Contents |
|---|---|---|
| NSIS installer (`*-setup.exe`) and its `.sig` | `windows-latest` | Tauri desktop app with the `liveaudio-cli` and `liveaudio-asr-worker` sidecars, `onnxruntime.dll`, `onnxruntime_providers_shared.dll` and `DirectML.dll`. Per-user install. |
| `liveaudio-cli-X.Y.Z-windows-x64.zip` | `windows-latest` | Standalone CLI and ASR worker with the ONNX Runtime and DirectML DLLs and their licenses. |
| `latest.json` | `publish` job | Tauri updater manifest (`windows-x86_64` NSIS + `linux-x86_64` AppImage, see section 4). |
| AppImage (`*.AppImage`, `.sig`), `*.deb` | `ubuntu-22.04` | Desktop app bundles (`targets: ["appimage", "deb"]` in `tauri.linux.conf.json`). |
| `liveaudio-cli-X.Y.Z-linux-x64.tar.gz` | `ubuntu-22.04` | Standalone CLI and ASR worker with ONNX Runtime CPU. |
| `SHA256SUMS.txt` | publish job | SHA-256 of every asset above. |

The `publish` job creates the GitHub release as a **draft** with generated
notes. Nothing is public until a maintainer reviews the draft and publishes it.
The updater endpoint (`releases/latest/download/latest.json`) only serves a
published release.

The CUDA ASR worker (`liveaudio-asr-worker-cuda.exe`, CTranslate2) is **not built by CI** and
is not in the installer. The maintainer builds it locally and uploads it to the release as an
separate `ct2-worker-vX.Y.Z` release (section 5), on its own cycle. The app downloads it on demand, with the NVIDIA cuBLAS/cuDNN DLLs,
after an explicit user click; the download is pinned by `packaging/ct2-worker.pin.json`
(URL, SHA-256, size) and verified before use.

Whisper model weights are not bundled. They are downloaded at runtime from
Hugging Face. The Silero VAD model is embedded in the binary.

## 2. Build locally

The steps mirror CI. Run them from the repository root unless noted.

1. Fetch the pinned native libraries (not committed):
   ```bash
   node packaging/fetch_onnxruntime.mjs                # current OS
   node packaging/fetch_onnxruntime.mjs windows-x64 linux-x64
   ```
   On Windows the script also copies the four VC++ runtime DLLs (`vcruntime140`, `vcruntime140_1`, `msvcp140`, `msvcp140_1`) from the local Visual Studio `VC\Redist` (found via vswhere; not hash-pinned) so they ship app-locally next to the exes.
   The script downloads official NuGet packages for ONNX Runtime 1.24.4 (`Microsoft.ML.OnnxRuntime.DirectML` on Windows, `Microsoft.ML.OnnxRuntime` on Linux) and DirectML 1.15.4 on Windows,
   checks each archive against a pinned SHA-256 and aborts on mismatch. The
   pins live in `packaging/fetch_onnxruntime.mjs`.
2. Build the sidecars:
   ```bash
   cargo build --release --locked -p liveaudio-asr-worker -p liveaudio-cli
   ```
3. Copy them into `desktop/src-tauri/binaries/` (gitignored) using Tauri's
   external-binary naming, `<name>-<target-triple>[.exe]`. `rustc -vV` prints the
   triple. `release.yml` shows the exact copy commands.
4. Build the bundle. The release config only adds the sidecars and must be
   passed explicitly. The signing key must be in the environment, as in CI:
   ```bash
   cd desktop
   TAURI_SIGNING_PRIVATE_KEY=... TAURI_SIGNING_PRIVATE_KEY_PASSWORD=... \
     npx --yes @tauri-apps/cli@2.12.1 build --config src-tauri/tauri.release.conf.json
   ```

Version consistency: CI fails when the tag (`vX.Y.Z`) does not match the
`liveaudio-desktop` package version in the Cargo workspace. Keep
`version` in `desktop/src-tauri/tauri.conf.json` on the same number, because the
installer and the updater compare that value.

## 3. Signing

### Updater signatures (Minisign)

- The updater uses Tauri's Minisign scheme. Bundles get a `.sig` at build time
  when `createUpdaterArtifacts` is on (it is in `tauri.conf.json`).
- The public key is `plugins.updater.pubkey` in `desktop/src-tauri/tauri.conf.json`.
- The private key and its password are GitHub Actions secrets:
  `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Never
  commit them. Generate a new pair with `npx @tauri-apps/cli signer generate -w <path-outside-repo>`.
- Rotating the key is a breaking step: an installed app only accepts updates
  signed by the public key it was built with, so a rotation needs a release that
  users install manually.

### Authenticode (Windows code signing): not in place

The installer and the CLI executables are **not Authenticode-signed**. The
release workflow has no signing step. Windows SmartScreen therefore shows an
"Unrecognized app" / "Unknown publisher" warning on first run. Minisign signs
updates; it does not replace Authenticode for SmartScreen. Adding Authenticode
signing (and reputation building) is an open item with no active track.

## 4. Update manifest (`latest.json`)

`packaging/assemble_latest_manifest.mjs` runs in the `publish` job after both
platform bundles are built:

```bash
node packaging/assemble_latest_manifest.mjs --version X.Y.Z --out-dir release_assets
```

- It finds the Windows NSIS installer and its `.sig`, plus the Linux `.AppImage`
  and its `.sig` when present.
- It writes `latest.json` with `version`, `notes`, `pub_date` and
  `platforms["windows-x86_64"]` and `platforms["linux-x86_64"]` (`signature` =
  the raw `.sig` contents, `url` =
  `https://github.com/<repo>/releases/download/<tag>/<bundle file name>`).
- `repo` and `tag` default to `GITHUB_REPOSITORY` and `GITHUB_REF_NAME`.
- Linux AppImage users update in-app via `platforms["linux-x86_64"]`. Linux
  `.deb` installations cannot self-update in-app and update via GitHub Releases.

The app checks `https://github.com/plynte-labs/LiveAudio/releases/latest/download/latest.json`
(`plugins.updater.endpoints`), installs in `passive` mode on Windows, and
replaces the AppImage in-place on Linux.

## 5. Checklist for a release

1. Bump the version in `desktop/src-tauri/tauri.conf.json` and the workspace
   `Cargo.toml`. Update `CHANGELOG.md` and `HISTORIAL_CAMBIOS.md`.
2. The CUDA worker ships on its own cycle, tagged `ct2-worker-vX.Y.Z` where `X.Y.Z` is the
   `crates/liveaudio-asr-worker-ct2` crate version (not the app version). Rebuild and
   re-pin it only when that crate changes (needs the CUDA toolchain, about 18 minutes; see
   `crates/liveaudio-asr-worker-ct2/README.md`):
   ```powershell
   .\packaging\build_ct2_worker.ps1 -CudaRoot <toolkit root> -Pin
   ```
   `-Pin` copies the exe to `dist-ct2/` (gitignored) and rewrites
   `packaging/ct2-worker.pin.json` with the crate version, the
   `ct2-worker-vX.Y.Z` release URL, SHA-256 and size. Create that release once and upload
   the exe (it must match the pin URL):
   ```bash
   gh release create ct2-worker-vX.Y.Z --latest=false dist-ct2/liveaudio-asr-worker-cuda.exe --title "CUDA worker X.Y.Z" --notes "CTranslate2 CUDA worker for LiveAudio."
   ```
   The "Latest" GitHub release must always be an app release: the updater reads
   `releases/latest/download/latest.json`, so never mark a worker release as latest.
   Published `ct2-worker-v*` releases are immutable. Any change to the worker or to its path
   dependency `liveaudio-worker-protocol` requires bumping the ct2 crate version and a new
   tag; never re-upload an asset under an existing tag.
   A normal app release needs none of this: leave the pin untouched.
3. Commit `packaging/ct2-worker.pin.json` when it changed. The app embeds it at compile
   time, so it must be in the tagged commit. While it holds the all-zero SHA-256
   placeholder the app reports the worker as not published, hides the NVIDIA offer and
   keeps using the ONNX Runtime worker. A worker that reports a different protocol
   version than the app is stopped at startup and the app falls back to ONNX Runtime.
4. Tag `vX.Y.Z` and push the tag. The workflow builds the app (unchanged; it does not
   build or upload the CUDA worker).
5. Before publishing the app draft, check that the `ct2-worker-v...` release named in the
   pin exists and is published.
6. Check that the draft has all assets, that `latest.json` points at the installer
   name, and that `SHA256SUMS.txt` matches. The worker is not in `SHA256SUMS.txt`
   (the pin file holds its hash).
7. Smoke-test the installer on a clean Windows machine, including the on-demand
   NVIDIA install. Follow [UPDATER_TESTING.md](UPDATER_TESTING.md) for the updater path.
8. Publish the draft. The worker release must already be published: users download it
   from the pin URL. It must hold the exact exe whose SHA-256 and size are in the
   committed pin; the installer downloads and verifies the worker before the NVIDIA
   archives, so a missing or different asset fails the install early.
9. Before publishing, complete the signed-update round trip in
   [UPDATER_TESTING.md](UPDATER_TESTING.md) with the real signing secret: CI cannot
   prove the secret matches the committed public key.

## 6. Troubleshooting

| Symptom | Likely cause |
|---|---|
| `fetch_onnxruntime.mjs`: SHA-256 mismatch | Upstream archive changed or a proxy rewrote it. Do not edit the pin without checking the upstream release. |
| CI: tag does not match Cargo version | The tag or `liveaudio-desktop` version is wrong. Re-tag after fixing the version. |
| CI: missing `DirectML.dll` or `onnxruntime.dll` | `fetch_onnxruntime.mjs` did not run before the build. |
| Updater reports no update | The release is still a draft, or the manifest version is not greater than the installed version. |
| SmartScreen warning on install | Expected while the installer is unsigned (section 3). |

Related: [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md) (licenses shipped
in the bundle), [packaging/onnxruntime/README.md](../packaging/onnxruntime/README.md)
(native library layout), [UPDATER_TESTING.md](UPDATER_TESTING.md) (VM test recipe).
