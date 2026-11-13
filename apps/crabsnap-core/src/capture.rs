//! What a capture hands back, and the shape a platform's rungs answer in.
//!
//! A frame keeps the bytes the platform wrote and its own description of what they mean:
//! converting at ingest would destroy an HDR frame for every later use, including a surface
//! that could have shown it. The packings differ per platform, so they are named by their
//! byte order rather than by any one protocol's spelling.

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use ui_core::colour::{
    Chromaticities, ContentColour, Destination, Domain, PixelStorage, Placement, SCRGB_WHITE_NITS,
    SDR_WHITE_NITS, ToneMapping, TransferFunction, WhiteAdaptation, f32_to_half,
    gamut_matrix_adapted, half_to_f32,
};
use ui_core::geom::Rect;

/// How much light a picture holds, and the white that says what counts as more than SDR.
///
/// Absolute light — scRGB floats or PQ codes — says nothing about *how much* of it there is
/// until the pixels are read, so whether a picture holds anything above SDR is measured
/// rather than claimed by its description. The measurement is shared through an `Arc` so a
/// worker and the pin that queued it see one value, and the pixels are only ever read.
#[derive(Clone, Default)]
pub struct Light {
    /// The display's SDR white in nits; zero when the picture came from an SDR surface.
    reference_nits: f32,
    /// The brightest channel measured so far, in nits.
    peak_nits: Arc<OnceLock<f32>>,
}

impl Light {
    /// A picture that carries no light above the desktop's own white.
    pub fn sdr() -> Self {
        Self::default()
    }

    /// A picture from a display whose white is `reference_nits`.
    pub fn of(reference_nits: f32) -> Self {
        Self {
            reference_nits,
            peak_nits: Arc::default(),
        }
    }

    /// The display's SDR white, in nits; zero for an SDR picture.
    pub fn reference_nits(&self) -> f32 {
        self.reference_nits
    }

    /// The measured peak, once someone has measured it.
    pub fn peak_nits(&self) -> Option<f32> {
        self.peak_nits.get().copied()
    }

    /// Whether the picture holds light above the display's white. Unmeasured reads as no:
    /// the badge waits for the worker rather than stalling the frame that showed the pin.
    /// Without a white to measure against, no measurement can make the claim.
    pub fn is_hdr(&self) -> bool {
        self.reference_nits > 0.0
            && self
                .peak_nits
                .get()
                .is_some_and(|peak| *peak > self.reference_nits * 1.01)
    }

    /// Record a measurement; the first answer stands, so a worker and an export agree.
    pub fn measured(&self, peak_nits: f32) {
        let _ = self.peak_nits.set(peak_nits);
    }
}

/// How a frame's 32-bit words are packed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// Red, green, blue, alpha: this engine's own order, which the Windows rungs produce.
    Rgba8,
    /// Blue, green, red, alpha: wl_shm's `argb8888`, DXGI's `B8G8R8A8_UNORM`.
    Bgra8,
    /// The same order with an undefined alpha byte, which a screen capture reads as opaque.
    Bgrx8,
    /// 10 bits per channel, blue in the low bits: wl_shm's `xbgr2101010`.
    Bgr10,
    /// Four half floats, red first: a desktop compositing in floats, whose light may exceed
    /// the reference white.
    Rgba16F,
}

impl PixelFormat {
    /// What the pixels mean before any description is applied.
    pub fn storage(self) -> PixelStorage {
        match self {
            PixelFormat::Rgba8 | PixelFormat::Bgra8 | PixelFormat::Bgrx8 => PixelStorage::Sdr8,
            PixelFormat::Bgr10 => PixelStorage::Packed10,
            PixelFormat::Rgba16F => PixelStorage::Float16,
        }
    }
}

/// One captured frame, top-down rows in the platform's own packing.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Bytes between rows; always tight (`width * 4`) because the frame owns its buffer.
    pub stride: u32,
    /// Native pixels: the bytes the platform wrote, in `colour.storage`'s packing.
    pub pixels: Vec<u8>,
    pub format: PixelFormat,
    /// What `pixels` mean. Carried, never applied.
    pub colour: ContentColour,
    /// The output this frame came from; `None` when the platform does not name one (a
    /// portal captures whatever the desktop shows).
    pub output: Option<String>,
    /// The output's device pixels per logical point: what a saved file's density says, and
    /// nothing about how the pixels are packed.
    pub scale: f32,
    /// The display's SDR white in nits; zero when the platform has no HDR to report.
    pub reference_nits: f32,
}

impl Frame {
    /// The frame as a picture: the platform's channel order, its own packing, its own
    /// description.
    pub fn into_image(self) -> Image {
        let mut data = self.pixels;
        match self.format {
            // Both come in the order the engine wants and are carried untouched: a float
            // frame's whole point is that its light is not reduced to eight-bit codes.
            PixelFormat::Rgba8 | PixelFormat::Rgba16F => {}
            PixelFormat::Bgra8 | PixelFormat::Bgrx8 => {
                let opaque = self.format == PixelFormat::Bgrx8;
                for pixel in data.as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2);
                    // `X` means the alpha byte is undefined, not "transparent".
                    if opaque {
                        pixel[3] = 0xff;
                    }
                }
            }
            // 10-bit words keep their layout; only the alpha bits are made opaque. A screen
            // capture has no transparency, and the two bits are 0 in practice, which would
            // make the whole frame invisible.
            PixelFormat::Bgr10 => {
                for word in data.as_chunks_mut::<4>().0 {
                    word[3] |= 0xc0;
                }
            }
        }
        Image {
            width: self.width,
            height: self.height,
            data: Arc::new(data),
            colour: self.colour,
            scale: self.scale,
            light: Light::of(self.reference_nits),
        }
    }
}

/// A picture the engine can draw: straight alpha, top-down rows, packed as `colour` says.
///
/// One `Arc` serves both the texture upload and every later crop, so marking up a capture
/// never copies the screen.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub data: Arc<Vec<u8>>,
    /// What the pixels mean; a renderer converts for the display it draws on.
    pub colour: ContentColour,
    /// Device pixels per logical point where the picture was taken. It says how large the
    /// picture was meant to be, not how to draw it: layout uses the surface's own scale.
    pub scale: f32,
    /// How much light the picture holds, and the white it came with.
    pub light: Light,
}

impl Image {
    /// The device-pixel rectangle `rect` covers, rounded outwards, as `(x0, y0, x1, y1)`.
    ///
    /// `rect` is in logical points at `scale` device pixels each. This is the one rule that
    /// turns a selection into pixels: [`Image::crop`] takes its pixels from it, and a surface
    /// that draws marks over the selection builds its `Space` from it, so the two cannot
    /// disagree about where the first pixel is.
    pub fn crop_of(&self, rect: Rect, scale: f32) -> (u32, u32, u32, u32) {
        // Layout arithmetic is f32: an edge meant for 864 arrives as 864.00006, and a bare
        // `ceil` would add a row nobody selected. Snap to a thousandth of a pixel first.
        let snap = |value: f32| (value * 1000.0).round() / 1000.0;
        let x0 = (snap(rect.min_x() * scale).floor().max(0.0) as u32).min(self.width);
        let y0 = (snap(rect.min_y() * scale).floor().max(0.0) as u32).min(self.height);
        let x1 = (snap(rect.max_x() * scale).ceil().max(0.0) as u32).clamp(x0, self.width);
        let y1 = (snap(rect.max_y() * scale).ceil().max(0.0) as u32).clamp(y0, self.height);
        (x0, y0, x1, y1)
    }

    /// The pixels under `rect`, rounded outwards so nothing the rectangle touches is lost.
    ///
    /// `rect` is in the picture's own pixels and may be fractional. The crop is a smaller
    /// copy of the same picture, not a rendition: the packing, the description and the scale
    /// travel with it.
    pub fn crop(&self, rect: Rect) -> Cropped {
        // `rect` is already in device pixels, so the grid is that of a scale of one.
        let (x0, y0, x1, y1) = self.crop_of(rect, 1.0);
        let (width, height) = (x1 - x0, y1 - y0);
        // A whole-frame selection is the frame: a pin of the entire screen does not need a
        // second 33 MB copy of pixels that are never written again.
        if (x0, y0, width, height) == (0, 0, self.width, self.height) {
            return Cropped {
                origin: (0, 0),
                width,
                height,
                data: Arc::clone(&self.data),
                colour: self.colour,
                scale: self.scale,
                // The same pixels, so a measurement of either answers for both.
                light: self.light.clone(),
            };
        }
        // A crop is a smaller copy of the same picture, not a rendition: the packing and the
        // description travel with it, so a pin of an HDR selection stays HDR.
        let texel = self.colour.storage.bytes_per_texel();
        let mut data = Vec::with_capacity((width * height) as usize * texel);
        for row in y0..y1 {
            let start = ((row * self.width + x0) as usize) * texel;
            let end = start + (width as usize) * texel;
            data.extend_from_slice(&self.data[start..end]);
        }
        Cropped {
            origin: (x0, y0),
            width,
            height,
            // Shared, not copied on: a pin's texture is this same buffer, and the crop is
            // never written after it is taken.
            data: Arc::new(data),
            colour: self.colour,
            scale: self.scale,
            // Different pixels, so the picture's own peak is not this one's.
            light: Light::of(self.light.reference_nits()),
        }
    }
}

/// The rendition a file gets: sRGB 8-bit, with the light above SDR white rolled off.
///
/// BT.2390 keeps everything below its knee untouched and brings the rest down into the
/// destination, so a highlight survives as a gradient instead of the flat white clipping
/// leaves. A picture whose peak is not above the file's white is unchanged by it.
const SDR_FILE_DESTINATION: Destination = Destination {
    domain: Domain::Encoded,
    tone: ToneMapping::Bt2390,
};

/// The brightest channel in a picture, in nits; `None` when its codes say nothing absolute.
///
/// A float row is scRGB half floats, where 1.0 is 80 nits; a PQ row decodes to a fraction of
/// 10000. Alpha is skipped because it says nothing about light.
pub fn brightest_nits(pixels: &[u8], colour: ContentColour) -> Option<f32> {
    let brightest = match colour.storage {
        PixelStorage::Float16 => {
            pixels
                .as_chunks::<8>()
                .0
                .iter()
                .map(|texel| {
                    (0..3)
                        .map(|channel| {
                            let low = texel[channel * 2];
                            let high = texel[channel * 2 + 1];
                            half_to_f32(u16::from_le_bytes([low, high]))
                        })
                        .fold(0.0f32, f32::max)
                })
                .fold(0.0f32, f32::max)
                * SCRGB_WHITE_NITS
        }
        _ if colour.transfer == TransferFunction::Pq => {
            pixels
                .chunks_exact(colour.storage.bytes_per_texel())
                .map(|texel| {
                    let (codes, _) = colour.storage.unpack(texel);
                    (0..3)
                        .map(|channel| colour.transfer.decode(codes[channel]))
                        .fold(0.0f32, f32::max)
                })
                .fold(0.0f32, f32::max)
                * 10_000.0
        }
        _ => return None,
    };
    Some(brightest)
}

/// A region of an [`Image`], in the picture's own packing.
pub struct Cropped {
    /// Where the region starts in the source's pixel grid: the corner the rectangle was
    /// rounded to, which is where these pixels actually came from.
    pub origin: (u32, u32),
    pub width: u32,
    pub height: u32,
    pub data: Arc<Vec<u8>>,
    pub colour: ContentColour,
    /// What the picture's points were worth when it was cropped, for the file's density.
    pub scale: f32,
    /// How much light the region holds; measured away from the capture, never on it.
    pub light: Light,
}

impl Cropped {
    /// Measure the brightest channel in the picture and record it, in nits.
    ///
    /// One pass over the pixels, so a pin queues this on the job worker rather than running
    /// it where the capture lands. Codes relative to a display's white have nothing to
    /// measure, and answer `None`.
    pub fn measure_peak(&self) -> Option<f32> {
        if let Some(peak) = self.light.peak_nits() {
            return Some(peak);
        }
        let nits = brightest_nits(&self.data, self.colour)?;
        self.light.measured(nits);
        Some(nits)
    }

    /// The transform a file written from this region gets.
    ///
    /// Tone mapping is bounded by the picture's own peak once that has been measured; one
    /// nobody measured is measured here, since a file is where the answer is needed. A pin
    /// measures on the job worker when it appears, so this normally finds it ready.
    fn file_placement(&self) -> Placement {
        let peak = self.measure_peak();
        let colour = match peak {
            Some(peak) => self.colour.with_peak(peak),
            None => self.colour,
        };
        colour.placement(SDR_FILE_DESTINATION)
    }

    /// The region as sRGB 8-bit RGBA: what every file format this tool writes takes.
    ///
    /// Borrowed when the picture is already SDR, so the common case copies nothing.
    pub fn rendition(&self) -> std::borrow::Cow<'_, [u8]> {
        let placement = self.file_placement();
        if placement.is_identity() {
            return std::borrow::Cow::Borrowed(self.data.as_slice());
        }
        let placement = placement.tabulated();
        let source = self.colour.storage.bytes_per_texel();
        let mut rgba = Vec::with_capacity(self.data.len() / source * 4);
        let convert = |bytes: &[u8], rgba: &mut Vec<u8>| {
            let [r, g, b, alpha] = placement.texel(bytes);
            let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            rgba.extend_from_slice(&[byte(r), byte(g), byte(b), byte(alpha)]);
        };
        // A float picture is the one case a file sees with its light still above white; the
        // placement rolls it into the SDR white, as every file this tool writes is SDR.
        if source == 8 {
            for texel in self.data.as_chunks::<8>().0 {
                convert(texel, &mut rgba);
            }
        } else {
            for texel in self.data.as_chunks::<4>().0 {
                convert(texel, &mut rgba);
            }
        }
        std::borrow::Cow::Owned(rgba)
    }

    /// The region in the SDR rendition a file gets, as a picture marks can be drawn on.
    ///
    /// Every file this tool writes is sRGB 8-bit whatever the capture was (`rendition`), so a
    /// mark composites on that rendition; refusing instead would drop it from the file.
    pub fn sdr(&self) -> Cropped {
        Cropped {
            origin: self.origin,
            width: self.width,
            height: self.height,
            data: Arc::new(self.rendition().into_owned()),
            colour: ContentColour::SDR,
            scale: self.scale,
            light: Light::sdr(),
        }
    }

    /// One texel as straight sRGB bytes, converted like `rendition`; `None` outside the
    /// region. What an eyedropper reads: a press converts the one texel, not the picture.
    pub fn texel(&self, x: usize, y: usize) -> Option<[u8; 4]> {
        if x >= self.width as usize || y >= self.height as usize {
            return None;
        }
        let texel = self.colour.storage.bytes_per_texel();
        let index = (y * self.width as usize + x) * texel;
        let bytes = self.data.get(index..index + texel)?;
        let [r, g, b, alpha] = self.file_placement().texel(bytes);
        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        Some([byte(r), byte(g), byte(b), byte(alpha)])
    }
}

/// What an HDR file from this tool holds, as H.273 code points: BT.2020 primaries in PQ.
const HDR_FILE_PRIMARIES: u8 = 9;
const HDR_FILE_TRANSFER: u8 = 16;

impl Cropped {
    /// Whether a file should keep the light instead of rolling it into SDR white.
    ///
    /// The ruler is the display's own white, as for the badge: a desktop screenshot whose
    /// brightest pixel is that white is an SDR picture, and only light above it needs PQ.
    pub fn wants_hdr_file(&self) -> bool {
        if !self.colour.holds_absolute_light() {
            return false;
        }
        // Measuring is what fills the light in; the value itself is `is_hdr`'s question.
        self.measure_peak();
        self.light.is_hdr()
    }

    /// The region as a 16-bit PNG that keeps the light: BT.2020 primaries, PQ codes.
    ///
    /// PQ is absolute, so nothing is tone mapped: a highlight the screen showed at 1000 nits
    /// is 1000 nits in the file. The cICP chunk says how to read the codes, and cLLI carries
    /// the measured peak for a reader that wants it before decoding.
    pub fn hdr_png(&self) -> Result<Vec<u8>, String> {
        let samples = self.pq_samples();
        let per_metre = pixels_per_metre(density(self.scale));
        let mut info = png::Info::default();
        info.width = self.width;
        info.height = self.height;
        info.bit_depth = png::BitDepth::Sixteen;
        info.color_type = png::ColorType::Rgba;
        // pHYs is the density field: the size the screen gave the picture, as for SDR.
        info.pixel_dims = Some(png::PixelDimensions {
            xppu: per_metre,
            yppu: per_metre,
            unit: png::Unit::Meter,
        });
        let mut bytes = Vec::new();
        let encoder = png::Encoder::with_info(&mut bytes, info)
            .map_err(|error| format!("encode PNG: {error}"))?;
        let mut writer = encoder
            .write_header()
            .map_err(|error| format!("encode PNG: {error}"))?;
        // The crate parses these two on the way in but writes neither on the way out, so
        // they are written here; both must precede IDAT.
        let code_points = [HDR_FILE_PRIMARIES, HDR_FILE_TRANSFER, 0, 1];
        writer
            .write_chunk(png::chunk::cICP, &code_points)
            .map_err(|error| format!("encode PNG: {error}"))?;
        // cLLI states the peak in units of 0.0001 cd/m²; zero means "not stated".
        if let Some(peak) = self.measure_peak() {
            let mut light_level = [0u8; 8];
            let quantized = (peak * 10_000.0).round() as u32;
            light_level[..4].copy_from_slice(&quantized.to_be_bytes());
            writer
                .write_chunk(png::chunk::cLLI, &light_level)
                .map_err(|error| format!("encode PNG: {error}"))?;
        }
        writer
            .write_image_data(&samples)
            .map_err(|error| format!("encode PNG: {error}"))?;
        drop(writer);
        Ok(bytes)
    }

    /// The picture as big-endian BT.2020 PQ samples, four 16-bit channels per texel.
    fn pq_samples(&self) -> Vec<u8> {
        let to_2020 = if self.colour.primaries == Chromaticities::BT2020 {
            None
        } else {
            Some(gamut_matrix_adapted(
                self.colour.primaries,
                Chromaticities::BT2020,
                WhiteAdaptation::Bradford,
            ))
        };
        // What one unit of the picture is worth in nits: a float texel is already scRGB's
        // light, PQ codes decode to a fraction of 10000, and relative codes mean the desktop
        // white the file would otherwise have used.
        let scale = if self.colour.storage == PixelStorage::Float16 {
            SCRGB_WHITE_NITS
        } else if self.colour.transfer == TransferFunction::Pq {
            10_000.0
        } else {
            SDR_WHITE_NITS
        };
        let texel = self.colour.storage.bytes_per_texel();
        let mut samples = Vec::with_capacity((self.width as usize) * (self.height as usize) * 8);
        for bytes in self.data.chunks_exact(texel) {
            let (values, alpha) = self.colour.storage.unpack(bytes);
            let linear = if self.colour.storage == PixelStorage::Float16 {
                values
            } else {
                values.map(|value| self.colour.transfer.decode(value))
            };
            let nits = linear.map(|value| value * scale);
            let [red, green, blue] = to_2020.map_or(nits, |matrix| multiply(matrix, nits));
            for channel in [red, green, blue] {
                samples.extend_from_slice(&quantize(channel).to_be_bytes());
            }
            let alpha = (alpha.clamp(0.0, 1.0) * 65535.0).round() as u16;
            samples.extend_from_slice(&alpha.to_be_bytes());
        }
        samples
    }

    /// The region as a PNG, for a file or for the clipboard.
    pub fn png(&self) -> Result<Vec<u8>, String> {
        let rgba = self.rendition();
        let per_metre = pixels_per_metre(density(self.scale));
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        // pHYs is the PNG density field: pixels per metre, the same fact a viewer reads to
        // show the picture at the size the screen gave it.
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: per_metre,
            yppu: per_metre,
            unit: png::Unit::Meter,
        }));
        {
            let mut writer = encoder
                .write_header()
                .map_err(|error| format!("encode PNG: {error}"))?;
            writer
                .write_image_data(&rgba)
                .map_err(|error| format!("encode PNG: {error}"))?;
        }
        Ok(bytes)
    }
}

/// One channel of light, in nits, as a 16-bit PQ sample.
fn quantize(nits: f32) -> u16 {
    (TransferFunction::Pq.encode(nits / 10_000.0).clamp(0.0, 1.0) * 65535.0).round() as u16
}

/// A linear RGB triple through a 3×3 colour matrix.
fn multiply(matrix: [[f32; 3]; 3], rgb: [f32; 3]) -> [f32; 3] {
    let row = |at: usize| matrix[at][0] * rgb[0] + matrix[at][1] * rgb[1] + matrix[at][2] * rgb[2];
    [row(0), row(1), row(2)]
}

/// A logical point is 96 dpi, the web's own convention: a 2× capture says 192 dpi, which is
/// exactly the scale it was taken at, in a field every viewer understands.
pub const POINT_DPI: f32 = 96.0;

/// The dpi a picture taken at `scale` declares: one logical point is 96 dpi.
pub fn density(scale: f32) -> u32 {
    (scale.max(0.01) * POINT_DPI).round() as u32
}

/// `pHYs` counts pixels per metre, not per inch.
pub fn pixels_per_metre(dpi: u32) -> u32 {
    (dpi as f32 / 0.0254).round().max(1.0) as u32
}

/// One decoded PNG: its pixels and what they mean.
pub struct Png {
    pub width: u32,
    pub height: u32,
    /// RGBA8 codes, or straight-alpha Float16 scRGB when `colour` says so.
    pub data: Vec<u8>,
    pub colour: ContentColour,
}

/// Decode PNG bytes, with the same crate this program writes them.
///
/// The ffmpeg program can only hand a picture back as a PNG, so reading that one goes
/// through a decoder we control instead of a second list of what ffmpeg can read. A 16-bit
/// PNG whose cICP says PQ is read back as the light it holds; every other PNG is sRGB codes.
pub fn read_png(bytes: &[u8]) -> Result<Png, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let (width, height, depth, color_type, cicp) = {
        let info = reader.info();
        (
            info.width,
            info.height,
            info.bit_depth,
            info.color_type,
            info.coding_independent_code_points,
        )
    };
    let capacity = reader
        .output_buffer_size()
        .ok_or_else(|| "the PNG is too large to decode".to_string())?;
    let mut pixels = vec![0; capacity];
    let frame = reader
        .next_frame(&mut pixels)
        .map_err(|error| error.to_string())?;
    pixels.truncate(frame.buffer_size());
    match (color_type, depth) {
        (png::ColorType::Rgba, png::BitDepth::Eight) => Ok(Png {
            width,
            height,
            data: pixels,
            colour: ContentColour::SDR,
        }),
        (png::ColorType::Rgba, png::BitDepth::Sixteen) => {
            Ok(sixteen_bit(width, height, pixels, cicp))
        }
        // The encoder on the other side was told to write RGBA; anything else is a producer
        // that did not keep its promise, and converting here would hide that.
        _ => Err(format!(
            "the PNG is {color_type:?} {depth:?}, not 8- or 16-bit RGBA"
        )),
    }
}

/// A 16-bit RGBA PNG as light: PQ codes become scRGB texels, anything else its sRGB code.
fn sixteen_bit(
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    cicp: Option<png::CodingIndependentCodePoints>,
) -> Png {
    let pq = cicp.filter(|code| code.transfer_function == 16);
    let Some(cicp) = pq else {
        // A 16-bit PNG with no PQ tag says sRGB in more bits: the high byte is the code.
        let data = pixels
            .as_chunks::<8>()
            .0
            .iter()
            .flat_map(|texel| [texel[0], texel[2], texel[4], texel[6]])
            .collect();
        return Png {
            width,
            height,
            data,
            colour: ContentColour::SDR,
        };
    };
    let primaries = match cicp.color_primaries {
        9 => Chromaticities::BT2020,
        12 => Chromaticities::DISPLAY_P3,
        _ => Chromaticities::SRGB,
    };
    let to_srgb = if primaries == Chromaticities::SRGB {
        None
    } else {
        Some(gamut_matrix_adapted(
            primaries,
            Chromaticities::SRGB,
            WhiteAdaptation::Bradford,
        ))
    };
    let code = |texel: &[u8; 8], at: usize| {
        f32::from(u16::from_be_bytes([texel[at], texel[at + 1]])) / 65535.0
    };
    let mut data = Vec::with_capacity(pixels.len());
    for texel in pixels.as_chunks::<8>().0 {
        let nits = [0, 2, 4].map(|at| TransferFunction::Pq.decode(code(texel, at)) * 10_000.0);
        let [red, green, blue] = to_srgb.map_or(nits, |matrix| multiply(matrix, nits));
        for channel in [red, green, blue] {
            let scrgb = f32_to_half(channel / SCRGB_WHITE_NITS);
            data.extend_from_slice(&scrgb.to_le_bytes());
        }
        data.extend_from_slice(&f32_to_half(code(texel, 6)).to_le_bytes());
    }
    Png {
        width,
        height,
        data,
        colour: ContentColour {
            storage: PixelStorage::Float16,
            primaries: Chromaticities::SRGB,
            transfer: TransferFunction::Linear,
            max_nits: 0.0,
        },
    }
}

/// What density a PNG says it was saved at, as the scale this app records; 1 when it says
/// nothing, or in a unit other than metres.
pub fn png_scale(bytes: &[u8]) -> f32 {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let Ok(reader) = decoder.read_info() else {
        return 1.0;
    };
    let Some(dims) = reader.info().pixel_dims else {
        return 1.0;
    };
    if dims.unit != png::Unit::Meter {
        return 1.0;
    }
    let dpi = dims.xppu as f32 * 0.0254;
    // A file claiming a density outside anything a screen has is not a scale to trust.
    if (24.0..=960.0).contains(&dpi) {
        dpi / POINT_DPI
    } else {
        1.0
    }
}

/// One capture rung: a platform's way of filming the screen.
///
/// A rung is tried in the platform's chain order; the app holds a [`Capture`] over whichever
/// one answered, so nothing above this trait names a protocol.
pub trait Backend {
    /// How `info status` names this rung.
    fn name(&self) -> &'static str;

    /// One grab, waiting for it.
    fn grab(&mut self, timeout: Duration) -> Result<Frame, String>;

    /// Ask for a grab the caller can poll instead of waiting in it.
    ///
    /// A rung that can only answer while the caller waits hands itself back as
    /// [`Started::Blocking`]; one that hands the wait to an event loop returns a [`Pending`].
    fn begin(self: Box<Self>, timeout: Duration) -> Result<Started, String>;
}

/// A capture asked for and not yet ready: one non-blocking pass per turn.
pub trait Pending {
    /// One non-blocking pass. `None` means "not yet"; an error ends the capture.
    fn poll(&mut self) -> Option<Result<Frame, String>>;

    /// When the caller should give up on this capture.
    fn deadline(&self) -> Instant;
}

/// What a capture request turned into.
pub enum Started {
    /// The loop polls it; no turn waits on the platform.
    Pending(Box<dyn Pending>),
    /// The rung can only grab while the caller waits (a portal's round trip, a GDI blit).
    Blocking(Box<dyn Backend>),
}

/// A bound rung, from the platform's chain.
pub struct Capture {
    backend: Box<dyn Backend>,
}

impl Capture {
    pub fn new(backend: Box<dyn Backend>) -> Self {
        Self { backend }
    }

    /// Which rung this capture bound, as `info status` reports it.
    pub fn name(&self) -> &'static str {
        self.backend.name()
    }

    /// Capture one frame, giving up after `timeout`.
    pub fn grab(&mut self, timeout: Duration) -> Result<Frame, String> {
        self.backend.grab(timeout)
    }

    /// Ask for one frame without waiting for it.
    pub fn begin(self, timeout: Duration) -> Result<Started, String> {
        self.backend.begin(timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32) -> Image {
        let data = (0..width * height)
            .flat_map(|i| [(i % 251) as u8, 0, 0, 255])
            .collect();
        Image {
            width,
            height,
            data: Arc::new(data),
            colour: ContentColour::SDR,
            scale: 1.0,
            light: Light::sdr(),
        }
    }

    /// A selection is rounded outwards onto the pixel grid: the user picked the pixels the
    /// rectangle covers, not the ones its corners land inside.
    #[test]
    fn a_crop_covers_every_pixel_the_selection_touches() {
        let source = image(4, 2);
        let cropped = source.crop(Rect::new(1.2, 0.4, 1.5, 1.2));
        assert_eq!((cropped.width, cropped.height), (2, 2));
        // The selection is where the pointer went; the region is where the pixels are. A pin
        // placed at the former sits a fraction of a pixel off and resamples every frame.
        assert_eq!(cropped.origin, (1, 0));
        // Row-major from (1, 0): source indices 1, 2, 5, 6.
        assert_eq!(
            cropped.data.as_slice(),
            vec![1, 0, 0, 255, 2, 0, 0, 255, 5, 0, 0, 255, 6, 0, 0, 255]
        );

        let narrow = source.crop(Rect::new(1.6, 0.0, 1.0, 1.0));
        assert_eq!(
            narrow.width, 2,
            "1.6..2.6 covers pixels 1 and 2, not only 2"
        );
        let noisy = source.crop(Rect::new(0.0, 0.0, 2.000_061, 1.000_061));
        assert_eq!(
            (noisy.width, noisy.height),
            (2, 1),
            "float noise below a thousandth of a pixel is not an extra row"
        );

        let outside = source.crop(Rect::new(-5.0, -5.0, 3.0, 3.0));
        assert_eq!(
            (outside.width, outside.height),
            (0, 0),
            "nothing on screen is empty"
        );
    }

    /// A 10-bit frame keeps the platform's words; only the undefined alpha becomes opaque.
    #[test]
    fn a_ten_bit_frame_keeps_its_words_and_gains_opaque_alpha() {
        let red = 0x3ffu32;
        let mid = 512u32 << 10;
        let bytes: Vec<u8> = [red, mid]
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        let frame = Frame {
            width: 2,
            height: 1,
            stride: 8,
            pixels: bytes,
            format: PixelFormat::Bgr10,
            colour: ContentColour {
                storage: PixelStorage::Packed10,
                ..ContentColour::SDR
            },
            output: None,
            scale: 1.0,
            reference_nits: 0.0,
        };
        let image = frame.into_image();
        let (codes, alpha) = PixelStorage::Packed10.unpack(&image.data[0..4]);
        assert_eq!(
            (codes[0], codes[1], codes[2]),
            (1.0, 0.0, 0.0),
            "red is bits 0-9"
        );
        assert_eq!(alpha, 1.0, "an X format is opaque, whatever its bits said");
        let (codes, _) = PixelStorage::Packed10.unpack(&image.data[4..8]);
        assert_eq!(codes[1], 512.0 / 1023.0, "green is bits 10-19");
        assert_eq!(image.colour.storage, PixelStorage::Packed10);
    }

    /// A float frame is carried, not converted: its light above white is what a pin shows and
    /// what only the display's own placement decides about.
    #[test]
    fn a_float_frame_keeps_the_light_above_white() {
        // 8.0 is exact in binary16 and 640 nits in scRGB; alpha is 1.0.
        let half = 0x4800u16.to_le_bytes();
        let opaque = 0x3c00u16.to_le_bytes();
        let frame = Frame {
            width: 1,
            height: 1,
            stride: 8,
            pixels: [half, half, half, opaque].concat(),
            format: PixelFormat::Rgba16F,
            colour: ContentColour::scrgb(1000.0),
            output: None,
            scale: 1.0,
            reference_nits: 0.0,
        };
        let image = frame.into_image();
        assert_eq!(
            image.data.len(),
            8,
            "a float texel is eight bytes, not four"
        );
        assert_eq!(image.colour.storage, PixelStorage::Float16);
        let (values, alpha) = PixelStorage::Float16.unpack(image.data.as_slice());
        assert_eq!(
            (values[0], alpha),
            (8.0, 1.0),
            "the light above white was reduced at ingest"
        );
    }

    /// A PQ picture is measured in nits, not in codes: the brightest channel decodes through
    /// the curve before it is compared with the display's white.
    #[test]
    fn a_picture_in_pq_codes_is_measured_in_nits() {
        let red = (TransferFunction::Pq.encode(1000.0 / 10_000.0) * 1023.0).round() as u32;
        // Red is bits 0-9 and the undefined alpha is made opaque; green and blue stay zero.
        let word = red | 0xc000_0000;
        let colour = ContentColour {
            storage: PixelStorage::Packed10,
            primaries: ui_core::colour::Chromaticities::BT2020,
            transfer: TransferFunction::Pq,
            max_nits: 1000.0,
        };
        let nits = brightest_nits(&word.to_le_bytes(), colour).expect("PQ codes are absolute");
        assert!((nits - 1000.0).abs() < 5.0, "measured {nits} nits");
        assert_eq!(
            brightest_nits(&[0u8; 4], ContentColour::SDR),
            None,
            "sRGB codes say nothing about how much light there is"
        );
    }

    /// A measured float picture is rolled off into the file's white rather than clipped flat:
    /// this desktop's white and a brighter highlight keep different codes, which clipping
    /// would collapse into one.
    #[test]
    fn a_measured_picture_rolls_its_highlights_into_the_file_white() {
        // 3.0 is 240 nits in scRGB, this display's SDR white; 12.5 is 1000 nits; 1.0 is opaque.
        let texel = |light: u16| [light, light, light, 0x3c00].map(u16::to_le_bytes).concat();
        let frame = Frame {
            width: 2,
            height: 1,
            stride: 16,
            pixels: [texel(0x4200), texel(0x4a40)].concat(),
            format: PixelFormat::Rgba16F,
            colour: ContentColour::scrgb(1000.0),
            output: None,
            scale: 1.0,
            reference_nits: 240.0,
        };
        let cropped = frame.into_image().crop(Rect::new(0.0, 0.0, 2.0, 1.0));
        assert_eq!(cropped.light.peak_nits(), None, "nothing measured it yet");

        let rgba = cropped.rendition();
        assert_eq!(rgba.len(), 8);
        assert!(
            rgba[0] < rgba[4],
            "the white ({}) and the highlight ({}) landed on one code",
            rgba[0],
            rgba[4]
        );
        assert_eq!(rgba[4], 255, "the peak lands at the file's white");
        assert_eq!(rgba[3], 255, "converting a float texel keeps its alpha");
        assert_eq!(
            cropped.light.peak_nits(),
            Some(1000.0),
            "the rendition measured the picture it wrote"
        );
    }

    /// An HDR PNG keeps the light it was saved with: the file declares PQ over BT.2020 in
    /// its cICP chunk, and reading it back measures the same nits.
    #[test]
    fn an_hdr_png_keeps_its_light_through_a_round_trip() {
        // 1.0 is 80 nits in scRGB and 12.5 is 1000; both opaque and exact in binary16.
        let texel = |light: u16| [light, light, light, 0x3c00].map(u16::to_le_bytes).concat();
        let frame = Frame {
            width: 2,
            height: 1,
            stride: 16,
            pixels: [texel(0x3c00), texel(0x4a40)].concat(),
            format: PixelFormat::Rgba16F,
            colour: ContentColour::scrgb(1000.0),
            output: None,
            scale: 1.0,
            reference_nits: 240.0,
        };
        let cropped = frame.into_image().crop(Rect::new(0.0, 0.0, 2.0, 1.0));
        assert!(
            cropped.wants_hdr_file(),
            "1000 nits is above the file's white and must not be rolled off"
        );

        let bytes = cropped.hdr_png().expect("encode");
        assert!(
            bytes.windows(4).any(|tag| tag == b"cICP"),
            "the file does not say how to read its codes"
        );

        let picture = read_png(&bytes).expect("decode");
        assert_eq!(picture.colour.storage, PixelStorage::Float16);
        assert!(picture.colour.holds_absolute_light());
        assert_eq!((picture.width, picture.height), (2, 1));
        let peak = brightest_nits(&picture.data, picture.colour).expect("float texels are light");
        assert!((peak - 1000.0).abs() < 1.0, "read back {peak} nits");
        let (values, alpha) = PixelStorage::Float16.unpack(&picture.data[..8]);
        assert!(
            (values[0] * SCRGB_WHITE_NITS - 80.0).abs() < 0.5,
            "the 80-nit texel came back as {} nits",
            values[0] * SCRGB_WHITE_NITS
        );
        assert_eq!(alpha, 1.0, "converting the file lost its alpha");
    }
}
