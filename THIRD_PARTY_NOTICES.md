# Third-party notices

Full license texts for every Rust crate are generated into `THIRD_PARTY_LICENSES.html` by cargo-about in the release build.

## nnnoiseless 0.5.2 — BSD-3-Clause

Copyright (c) 2020, Joe Neeman  
Copyright (c) 2017, Mozilla  
Copyright (c) 2007-2017, Jean-Marc Valin  
Copyright (c) 2005-2017, Xiph.Org Foundation  
Copyright (c) 2003-2004, Mark Borgerding

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

- Redistributions of source code must retain the above copyright
  notice, this list of conditions and the following disclaimer.

- Redistributions in binary form must reproduce the above copyright
  notice, this list of conditions and the following disclaimer in the
  documentation and/or other materials provided with the distribution.

- Neither the name of the Xiph.Org Foundation nor the names of its
  contributors may be used to endorse or promote products derived from
  this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION
OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

## Silero VAD v6.0 16 kHz op15 model — MIT

The bundled ONNX model is `crates/liveaudio-vad/models/silero_vad_v6.onnx`, embedded at build time
with `include_bytes!` in `crates/liveaudio-vad/src/silero.rs`. It is the upstream
`src/silero_vad/data/silero_vad_16k_op15.onnx` from the `snakers4/silero-vad` repository at commit
`fba061dc5559f696e62171e9a0741782b0fdc23c` (Git blob `3036c9e2a0293175d098dbbec63c229c1e0f49fa`,
identical to the local file). It is distributed under the following license:

MIT License

Copyright (c) 2020-present Silero Team

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## ONNX Runtime 1.24.4 and DirectML 1.15.4 — MIT and Microsoft Software License Terms

ONNX Runtime 1.24.4 (MIT License, Copyright (c) Microsoft Corporation) is
redistributed with the app: `onnxruntime.dll` and `onnxruntime_providers_shared.dll`
on Windows (NuGet `Microsoft.ML.OnnxRuntime.DirectML`), `libonnxruntime.so.1.24.4`
and `libonnxruntime_providers_shared.so` on Linux (official GitHub release).
Its license and third-party notices ship per platform under
`onnxruntime/<platform>/LICENSE` and `onnxruntime/<platform>/ThirdPartyNotices.txt`.

DirectML 1.15.4 (`DirectML.dll`, NuGet `Microsoft.AI.DirectML`) is redistributed
on Windows as part of the app under the Microsoft Software License Terms
(Microsoft DirectX Machine Learning (DirectML)), which are NOT the MIT license.
The full text ships as `onnxruntime/windows-x64/DirectML-LICENSE.txt`.

## Whisper model weights — downloaded at runtime, not redistributed

LiveAudio does not ship Whisper weights. On first use the app downloads the selected model from `huggingface.co`:
the ONNX Runtime worker uses `crates/liveaudio-asr-worker/src/download.rs`, and the CUDA worker uses
`crates/liveaudio-asr-worker-ct2/src/model.rs`. Licenses are those each repository declares in its Hugging Face
metadata (`https://huggingface.co/api/models/<repo>`), checked on 2026-11-08.

| Model | Repository | Declared license |
|---|---|---|
| tiny (ONNX) | `onnx-community/whisper-tiny` | License not declared on the card. Its base model `openai/whisper-tiny` is Apache-2.0. |
| base (ONNX) | `onnx-community/whisper-base` | Apache-2.0 |
| small (ONNX) | `onnx-community/whisper-small` | License not declared on the card. Its base model `openai/whisper-small` is Apache-2.0. |
| medium and medium.en (ONNX) | `onnx-community/whisper-medium-ONNX` | License not declared on the card, and no base model is declared. Upstream `openai/whisper-medium` is Apache-2.0. The multilingual model also serves `medium.en`. |
| large-v3-turbo (ONNX) | `onnx-community/whisper-large-v3-turbo` | License not declared on the card. Its base model `openai/whisper-large-v3-turbo` is MIT. |
| tiny (CUDA) | `Systran/faster-whisper-tiny` | MIT |
| base (CUDA) | `Systran/faster-whisper-base` | MIT |
| small (CUDA) | `Systran/faster-whisper-small` | MIT |
| medium (CUDA) | `Systran/faster-whisper-medium` | MIT |
| large-v3 (CUDA) | `Systran/faster-whisper-large-v3` | MIT |
| turbo (CUDA) | `deepdml/faster-whisper-large-v3-turbo-ct2` | MIT |
| preprocessor config for tiny, base, small, medium (CUDA) | `openai/whisper-{size}` | Apache-2.0 |

The ONNX worker maps `large`, `large-v2` and `large-v3` to the turbo repository. Other sizes the CUDA worker accepts
(`large-v1`, `large-v2`, `.en` variants) were not checked.

## Optional NVIDIA CUDA worker — ct2rs and CTranslate2 (MIT)

The optional CUDA ASR worker (`liveaudio-asr-worker-cuda.exe`, built locally and attached to
the GitHub release, downloaded only on user request) statically links the two MIT-licensed components below.

### ct2rs 0.10.4 — MIT

Source: https://github.com/jkawamoto/ctranslate2-rs

Copyright (c) 2023-2025 Junpei Kawamoto

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

### CTranslate2 — MIT (vendored in ct2rs 0.10.4)

Source: https://github.com/OpenNMT/CTranslate2

Copyright (c) 2018- SYSTRAN.

Copyright (c) 2019- The OpenNMT Authors.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

CTranslate2 vendors further components under their own licenses (cpuinfo: BSD-2-Clause; CUTLASS, cub and GoogleTest: BSD-3-Clause; cpu_features, ruy and Thrust: Apache-2.0; cxxopts and spdlog: MIT), as listed in the ct2rs README. Their notices are not reproduced here.

NVIDIA cuBLAS and cuDNN are NOT distributed with LiveAudio and are not stored in the repository or the installer. When the user asks for GPU acceleration, the app downloads pinned archives directly from NVIDIA's servers (`developer.download.nvidia.com`) after the user accepts NVIDIA's license terms, which are shown before the download. It extracts only the listed DLLs and the NVIDIA license text into `%LOCALAPPDATA%\LiveAudio\runtime\cuda12`, verifying each archive by SHA-256 and size (`crates/liveaudio-core/src/nvidia_runtime.rs`).

- cuBLAS: https://developer.download.nvidia.com/compute/cuda/redist/libcublas/LICENSE.txt
- cuDNN: https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/LICENSE.txt

Lucide icons (inline SVG paths for the info, bug, star, heart, triangle-alert, refresh-cw, download, play and chevron-right icons, copied from lucide-static 0.468.0) are distributed under the ISC License. https://lucide.dev

ISC License

Copyright (c) for portions of Lucide are held by Cole Bemis 2013-2022 as part of Feather (MIT). All other copyright (c) for Lucide are held by Lucide Contributors 2022.

Permission to use, copy, modify, and/or distribute this software for any purpose with or without fee is hereby granted, provided that the above copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
