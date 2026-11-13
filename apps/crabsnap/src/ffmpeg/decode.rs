//! Reading a picture through the same ffmpeg authority that writes one.
//!
//! A pin draws whatever this machine's encoder can write, so it has to read that same set.
//! Asking ffmpeg keeps one answer to "what can this machine show", and a file that says it
//! is 10-bit or PQ arrives with that description instead of being flattened to sRGB.

use std::sync::Arc;

use crabsnap_platform::shim;
use ui_core::colour::{Chromaticities, ContentColour, PixelStorage, TransferFunction};

use super::{Ffmpeg, Source};

/// One decoded picture: straight-alpha RGBA8, or 2-10-10-10 words, as `colour` says.
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub data: Arc<Vec<u8>>,
    pub colour: ContentColour,
}

impl Ffmpeg {
    /// Read the first picture of `bytes`; an animated file gives the frame it starts on.
    ///
    /// The bytes are what the caller already read to classify the file, so a large picture
    /// is opened without a second copy of itself.
    pub fn decode(&self, bytes: &[u8]) -> Result<Decoded, String> {
        match &self.source {
            Source::Libraries(_) => {
                let decoded = shim::decode(bytes)?;
                Ok(Decoded {
                    width: decoded.width as u32,
                    height: decoded.height as u32,
                    data: Arc::new(decoded.data),
                    colour: content_colour(decoded.colour, decoded.ten_bit),
                })
            }
            Source::Program(program) => {
                // The program answers in PNG, so it can only hand back 8-bit sRGB.
                let png = program.decode(bytes)?;
                let (width, height, data) = crate::capture::read_png(&png)?;
                Ok(Decoded {
                    width,
                    height,
                    data: Arc::new(data),
                    colour: ContentColour::SDR,
                })
            }
            // PNG is the one format this build writes itself, so its own reader is what it can
            // always go back to; it is 8-bit sRGB, which is what this build writes.
            Source::None => {
                let (width, height, data) = crate::capture::read_png(bytes).map_err(|error| {
                    format!(
                        "cannot read this picture: {error}; {}",
                        self.failure()
                            .unwrap_or_else(|| "no ffmpeg libraries and no ffmpeg program".into())
                    )
                })?;
                Ok(Decoded {
                    width,
                    height,
                    data: Arc::new(data),
                    colour: ContentColour::SDR,
                })
            }
        }
    }
}

/// AVColorPrimaries to the coordinates the renderer converts from; anything unnamed is read
/// as sRGB, which is what an untagged file means.
fn primaries(code: i32) -> Chromaticities {
    match code {
        9 => Chromaticities::BT2020,      // AVCOL_PRI_BT2020
        12 => Chromaticities::DISPLAY_P3, // AVCOL_PRI_SMPTE432
        _ => Chromaticities::SRGB,
    }
}

/// AVColorTransferCharacteristic to the curve the codes are in.
fn transfer(code: i32) -> TransferFunction {
    match code {
        4 => TransferFunction::Gamma(2.2), // AVCOL_TRC_GAMMA22
        5 => TransferFunction::Gamma(2.8), // AVCOL_TRC_GAMMA28
        8 => TransferFunction::Linear,     // AVCOL_TRC_LINEAR
        16 => TransferFunction::Pq,        // AVCOL_TRC_SMPTE2084
        18 => TransferFunction::Hlg,       // AVCOL_TRC_ARIB_STD_B67
        // sRGB and the BT.709 family (BT.709, BT.2020 10/12-bit, SMPTE 170M) are close
        // enough that keeping their codes as they are costs less than a matrix round trip.
        _ => TransferFunction::Srgb,
    }
}

fn content_colour(colour: shim::Colour, ten_bit: bool) -> ContentColour {
    ContentColour {
        storage: if ten_bit {
            PixelStorage::Packed10
        } else {
            PixelStorage::Sdr8
        },
        primaries: primaries(colour.primaries),
        transfer: transfer(colour.transfer),
        // A stream can state its peak; none of the ones a pin reads do, so the renderer
        // scales by the display's own reference instead of a number we made up.
        max_nits: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ISO-BMFF keeps its index after the picture, so a memory source has to seek to it and
    /// back. Without a seek callback the demuxer calls the bytes partial and hands over no
    /// frame, which is how a real 2560×1440 AVIF failed to pin. The 2×2 picture is also
    /// narrower than one SIMD block, so this decode covers the destination's tail padding.
    #[test]
    fn a_picture_whose_index_sits_at_the_end_decodes_from_memory() {
        let ffmpeg = Ffmpeg::probe(None, None);
        let Some(format) = ffmpeg.format("avif").cloned() else {
            // Only a machine with no AVIF writer gets here, and such a machine cannot build
            // this layout either; every other memory decode still runs the same reader.
            return;
        };
        let image = crate::capture::Image {
            width: 2,
            height: 2,
            data: Arc::new([253u8, 0, 0, 255].repeat(4)),
            colour: ContentColour::SDR,
            scale: 1.0,
        };
        let picture = image.crop(ui_core::geom::Rect::new(0.0, 0.0, 2.0, 2.0));
        let avif = ffmpeg.encode(&format, &picture, &[]).expect("an AVIF");
        let decoded = ffmpeg
            .decode(&index_last(&avif))
            .expect("an AVIF decodes from memory");
        assert_eq!((decoded.width, decoded.height), (2, 2));
        assert_eq!(decoded.colour.storage, PixelStorage::Sdr8);
        assert_eq!(decoded.data.len(), 2 * 2 * 4);
        // AV1 is lossy, so the exact code drifts; what matters is that the pixels came back
        // red and opaque rather than zeroed or byte-swapped.
        let texel = &decoded.data[..4];
        assert!(
            texel[0] > 200 && texel[1] < 80 && texel[2] < 80 && texel[3] == 255,
            "a red texel came back as {texel:?}"
        );
    }

    /// The layout a tool that writes its index last produces: `ftyp`, a gap keeping `mdat`
    /// where the index says it is, `mdat`, padding, `meta`. The padding is what pushes the
    /// picture out of the demuxer's read buffer, so the seek back to it is a real one.
    fn index_last(avif: &[u8]) -> Vec<u8> {
        let mut boxes: Vec<(&[u8; 4], &[u8])> = Vec::new();
        let mut at = 0;
        while at < avif.len() {
            let size =
                u32::from_be_bytes(avif[at..at + 4].try_into().expect("four size bytes")) as usize;
            assert!(
                size >= 8 && at + size <= avif.len(),
                "a box this test can read"
            );
            let kind = avif[at + 4..at + 8].try_into().expect("four kind bytes");
            boxes.push((kind, &avif[at..at + size]));
            at += size;
        }
        let boxed = |kind: &[u8; 4]| {
            boxes
                .iter()
                .find(|(name, _)| *name == kind)
                .map(|(_, body)| *body)
                .unwrap_or_else(|| panic!("the muxer writes a {:?} box", kind))
        };
        let (ftyp, meta, mdat) = (boxed(b"ftyp"), boxed(b"meta"), boxed(b"mdat"));
        let mut out = Vec::new();
        out.extend_from_slice(ftyp);
        out.extend_from_slice(&free_box(meta.len()));
        out.extend_from_slice(mdat);
        out.extend_from_slice(&free_box(128 << 10));
        out.extend_from_slice(meta);
        out
    }

    /// A `free` box of `size` bytes, which every ISO-BMFF reader skips.
    fn free_box(size: usize) -> Vec<u8> {
        let mut padding = Vec::with_capacity(size);
        padding.extend_from_slice(&(size as u32).to_be_bytes());
        padding.extend_from_slice(b"free");
        padding.resize(size, 0);
        padding
    }

    /// A tag is what makes an HDR pin draw right: reading PQ as sRGB would show it washed
    /// out, and packing it as Sdr8 would cut every code to the top eight bits.
    #[test]
    fn ffmpeg_colour_tags_become_the_description_the_renderer_reads() {
        let pq = shim::Colour {
            primaries: 9,
            transfer: 16,
            space: 9,
            range: 2,
        };
        let colour = content_colour(pq, true);
        assert_eq!(colour.primaries, Chromaticities::BT2020);
        assert_eq!(colour.transfer, TransferFunction::Pq);
        assert_eq!(colour.storage, PixelStorage::Packed10);
        assert_eq!(colour.max_nits, 0.0, "no stream stated a peak");

        let untagged = shim::Colour {
            primaries: 2,
            transfer: 2,
            space: 2,
            range: 0,
        };
        assert_eq!(
            content_colour(untagged, false),
            ContentColour::SDR,
            "an untagged 8-bit picture is what every other picture here is"
        );

        let p3 = shim::Colour {
            primaries: 12,
            transfer: 13,
            ..shim::Colour::default()
        };
        let colour = content_colour(p3, false);
        assert_eq!(colour.primaries, Chromaticities::DISPLAY_P3);
        assert_eq!(colour.transfer, TransferFunction::Srgb);

        let gamma = shim::Colour {
            transfer: 4,
            ..shim::Colour::default()
        };
        assert_eq!(
            content_colour(gamma, false).transfer,
            TransferFunction::Gamma(2.2)
        );
    }
}
