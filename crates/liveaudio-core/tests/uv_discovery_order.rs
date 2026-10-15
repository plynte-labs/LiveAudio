//! Single test per binary: it changes the process working directory and PATH.

use liveaudio_core::runtime::find_uv_executable;
use std::{env, fs, path::PathBuf};

#[test]
fn uv_planted_in_current_directory_never_beats_path() {
    let root = env::temp_dir().join(format!("liveaudio-uv-order-{}", std::process::id()));
    let planted_dir = root.join("planted");
    let path_dir = root.join("path");
    fs::create_dir_all(&planted_dir).unwrap();
    fs::create_dir_all(&path_dir).unwrap();
    let name = if cfg!(windows) { "uv.exe" } else { "uv" };
    fs::write(planted_dir.join(name), "planted").unwrap();
    let real = path_dir.join(name);
    fs::write(&real, "real").unwrap();

    let old_cwd = env::current_dir().unwrap();
    let old_path = env::var_os("PATH").unwrap_or_default();
    // A relative PATH entry resolves against the cwd, so it must be ignored too.
    let path = env::join_paths([PathBuf::from("."), path_dir.clone()]).unwrap();
    env::set_current_dir(&planted_dir).unwrap();
    env::set_var("PATH", &path);
    let found = find_uv_executable();
    env::set_current_dir(old_cwd).unwrap();
    env::set_var("PATH", old_path);
    fs::remove_dir_all(&root).ok();

    assert_eq!(found.as_deref(), Some(real.as_path()));
}
