// Windows VERSIONINFO resource for the sidecar executables. Each sidecar's build.rs includes this
// file (Windows target only). The publisher and copyright strings are release metadata: keep them
// in sync with LICENSE and THIRD_PARTY_NOTICES.md.

use std::{env, fs, path::PathBuf};

/// Embeds a VERSIONINFO resource into the executable that is being built.
pub fn embed(original_filename: &str, file_description: &str) {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let var = |name: &str| env::var(name).unwrap_or_else(|_| panic!("{name} is not set"));
    let version = var("CARGO_PKG_VERSION");
    let numeric = format!(
        "{},{},{},0",
        var("CARGO_PKG_VERSION_MAJOR"),
        var("CARGO_PKG_VERSION_MINOR"),
        var("CARGO_PKG_VERSION_PATCH")
    );
    let internal_name = original_filename.trim_end_matches(".exe");
    // The code page pragma makes rc.exe read this file as UTF-8, so the copyright sign is stored as U+00A9.
    let rc = format!(
        r#"#pragma code_page(65001)
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "Plynte Labs / FranGuh"
            VALUE "FileDescription", "{file_description}"
            VALUE "FileVersion", "{version}"
            VALUE "InternalName", "{internal_name}"
            VALUE "LegalCopyright", "Copyright © 2026 Plynte Labs / FranGuh / LiveAudio Contributors"
            VALUE "OriginalFilename", "{original_filename}"
            VALUE "ProductName", "LiveAudio"
            VALUE "ProductVersion", "{version}"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
"#
    );
    let rc_path = PathBuf::from(var("OUT_DIR")).join(format!("{internal_name}-version.rc"));
    fs::write(&rc_path, rc).expect("write version resource");
    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_required()
        .expect("compile Windows version resource");
}
