//! Compiles the ffmpeg shim against the headers this machine has.
//!
//! With headers, `ffi/ffmpeg_shim.c` is built and the app can encode through the libraries
//! in-process. Without them, `ffi/ffmpeg_shim_stub.c` takes its place and every call reports
//! "unavailable"; the app then uses the ffmpeg program. Both define the same symbols, so the
//! Rust side is the same either way and no compile-time switch selects a backend.
//!
//! The shim never links the libraries — it dlopens them — so the headers alone are enough.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    // The crate is empty off unix: there is no module to link the shim into.
    if std::env::var_os("CARGO_CFG_UNIX").is_none() {
        return;
    }
    for file in [
        "ffi/ffmpeg_shim.c",
        "ffi/ffmpeg_shim.h",
        "ffi/ffmpeg_shim_stub.c",
    ] {
        println!("cargo:rerun-if-changed={file}");
    }
    println!("cargo:rerun-if-env-changed=CRABSNAP_NO_FFMPEG_SHIM");

    let forced_off = std::env::var_os("CRABSNAP_NO_FFMPEG_SHIM").is_some_and(|value| value != "0");
    let includes = if forced_off { None } else { include_paths() };

    let mut build = cc::Build::new();
    build.warnings(true);
    match &includes {
        Some(paths) => {
            build
                .includes(paths)
                .flag("-pthread")
                .file(PathBuf::from("ffi/ffmpeg_shim.c"));
            // The shim keeps its one library state behind a mutex; before glibc 2.34 those
            // symbols lived in libpthread rather than libc.
            println!("cargo:rustc-link-lib=pthread");
        }
        None => {
            println!(
                "cargo:warning=no ffmpeg headers: formats other than PNG will need the ffmpeg program"
            );
            build.file(PathBuf::from("ffi/ffmpeg_shim_stub.c"));
        }
    }
    build.compile("ffmpeg_shim");
}

/// Where pkg-config says the ffmpeg headers are; `None` when it knows no such thing.
///
/// pkg-config answers for the libraries, which is not the same as the headers being there —
/// a runtime package without the development one still has a `.pc`. The header itself is
/// what decides, because the shim includes it.
fn include_paths() -> Option<Vec<String>> {
    let output = Command::new("pkg-config")
        .args(["--cflags"])
        .args(["libavformat", "libavcodec", "libavutil", "libswscale"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let flags = String::from_utf8(output.stdout).ok()?;
    let paths: Vec<String> = flags
        .split_whitespace()
        .filter_map(|word| word.strip_prefix("-I"))
        .map(str::to_string)
        .collect();
    let mut candidates = paths.clone();
    // No -I at all means the compiler's own search path, which starts here.
    candidates.push("/usr/include".to_string());
    let present = candidates.iter().any(|directory| {
        PathBuf::from(directory)
            .join("libavcodec/avcodec.h")
            .is_file()
    });
    present.then_some(paths)
}
