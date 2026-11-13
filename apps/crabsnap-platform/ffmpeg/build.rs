//! Compiles the C shim against the headers this machine has.
//!
//! With headers, `ffi/ffmpeg_shim.c` is built and the app encodes through the libraries
//! in-process. Without them, `ffi/ffmpeg_shim_stub.c` takes its place and every call reports
//! "unavailable"; the app then uses the ffmpeg program. Both define the same symbols, so the
//! Rust side is the same either way and no compile-time switch selects a backend.
//!
//! The shim never links the libraries — it loads them at runtime — so the headers alone are
//! enough. Where they are is the platform's question: Unix asks pkg-config, and Windows has
//! no such registry, so `CRABSNAP_FFMPEG_INCLUDE` names the directory holding `libavcodec/`.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    for file in [
        "ffi/ffmpeg_shim.c",
        "ffi/ffmpeg_shim.h",
        "ffi/ffmpeg_shim_stub.c",
    ] {
        println!("cargo:rerun-if-changed={file}");
    }
    println!("cargo:rerun-if-env-changed=CRABSNAP_NO_FFMPEG_SHIM");
    println!("cargo:rerun-if-env-changed=CRABSNAP_FFMPEG_INCLUDE");

    let forced_off = std::env::var_os("CRABSNAP_NO_FFMPEG_SHIM").is_some_and(|value| value != "0");
    let includes = if forced_off { None } else { include_paths() };

    let mut build = cc::Build::new();
    build.warnings(true);
    match &includes {
        Some(paths) => {
            build.includes(paths).file("ffi/ffmpeg_shim.c");
            if build.get_compiler().is_like_msvc() {
                // Our sources are UTF-8; under a GBK console page cl.exe misreads them.
                build.flag("/utf-8");
                // The installed headers are written for gcc, and their conversions are
                // theirs: under /W4 they would drown our own.
                build.flag("/external:anglebrackets").flag("/external:W0");
            }
            if target_family() == "unix" {
                // The shim keeps its one library state behind a mutex; before glibc 2.34
                // those symbols lived in libpthread rather than libc.
                build.flag("-pthread");
                println!("cargo:rustc-link-lib=pthread");
            }
        }
        None => {
            // Windows has no pkg-config registry, so the directory is the caller's to name.
            println!(
                "cargo:warning=no ffmpeg headers: set CRABSNAP_FFMPEG_INCLUDE to the directory \
                 holding libavcodec/avcodec.h, or only PNG is encoded in-process"
            );
            build.file("ffi/ffmpeg_shim_stub.c");
        }
    }
    build.compile("ffmpeg_shim");
}

/// The platform this build is for, which a cross build makes different from the host's.
fn target_family() -> String {
    std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default()
}

/// Where the headers are, or `None` when this machine has none.
///
/// `CRABSNAP_FFMPEG_INCLUDE` is taken at its word: whoever set it wants the libraries, so a
/// directory without the header is an error rather than a silent stub.
fn include_paths() -> Option<Vec<String>> {
    if let Some(directory) = std::env::var_os("CRABSNAP_FFMPEG_INCLUDE") {
        let directory = PathBuf::from(directory);
        if !directory.join("libavcodec").join("avcodec.h").is_file() {
            panic!(
                "CRABSNAP_FFMPEG_INCLUDE names {}, which has no libavcodec/avcodec.h",
                directory.display()
            );
        }
        return Some(vec![directory.to_string_lossy().into_owned()]);
    }
    pkg_config_includes()
}

/// Where pkg-config says the ffmpeg headers are; `None` when it knows no such thing.
///
/// pkg-config answers for the libraries, which is not the same as the headers being there —
/// a runtime package without the development one still has a `.pc`. The header itself is
/// what decides, because the shim includes it.
fn pkg_config_includes() -> Option<Vec<String>> {
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
