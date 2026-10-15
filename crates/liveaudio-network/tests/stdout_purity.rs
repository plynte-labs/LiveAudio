// SPDX-License-Identifier: MIT

//! `liveaudio-cli serve` stdout is a machine-readable JSON-lines channel (OpenCohost parses it
//! line by line), so this library must log through `tracing`, never print to stdout.

#[test]
fn network_library_never_prints_to_stdout() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in std::fs::read_dir(&src).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        let source = std::fs::read_to_string(&path).expect("read source");
        for (index, line) in source.lines().enumerate() {
            assert!(
                !line.contains("println!") && !line.contains("print!("),
                "{}:{} prints to stdout: {}",
                path.display(),
                index + 1,
                line.trim()
            );
        }
    }
}
