// SPDX-License-Identifier: MIT

//! Shared stdin/stdout JSON-lines protocol and generic worker loop for LiveAudio ASR workers.

pub mod protocol;
pub mod worker;

pub use worker::{run_self_test_cli, run_worker, Engine, LoadError, ProgressFn, ReadyInfo, Transcript};
