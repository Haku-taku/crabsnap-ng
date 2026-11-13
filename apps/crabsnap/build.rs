//! The executable's Windows resources: its icon, its version block and its manifest.
//!
//! The version block is generated here rather than kept in a file, so the number Explorer
//! shows cannot drift from the one `crabsnap --version` prints. Nothing is embedded on a
//! non-Windows target, where a resource script has no consumer.

use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=assets/crabsnap.ico");
    println!("cargo:rerun-if-changed=assets/crabsnap.manifest");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return Ok(());
    }
    let script = PathBuf::from(std::env::var("OUT_DIR")?).join("crabsnap.rc");
    let version = std::env::var("CARGO_PKG_VERSION")?;
    std::fs::write(&script, resource_script(&version)?)?;
    embed_resource::compile(&script, embed_resource::NONE).manifest_required()?;
    Ok(())
}

/// The resource script `rc.exe` compiles: the icon, the manifest and the version block.
///
/// Both files are named absolutely, because the compiler runs outside the package directory.
fn resource_script(version: &str) -> Result<String, Box<dyn std::error::Error>> {
    let icon = std::path::absolute("assets/crabsnap.ico")?;
    let manifest = std::path::absolute("assets/crabsnap.manifest")?;
    let file = version_numbers(version);
    Ok(format!(
        r#"1 ICON "{}"
1 24 "{}"

1 VERSIONINFO
FILEVERSION {a},{b},{c},{d}
PRODUCTVERSION {a},{b},{c},{d}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Maritedo"
      VALUE "FileDescription", "Screenshot and pin tool"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "crabsnap"
      VALUE "OriginalFilename", "crabsnap.exe"
      VALUE "ProductName", "crabsnap"
      VALUE "ProductVersion", "{version}"
      VALUE "LegalCopyright", "Copyright (c) Maritedo. MIT OR Apache-2.0."
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        path(icon.as_path()),
        path(manifest.as_path()),
        a = file[0],
        b = file[1],
        c = file[2],
        d = file[3],
    ))
}

/// The four numbers a version block counts with; a pre-release suffix names no number.
fn version_numbers(version: &str) -> [u16; 4] {
    let mut numbers = [0u16; 4];
    for (slot, part) in numbers.iter_mut().zip(version.split(['.', '-'])) {
        let digits: String = part.chars().take_while(|character| character.is_ascii_digit()).collect();
        *slot = digits.parse().unwrap_or(0);
    }
    numbers
}

/// A path as a resource script carries it: forward slashes, so no backslash is read as an
/// escape and the file name needs no quoting rules of its own.
fn path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
