// SPDX-License-Identifier: MIT

//! Minimal command-line flag parsing for the desktop launcher.

/// Flag that makes the desktop app start the transcription service after setup.
pub const AUTOSTART_FLAG: &str = "--autostart-service";

/// Returns true when `args` contains exactly `--autostart-service`.
/// The first item (executable path) is ignored. No case folding, no `=` forms.
pub fn autostart_requested(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().skip(1).any(|arg| arg == AUTOSTART_FLAG)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn flag_present_is_detected() {
        assert!(autostart_requested(v(&["app.exe", "--autostart-service"])));
    }

    #[test]
    fn flag_absent_is_false() {
        assert!(!autostart_requested(v(&["app.exe"])));
        assert!(!autostart_requested(v(&[])));
    }

    #[test]
    fn unrelated_args_are_ignored() {
        assert!(!autostart_requested(v(&["app.exe", "--verbose", "autostart-service"])));
        assert!(autostart_requested(v(&["app.exe", "--verbose", "--autostart-service"])));
    }

    #[test]
    fn case_and_equals_forms_are_rejected() {
        assert!(!autostart_requested(v(&["app.exe", "--AUTOSTART-SERVICE"])));
        assert!(!autostart_requested(v(&["app.exe", "--autostart-service=1"])));
        assert!(!autostart_requested(v(&["app.exe", "--autostart-service=true"])));
    }

    #[test]
    fn executable_path_is_not_treated_as_flag() {
        assert!(!autostart_requested(v(&["--autostart-service"])));
    }
}
