// SPDX-License-Identifier: MIT

//! Pure, GPU-free logic of the CTranslate2 (CUDA) ASR worker: model mapping, cache layout,
//! load planning and model download. The CTranslate2 engine itself lives in `main.rs`.

pub mod download;
pub mod model;
