//! Write `assets/crabsnap.ico` from the drawing the notification area shows.
//!
//! Run from `apps/`: `cargo run -p crabsnap --example icon`. The icon is committed because a
//! build script cannot draw it: the artwork is the engine's primitives, and a build script
//! links none of them. Each size is a PNG entry, which every Windows this backend targets
//! reads at any size.

#![allow(dead_code)] // the included module also carries the tray's own entry point

#[path = "../src/tray_icon.rs"]
mod tray_icon;

use image::ImageEncoder;

/// The sizes Explorer asks an application icon for, from the tree to the 256 it shows in a
/// large-icon view.
const SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];

fn main() -> Result<(), String> {
    let mut frames = Vec::with_capacity(SIZES.len());
    for size in SIZES {
        let icon = tray_icon::icon_at(size)?;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&icon.rgba, size, size, image::ExtendedColorType::Rgba8)
            .map_err(|error| format!("{size}x{size}: {error}"))?;
        frames.push(
            image::codecs::ico::IcoFrame::with_encoded(
                png,
                size,
                size,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|error| format!("{size}x{size}: {error}"))?,
        );
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/crabsnap.ico");
    let file =
        std::fs::File::create(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    image::codecs::ico::IcoEncoder::new(std::io::BufWriter::new(file))
        .encode_images(&frames)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    println!("wrote {} at {SIZES:?}", path.display());
    Ok(())
}
