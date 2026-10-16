//! Turning a located preview into something a web view can show quickly.
//!
//! Three sizes, each made the cheapest way that size allows:
//!
//! - **Thumb** — a few hundred pixels, for the filmstrip and the grid. Cut
//!   from the *smallest* embedded preview that is big enough, which in a RAW
//!   is the camera's screen-sized one. MEASURED: 3.4 ms to decode on a Sony
//!   ARW, against 35 ms cutting the same thumbnail from the full-size frame.
//!
//! - **Fit** — screen-sized, for the loupe. Decoded from the full-size
//!   preview at a fraction of its size, which the JPEG format gives away for
//!   free: an 8×8 block can be reconstructed as 4×4 with less arithmetic, not
//!   more. A 7008-pixel frame comes out at 3504 in 53 ms; the same frame
//!   decoded whole and then shrunk takes 49 ms + 29 ms.
//!
//! - **Full** — every pixel, for zooming in. Not decoded at all. The preview's
//!   own bytes are handed to the web view exactly as the camera wrote them,
//!   with only its orientation corrected, and the view decodes them itself.
//!
//! And one that costs nothing to make:
//!
//! - **Screen** — the camera's screen-sized preview, likewise untouched. Around
//!   1600 pixels across and a few hundred kilobytes, it is found and handed
//!   over in about a millisecond. It is what the loupe shows in the gap before
//!   a fit rendering that has not been made yet — a photo jumped to cold —
//!   so that the gap is filled with a real picture rather than a thumbnail
//!   stretched to fill the screen. It is also what is held ready for the
//!   photos about to be stepped onto: small enough to decode faster than an
//!   arrow key repeats.
//!
//!   A file with no such preview — a JPEG straight from the camera holds
//!   only itself and a postage stamp — has one MADE for it, of the same size,
//!   and kept. Without it, a held arrow key through a folder of JPEGs showed
//!   each photo as its thumbnail stretched across the screen until the loupe
//!   rendering had been decoded, however ready that was on disk.

use std::path::Path;

use fast_image_resize as fr;

use crate::raw::io::{FileSource, ReadAt};
use crate::raw::{self, jpeg, Probe};

/// Which rendering of a photo is wanted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tier {
    /// Exactly this many pixels along the long edge.
    Thumb(u32),
    /// At least this many pixels along the long edge, when the file has them.
    Fit(u32),
    /// The camera's own screen-sized preview, exactly as stored — or, for a
    /// file without one, a picture of that size made from it.
    Screen,
    Full,
}

impl Tier {
    /// The name this tier goes by in a URL and in a cache key.
    pub fn label(&self) -> String {
        match self {
            Tier::Thumb(n) => format!("thumb{n}"),
            Tier::Fit(n) => format!("fit{n}"),
            Tier::Screen => "screen".into(),
            Tier::Full => "full".into(),
        }
    }
}

pub struct Rendered {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
}

/// Decoded pixels: always three bytes each, red-green-blue.
struct Pixels {
    data: Vec<u8>,
    width: u32,
    height: u32,
    icc: Option<Vec<u8>>,
}

/// A fit rendering this much larger than asked for is kept as decoded rather
/// than shrunk again. The decoder only offers halvings, so asking for 2400
/// from a 7008-pixel frame yields 3504 — and shrinking that to 2400 would
/// spend time making the picture softer on exactly the high-density screens
/// the extra pixels are useful on.
const FIT_SLACK: f64 = 1.6;

/// How far short of the size asked for a fit rendering may fall.
///
/// The decoder's free sizes are halvings of the frame, and common frames
/// halve to just under a round number: 7008 to 3504, 6000 to 3000. Insisting
/// on the full figure would mean decoding all 33 megapixels to deliver a
/// picture 3% larger — MEASURED on a 7008-pixel frame, 53 ms for the half
/// against 49 ms + 29 ms for the whole frame shrunk back down.
const FIT_TOLERANCE: f64 = 0.85;

/// What counts as screen-sized. Smaller than this is a thumbnail, and no
/// improvement on the one already showing. Larger is the full-size picture,
/// which takes the view long enough to decode that handing it over as a
/// stopgap would be slower than the thing it was standing in for.
const SCREEN_MIN: u32 = 1000;
const SCREEN_MAX: u32 = 3000;

/// What a screen-sized picture made for a file without one aims at along
/// its long edge: about what a camera's own is.
const SCREEN_MADE: u32 = 1400;
const SCREEN_QUALITY: u8 = 85;

const THUMB_QUALITY: u8 = 82;
const FIT_QUALITY: u8 = 88;

pub fn render(path: &Path, tier: Tier) -> Result<Rendered, String> {
    if raw::extension(path) == "png" {
        return render_png(path, tier);
    }

    let src = FileSource::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let probe = raw::probe_source(&src);

    match tier {
        Tier::Full => {
            let e = probe.largest().ok_or("no preview in this file")?;
            let bytes = read(&src, e.offset, e.len)?;
            Ok(Rendered { bytes: jpeg::set_orientation(bytes, probe.orientation), mime: "image/jpeg" })
        }
        Tier::Screen => {
            if let Some(stored) = stored_screen(&src, &probe)? {
                return Ok(stored);
            }
            // None stored: one is made, the same way a loupe rendering is.
            let pixels = decode_embedded(&src, &probe, (SCREEN_MADE as f64 * FIT_TOLERANCE) as u32)?;
            let long = pixels.width.max(pixels.height) as f64;
            let pixels = if long > SCREEN_MADE as f64 * FIT_SLACK { shrink_to(pixels, SCREEN_MADE)? } else { pixels };
            encode(orient(pixels, probe.orientation), SCREEN_QUALITY)
        }
        Tier::Thumb(target) => {
            let pixels = decode_embedded(&src, &probe, target)?;
            let pixels = shrink_to(pixels, target)?;
            encode(orient(pixels, probe.orientation), THUMB_QUALITY)
        }
        Tier::Fit(target) => {
            let at_least = (target as f64 * FIT_TOLERANCE) as u32;
            let pixels = decode_embedded(&src, &probe, at_least)?;
            let long = pixels.width.max(pixels.height) as f64;
            let pixels = if long > target as f64 * FIT_SLACK { shrink_to(pixels, target)? } else { pixels };
            encode(orient(pixels, probe.orientation), FIT_QUALITY)
        }
    }
}

/// The screen-sized preview a file holds, as stored, if it holds one.
fn stored_screen(src: &dyn ReadAt, probe: &Probe) -> Result<Option<Rendered>, String> {
    let Some(e) = probe.pick(SCREEN_MIN).filter(|e| (SCREEN_MIN..=SCREEN_MAX).contains(&e.long_edge())) else {
        return Ok(None);
    };
    let bytes = read(src, e.offset, e.len)?;
    Ok(Some(Rendered { bytes: jpeg::set_orientation(bytes, probe.orientation), mime: "image/jpeg" }))
}

/// The camera's own screen-sized preview, lifted out of the file as it is.
/// `None` for a file that holds none, which must have one made (`render`).
/// Costs a read and nothing else, so it can be done on the spot.
pub fn lift_screen(path: &Path) -> Result<Option<Rendered>, String> {
    if raw::extension(path) == "png" {
        return Ok(None);
    }
    let src = FileSource::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    stored_screen(&src, &raw::probe_source(&src))
}

/// A photo's pixels, the right way up: three bytes each, red-green-blue.
pub struct Upright {
    pub rgb: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Decodes a photo's embedded picture the right way up, at the smallest size
/// the decoder offers that is at least `long_edge` on its longer side — or
/// the largest there is, if none is.
///
/// For anything that has to look at the photograph itself rather than hand
/// it to the view: finding faces, and cutting them out to be judged.
pub fn decode_upright(path: &Path, long_edge: u32) -> Result<Upright, String> {
    let src = FileSource::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let probe = raw::probe_source(&src);
    let pixels = orient(decode_embedded(&src, &probe, long_edge)?, probe.orientation);
    Ok(Upright { rgb: pixels.data, width: pixels.width, height: pixels.height })
}

/// Reuse a sufficiently detailed existing preview without making another JPEG.
/// A 640px thumbnail may be too short on panoramic images, so require a
/// 256px short edge. Prefer 1280px previews to reduce recompression changes.
pub fn grouping_pixels(path: &Path, version: &str, cache: &crate::cache::Cache) -> Result<Upright, String> {
    for tier in [Tier::Thumb(1280), Tier::Thumb(640), Tier::Screen, Tier::Fit(1600), Tier::Fit(2400), Tier::Fit(3400)] {
        let key = cache.path_for(path, version, &tier.label());
        if let Some(bytes) = cache.get(&key) {
            if dimensions(&bytes).is_some_and(|(w,h)| w.min(h) >= 256) {
                if let Ok(p) = decode_jpeg(&bytes, 512) {
                    // Generated previews are upright; lifted camera Screen
                    // JPEGs retain EXIF orientation, so apply it exactly once.
                    let p = orient(p, jpeg::own_orientation(&bytes).unwrap_or(1));
                    return Ok(Upright { rgb: p.data, width: p.width, height: p.height });
                }
            }
        }
    }
    decode_upright(path, 512)
}

/// Encodes pixels as a JPEG. For tools that save what they cut out.
pub fn encode_jpeg(rgb: Vec<u8>, width: u32, height: u32, quality: u8) -> Result<Vec<u8>, String> {
    encode(Pixels { data: rgb, width, height, icc: None }, quality).map(|r| r.bytes)
}

fn read(src: &dyn ReadAt, offset: u64, len: u64) -> Result<Vec<u8>, String> {
    src.read_at(offset, len as usize).ok_or_else(|| "preview runs past the end of the file".to_string())
}

/// Decodes the cheapest embedded picture that can supply `target` pixels.
///
/// If that one turns out not to decode — a preview cut short by a card pulled
/// early, say — the next size up is tried rather than giving up on a file
/// that still has a perfectly good picture in it.
fn decode_embedded(src: &dyn ReadAt, probe: &Probe, target: u32) -> Result<Pixels, String> {
    let first = probe.pick(target).ok_or("no preview in this file")?;
    let mut last_error = String::new();
    let candidates = std::iter::once(first)
        .chain(probe.embedded.iter().copied().filter(|e| e.offset != first.offset && e.long_edge() > first.long_edge()));
    for e in candidates {
        match read(src, e.offset, e.len).and_then(|bytes| decode_jpeg(&bytes, target)) {
            Ok(pixels) => return Ok(pixels),
            Err(err) => last_error = err,
        }
    }
    Err(last_error)
}

/// Which JPEG code this build makes previews with. Part of every cached
/// preview's name: the two do not produce the same sizes for the same
/// request, and a cache must not hand one's work back as the other's.
pub const CODEC: &str = if cfg!(any(feature = "turbo", feature = "turbo-system")) { "turbo" } else { "rust" };

/// Decodes a JPEG at a reduced size that still gives `target` pixels along
/// the long edge.
fn decode_jpeg(bytes: &[u8], target: u32) -> Result<Pixels, String> {
    // libjpeg-turbo, when the build has it. Anything it will not decode —
    // CMYK, most likely, which no camera writes — is handed to the portable
    // decoder rather than refused.
    #[cfg(any(feature = "turbo", feature = "turbo-system"))]
    if let Ok(pixels) = turbo::decode(bytes, target) {
        return Ok(pixels);
    }
    decode_portable(bytes, target)
}

/// The pure-Rust decoder: builds anywhere, and about a third the speed.
fn decode_portable(bytes: &[u8], target: u32) -> Result<Pixels, String> {
    let mut decoder = jpeg_decoder::Decoder::new(bytes);
    decoder.read_info().map_err(|e| e.to_string())?;
    let info = decoder.info().ok_or("JPEG has no header")?;

    // Ask for the size whose long edge is `target`. The decoder answers with
    // the nearest of its fractions at or above that — an eighth, a quarter, a
    // half, or the whole thing.
    let (w, h) = (info.width as f64, info.height as f64);
    let k = (target as f64 / w.max(h)).min(1.0);
    let (width, height) = decoder
        .scale((w * k).ceil().max(1.0) as u16, (h * k).ceil().max(1.0) as u16)
        .map_err(|e| e.to_string())?;

    let icc = decoder.icc_profile();
    let raw = decoder.decode().map_err(|e| e.to_string())?;
    let format = decoder.info().ok_or("JPEG has no header")?.pixel_format;

    let data = match format {
        jpeg_decoder::PixelFormat::RGB24 => raw,
        jpeg_decoder::PixelFormat::L8 => raw.iter().flat_map(|&v| [v, v, v]).collect(),
        // Sixteen-bit grey, most significant byte first.
        jpeg_decoder::PixelFormat::L16 => raw.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0]]).collect(),
        // Inverted CMYK, as Adobe writes it. Not something a camera produces,
        // but a folder of scans may hold one.
        jpeg_decoder::PixelFormat::CMYK32 => raw
            .chunks_exact(4)
            .flat_map(|c| {
                let k = c[3] as u32;
                [(c[0] as u32 * k / 255) as u8, (c[1] as u32 * k / 255) as u8, (c[2] as u32 * k / 255) as u8]
            })
            .collect(),
    };
    Ok(Pixels { data, width: width as u32, height: height as u32, icc })
}

/// libjpeg-turbo: the same two jobs as the portable code beside it, done by
/// a library hand-tuned for each processor.
///
/// MEASURED on a 7008-pixel frame, decoding at half size and writing the
/// result, on one core: 38 ms here against 99 ms. One core is the figure that
/// matters most, because a folder is prepared many photos at a time with a
/// core apiece — so this is close to three times the photos a second.
#[cfg(any(feature = "turbo", feature = "turbo-system"))]
mod turbo {
    use super::Pixels;
    use crate::raw::jpeg;

    /// The sizes it can decode straight to, as fractions of the original.
    /// Finer than the portable decoder's halvings, so less is decoded only to
    /// be thrown away: asked for 2040 from 7008, this stops at 2628 where the
    /// other has to go to 3504.
    const FRACTIONS: [(usize, usize); 8] = [(1, 8), (1, 4), (3, 8), (1, 2), (5, 8), (3, 4), (7, 8), (1, 1)];

    pub fn decode(bytes: &[u8], target: u32) -> Result<Pixels, String> {
        let mut decoder = turbojpeg::Decompressor::new().map_err(|e| e.to_string())?;
        let header = decoder.read_header(bytes).map_err(|e| e.to_string())?;
        let long = header.width.max(header.height);
        let factor = FRACTIONS
            .into_iter()
            .map(|(n, d)| turbojpeg::ScalingFactor::new(n, d))
            .find(|f| f.scale(long) >= target as usize)
            .unwrap_or(turbojpeg::ScalingFactor::ONE);
        decoder.set_scaling_factor(factor).map_err(|e| e.to_string())?;

        let size = header.scaled(factor);
        let mut image = turbojpeg::Image {
            pixels: vec![0u8; size.width * size.height * 3],
            width: size.width,
            pitch: size.width * 3,
            height: size.height,
            format: turbojpeg::PixelFormat::RGB,
        };
        decoder.decompress(bytes, image.as_deref_mut()).map_err(|e| e.to_string())?;
        Ok(Pixels {
            data: image.pixels,
            width: size.width as u32,
            height: size.height as u32,
            icc: jpeg::icc_profile(bytes),
        })
    }

    pub fn encode(p: &Pixels, quality: u8) -> Result<Vec<u8>, String> {
        let mut encoder = turbojpeg::Compressor::new().map_err(|e| e.to_string())?;
        encoder.set_quality(quality as i32).map_err(|e| e.to_string())?;
        encoder.set_subsamp(turbojpeg::Subsamp::Sub2x2).map_err(|e| e.to_string())?;
        let image = turbojpeg::Image {
            pixels: p.data.as_slice(),
            width: p.width as usize,
            pitch: p.width as usize * 3,
            height: p.height as usize,
            format: turbojpeg::PixelFormat::RGB,
        };
        let bytes = encoder.compress_to_vec(image).map_err(|e| e.to_string())?;
        Ok(match &p.icc {
            Some(icc) => jpeg::with_icc_profile(bytes, icc),
            None => bytes,
        })
    }
}

/// Shrinks so the long edge is `target`. Never enlarges: a preview smaller
/// than asked for is sent as it is, and the view scales it.
fn shrink_to(p: Pixels, target: u32) -> Result<Pixels, String> {
    let long = p.width.max(p.height);
    if long <= target {
        return Ok(p);
    }
    let k = target as f64 / long as f64;
    let width = ((p.width as f64 * k).round() as u32).max(1);
    let height = ((p.height as f64 * k).round() as u32).max(1);

    let src = fr::images::ImageRef::new(p.width, p.height, &p.data, fr::PixelType::U8x3)
        .map_err(|e| e.to_string())?;
    let mut dst = fr::images::Image::new(width, height, fr::PixelType::U8x3);
    fr::Resizer::new()
        .resize(
            &src,
            &mut dst,
            &fr::ResizeOptions::new().resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Hamming)),
        )
        .map_err(|e| e.to_string())?;
    Ok(Pixels { data: dst.into_vec(), width, height, icc: p.icc })
}

/// Turns the pixels the right way up.
///
/// Done after shrinking, so it moves the fewest pixels it can, and done here
/// rather than left to the view because these renderings are re-encoded
/// anyway — there is no saving in keeping them sideways.
fn orient(p: Pixels, orientation: u8) -> Pixels {
    if !(2..=8).contains(&orientation) {
        return p;
    }
    let (w, h) = (p.width as usize, p.height as usize);
    let swaps = orientation >= 5;
    let (ow, oh) = if swaps { (h, w) } else { (w, h) };
    let mut out = vec![0u8; p.data.len()];

    for oy in 0..oh {
        for ox in 0..ow {
            // Where this output pixel comes from in the source, for each of
            // the eight ways EXIF can describe a frame.
            let (sx, sy) = match orientation {
                2 => (w - 1 - ox, oy),
                3 => (w - 1 - ox, h - 1 - oy),
                4 => (ox, h - 1 - oy),
                5 => (oy, ox),
                6 => (oy, h - 1 - ox),
                7 => (w - 1 - oy, h - 1 - ox),
                _ => (w - 1 - oy, ox), // 8
            };
            let s = (sy * w + sx) * 3;
            let d = (oy * ow + ox) * 3;
            out[d..d + 3].copy_from_slice(&p.data[s..s + 3]);
        }
    }
    Pixels { data: out, width: ow as u32, height: oh as u32, icc: p.icc }
}

/// Writes pixels out as a JPEG.
///
/// Colour at half resolution, as the camera's own JPEGs store it: smaller and
/// quicker to both write and read back, for no difference a preview can show.
///
/// The source's colour profile is carried through. A camera set to Adobe RGB
/// writes Adobe RGB numbers, and a preview that drops the profile shows those
/// numbers as though they were sRGB — every colour pulled inward and duller
/// than the photograph.
fn encode(p: Pixels, quality: u8) -> Result<Rendered, String> {
    #[cfg(any(feature = "turbo", feature = "turbo-system"))]
    let bytes = turbo::encode(&p, quality)?;
    #[cfg(not(any(feature = "turbo", feature = "turbo-system")))]
    let bytes = encode_portable(&p, quality)?;
    Ok(Rendered { bytes, mime: "image/jpeg" })
}

#[cfg_attr(any(feature = "turbo", feature = "turbo-system"), allow(dead_code))]
fn encode_portable(p: &Pixels, quality: u8) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(p.data.len() / 8);
    let mut encoder = jpeg_encoder::Encoder::new(&mut bytes, quality);
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_2_2);
    if let Some(icc) = &p.icc {
        encoder.add_icc_profile(icc).map_err(|e| e.to_string())?;
    }
    encoder
        .encode(&p.data, p.width as u16, p.height as u16, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// PNGs are not what this app is for, but a photo folder collects them —
/// screenshots, exports, a logo — and showing a blank cell for each is worse
/// than drawing them.
fn render_png(path: &Path, tier: Tier) -> Result<Rendered, String> {
    if tier == Tier::Full {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        return Ok(Rendered { bytes, mime: "image/png" });
    }

    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let size = reader.output_buffer_size().ok_or("PNG is too large to decode")?;
    let mut buf = vec![0u8; size];
    let frame = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(frame.buffer_size());

    let data: Vec<u8> = match frame.color_type {
        png::ColorType::Rgb => buf,
        png::ColorType::Rgba => buf.chunks_exact(4).flat_map(|c| [c[0], c[1], c[2]]).collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&v| [v, v, v]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0]]).collect(),
        png::ColorType::Indexed => return Err("indexed PNG was not expanded".into()),
    };
    let pixels = Pixels { data, width: frame.width, height: frame.height, icc: None };

    match tier {
        Tier::Thumb(n) => encode(shrink_to(pixels, n)?, THUMB_QUALITY),
        Tier::Fit(n) => encode(shrink_to(pixels, n.max(2400))?, FIT_QUALITY),
        Tier::Screen => Err("a PNG has no embedded preview".into()),
        Tier::Full => unreachable!("handled above"),
    }
}

/// Width and height of a rendered JPEG, read back from its header.
pub fn dimensions(jpeg_bytes: &[u8]) -> Option<(u32, u32)> {
    jpeg::frame(&jpeg_bytes, 0, jpeg_bytes.len() as u64).map(|f| (f.width, f.height))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3×2 frame whose pixels are numbered, so a turn can be read off.
    ///
    ///     0 1 2
    ///     3 4 5
    fn numbered() -> Pixels {
        let data = (0u8..6).flat_map(|v| [v, v, v]).collect();
        Pixels { data, width: 3, height: 2, icc: None }
    }

    fn layout(p: &Pixels) -> Vec<Vec<u8>> {
        p.data.chunks_exact(3).map(|c| c[0]).collect::<Vec<_>>().chunks(p.width as usize).map(<[u8]>::to_vec).collect()
    }

    #[test]
    fn each_orientation_turns_the_frame_the_way_exif_says() {
        // 1: as shot.
        assert_eq!(layout(&orient(numbered(), 1)), vec![vec![0, 1, 2], vec![3, 4, 5]]);
        // 2: mirrored left to right.
        assert_eq!(layout(&orient(numbered(), 2)), vec![vec![2, 1, 0], vec![5, 4, 3]]);
        // 3: upside down.
        assert_eq!(layout(&orient(numbered(), 3)), vec![vec![5, 4, 3], vec![2, 1, 0]]);
        // 4: mirrored top to bottom.
        assert_eq!(layout(&orient(numbered(), 4)), vec![vec![3, 4, 5], vec![0, 1, 2]]);
        // 5: flipped across the diagonal.
        assert_eq!(layout(&orient(numbered(), 5)), vec![vec![0, 3], vec![1, 4], vec![2, 5]]);
        // 6: camera turned so the stored frame needs a quarter turn clockwise.
        assert_eq!(layout(&orient(numbered(), 6)), vec![vec![3, 0], vec![4, 1], vec![5, 2]]);
        // 7: flipped across the other diagonal.
        assert_eq!(layout(&orient(numbered(), 7)), vec![vec![5, 2], vec![4, 1], vec![3, 0]]);
        // 8: a quarter turn anticlockwise.
        assert_eq!(layout(&orient(numbered(), 8)), vec![vec![2, 5], vec![1, 4], vec![0, 3]]);
    }

    /// A JPEG file of the given size, such as a camera set to JPEG writes:
    /// the picture itself and nothing smaller beside it.
    fn jpeg_file(name: &str, width: u32, height: u32) -> std::path::PathBuf {
        let data: Vec<u8> = (0..width * height).flat_map(|i| [(i % 251) as u8, ((i / width) % 241) as u8, 90]).collect();
        let bytes = encode(Pixels { data, width, height, icc: None }, 90).unwrap().bytes;
        let path = std::env::temp_dir().join(format!("opencull-preview-{}-{name}.jpg", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn grouping_reuses_a_suitable_preview_without_opening_the_source() {
        let root = std::env::temp_dir().join(format!("opencull-grouping-preview-{}",std::process::id()));
        let cache = crate::cache::Cache::new(root.clone());
        let missing = root.join("source-not-present.jpg");
        let bytes = encode(Pixels {data:vec![80;640*359*3],width:640,height:359,icc:None},90).unwrap().bytes;
        let key=cache.path_for(&missing,"v1",&Tier::Thumb(640).label());
        cache.put(&key,&bytes);
        let image=grouping_pixels(&missing,"v1",&cache).unwrap();
        assert!(image.width>=512 && image.height>=256);
        assert!(grouping_pixels(&missing,"changed-version",&cache).is_err());
        cache.put(&key,b"broken preview");
        assert!(grouping_pixels(&missing,"v1",&cache).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn grouping_reuses_screen_and_fit_caches_and_applies_exif_orientation_once() {
        for (name,tier,orientation) in [("screen",Tier::Screen,6),("fit1600",Tier::Fit(1600),1),("fit2400",Tier::Fit(2400),1),("fit3400",Tier::Fit(3400),1)] {
            let root = std::env::temp_dir().join(format!("opencull-grouping-{name}-{}",std::process::id()));
            let cache = crate::cache::Cache::new(root.clone());
            let source = root.join("missing-original.RW2");
            let bytes = encode(Pixels {data:vec![80;768*512*3],width:768,height:512,icc:None},90).unwrap().bytes;
            let bytes = jpeg::set_orientation(bytes,orientation);
            let key = cache.path_for(&source,"v1",&tier.label());
            cache.put(&key,&bytes);
            let image = grouping_pixels(&source,"v1",&cache).unwrap();
            assert_eq!(image.height>image.width,orientation==6,"{name}");
            assert!(grouping_pixels(&source,"changed",&cache).is_err());
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn a_large_jpeg_has_a_screen_sized_picture_made_for_it() {
        let path = jpeg_file("large", 6000, 4000);
        assert!(lift_screen(&path).unwrap().is_none(), "it holds none of its own");
        let made = render(&path, Tier::Screen).unwrap();
        let (w, h) = dimensions(&made.bytes).unwrap();
        assert!((SCREEN_MIN..=SCREEN_MAX).contains(&w.max(h)), "made at {w}x{h}");
        assert!((w as f64 / h as f64 - 1.5).abs() < 0.01, "and the same shape");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_jpeg_already_screen_sized_is_handed_over_as_it_is() {
        let path = jpeg_file("small", 1800, 1200);
        let stored = std::fs::read(&path).unwrap();
        let lifted = lift_screen(&path).unwrap().expect("it is its own screen-sized picture");
        assert_eq!(dimensions(&lifted.bytes), Some((1800, 1200)));
        assert!(lifted.bytes.len().abs_diff(stored.len()) < 64, "not decoded and encoded again");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn a_quarter_turn_swaps_width_and_height() {
        let turned = orient(numbered(), 6);
        assert_eq!((turned.width, turned.height), (2, 3));
        let flat = orient(numbered(), 3);
        assert_eq!((flat.width, flat.height), (3, 2));
    }

    #[test]
    fn an_orientation_outside_the_eight_is_ignored() {
        assert_eq!(layout(&orient(numbered(), 0)), layout(&numbered()));
        assert_eq!(layout(&orient(numbered(), 9)), layout(&numbered()));
    }

    #[test]
    fn shrinking_keeps_the_shape_and_never_enlarges() {
        let big = Pixels { data: vec![128; 600 * 400 * 3], width: 600, height: 400, icc: None };
        let small = shrink_to(big, 300).unwrap();
        assert_eq!((small.width, small.height), (300, 200));

        let already = Pixels { data: vec![128; 100 * 50 * 3], width: 100, height: 50, icc: None };
        let same = shrink_to(already, 300).unwrap();
        assert_eq!((same.width, same.height), (100, 50));
    }

    #[test]
    fn what_is_encoded_decodes_back_at_the_same_size() {
        let p = Pixels { data: vec![90; 64 * 48 * 3], width: 64, height: 48, icc: Some(b"profile".to_vec()) };
        let out = encode(p, 85).unwrap();
        assert_eq!(dimensions(&out.bytes), Some((64, 48)));

        // And the profile went with it.
        let mut d = jpeg_decoder::Decoder::new(&out.bytes[..]);
        d.read_info().unwrap();
        assert_eq!(d.icc_profile().as_deref(), Some(&b"profile"[..]));
    }
}
