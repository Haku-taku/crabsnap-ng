//! One frame from each capture rung, so a rung that answers with nothing says so.
//!
//! The mean luma is the check: a blank or uninitialised frame reads 0, and the rungs must
//! agree on the desktop they photographed. Each frame is compared in the rendition a file
//! gets, so the rungs agree whatever packing the desktop handed over, and the native storage
//! is printed beside it.

use std::time::{Duration, Instant};

use crabsnap_core::capture::{Frame, Image, PixelFormat};
use crabsnap_platform_win32::capture;
use ui_core::colour::ContentColour;
use ui_core::geom::Rect;

fn main() -> Result<(), String> {
    // A grab borrows nothing on Windows; the session exists so both platforms open alike.
    let session = capture::Session;
    let mut frames: Vec<(&'static str, Frame)> = Vec::new();
    for rung in crabsnap_platform_win32::CAPTURE_BACKENDS {
        if *rung == "auto" {
            continue;
        }
        let Some(mut capture) = capture::open(&session, Some(rung), 0)? else {
            println!("{rung}: unavailable");
            continue;
        };
        let started = Instant::now();
        match capture.grab(Duration::from_secs(3)) {
            Ok(frame) => {
                let storage = frame.colour.storage;
                let output = frame.output.clone();
                let mut shot = rendition(&frame.into_image());
                shot.output = output;
                println!(
                    "{}: {}x{} stride {} scale {} output {} mean {:.1} storage {:?} after {:?}",
                    capture.name(),
                    shot.width,
                    shot.height,
                    shot.stride,
                    shot.scale,
                    shot.output.as_deref().unwrap_or("none"),
                    mean_luma(&shot),
                    storage,
                    started.elapsed()
                );
                let opaque = shot.pixels.as_chunks::<4>().0.iter().all(|t| t[3] == 0xff);
                println!("{}: opaque {opaque}", capture.name());
                frames.push((capture.name(), shot));
            }
            Err(error) => println!("{rung}: {error} after {:?}", started.elapsed()),
        }
    }
    compare(&frames);

    // The automatic chain must bind the first rung that answers.
    let mut automatic = capture::open(&session, None, 0)?
        .ok_or_else(|| "the automatic chain bound nothing".to_string())?;
    let frame = automatic.grab(Duration::from_secs(3))?;
    println!(
        "auto: {} at {}x{}",
        automatic.name(),
        frame.width,
        frame.height
    );
    Ok(())
}

/// A picture as the SDR file this tool writes, from whatever packing the desktop handed over.
fn rendition(image: &Image) -> Frame {
    let whole = Rect::new(0.0, 0.0, image.width as f32, image.height as f32);
    let pixels = image.crop(whole).rendition().into_owned();
    Frame {
        width: image.width,
        height: image.height,
        stride: image.width * 4,
        pixels,
        format: PixelFormat::Rgba8,
        colour: ContentColour::SDR,
        output: None,
        scale: image.scale,
    }
}

/// How far apart two rungs' pictures of the same desktop are, in 8-bit codes.
fn compare(frames: &[(&'static str, Frame)]) {
    for pair in frames.windows(2) {
        let (a_name, a) = &pair[0];
        let (b_name, b) = &pair[1];
        if (a.width, a.height) != (b.width, b.height) {
            println!("{a_name} and {b_name} disagree on the size");
            continue;
        }
        let mut total = 0u64;
        let mut worst = 0i32;
        let mut counted = 0u64;
        for (texel_a, texel_b) in a
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(b.pixels.as_chunks::<4>().0.iter())
            .step_by(97)
        {
            for channel in 0..3 {
                let difference = (texel_a[channel] as i32 - texel_b[channel] as i32).abs();
                total += difference as u64;
                worst = worst.max(difference);
                counted += 1;
            }
        }
        println!(
            "{a_name} vs {b_name}: mean {:.2} codes, worst {worst}",
            total as f64 / counted as f64
        );
    }
}

fn mean_luma(frame: &Frame) -> f64 {
    if frame.pixels.is_empty() {
        return -1.0;
    }
    let total: f64 = frame
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|texel| (texel[0] as f64 + texel[1] as f64 + texel[2] as f64) / 3.0)
        .sum();
    total / (frame.pixels.len() / 4) as f64
}
