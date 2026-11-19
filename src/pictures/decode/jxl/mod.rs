//! JPEG XL through `jxl-oxide`, in pure Rust.
//!
//! `jxl-oxide` has no scaled decode: the grid decodes the full image, and the disk cache keeps
//! the result. An animated file shows its first frame. `jxl-oxide` renders XYB and the enum
//! encodings into the target itself. ICC samples go through `moxcms` (`IccToTarget`). A PQ or
//! HLG file keeps its 16-bit signal for the tone map of `Decoded` (`HdrSignal`). Every route
//! copies the rendered grids on all cores (`render_samples`).

use crate::pictures::decode::Decoded;
use crate::pictures::decode::color::{ColorTarget, SourceColor};
use crate::pictures::decode::hdr;
use image::{DynamicImage, GrayAlphaImage, GrayImage, ImageBuffer, RgbImage, RgbaImage};
use jxl_oxide::color::{ColourEncoding, Primaries, TransferFunction, WhitePoint};
use jxl_oxide::image::BitDepth;
use jxl_oxide::{
    AlignedGrid, AllocTracker, AuxBoxData, EnumColourEncoding, ExtraChannel, FrameBufferSample,
    JxlImage, Moxcms, PixelFormat, Render, RenderingIntent,
};
use jxl_render::ImageBuffer as Grid;
use moxcms::{
    CmsError, ColorProfile, Layout, TransferCharacteristics, Transform8BitExecutor,
    Transform16BitExecutor, TransformOptions,
};
use rayon::prelude::*;
use std::io;
use std::path::Path;
use std::sync::Arc;

/// A larger header fails with an error instead of an abort. A 24 MP RGB image needs 300 MB.
const DECODER_MEMORY_LIMIT: usize = 2 * 1024 * 1024 * 1024;

const ROWS_PER_TASK: usize = 64;

/// Decode the first frame of `path` into `target`. The codestream orientation applies, the EXIF
/// orientation does not.
pub fn decode(path: &Path, target: ColorTarget) -> io::Result<Decoded> {
    let image = open_for_render(path)?;
    let format = image.pixel_format();
    if let Some(signal) = HdrSignal::for_image(&image, format) {
        reserve(&image, hdr_bytes_per_pixel(format))?;
        return signal.decode(image);
    }
    let in_target = |image| Decoded {
        image,
        color: SourceColor::of_target(target),
        opaque: false,
        peak: None,
    };
    if let Some(icc) = IccToTarget::for_image(&image, format, target) {
        reserve(&image, icc.bytes_per_pixel())?;
        return decode_icc_or_enum(path, &image, &icc, target).map(in_target);
    }
    reserve(&image, format.channels())?;
    decode_enum(image, target).map(in_target)
}

/// The rendered 16-bit samples, plus the RGB16 or RGBA16 copy that a gray file needs.
fn hdr_bytes_per_pixel(format: PixelFormat) -> usize {
    let rendered = format.channels() * 2;
    if format.is_grayscale() {
        rendered + (format.channels() + 2) * 2
    } else {
        rendered
    }
}

/// The 16-bit signal of a PQ or HLG file and its CICP code points.
struct HdrSignal {
    color: SourceColor,
    /// The enum encoding that turns XYB into the signal. `None` for ICC samples.
    /// `jxl-oxide` 0.12 clips XYB to the sRGB gamut before the primaries conversion, and a lossy
    /// file loses saturation there.
    encoding: Option<EnumColourEncoding>,
    /// The mastering maximum of a PQ file whose encoder set `intensity_target`.
    peak: Option<f64>,
}

/// The `intensity_target` values that carry no peak: 255 is the header default, and cjxl writes
/// 10000 (the PQ ceiling) for each PQ file that has no `--intensity_target`.
const UNSET_INTENSITY_TARGETS: [f32; 2] = [255.0, 10_000.0];

impl HdrSignal {
    /// The signal of a file with a PQ or HLG curve. A gray file counts only with an enum encoding.
    /// Custom primaries, other white points and XYB with an ICC profile stay with `jxl-oxide`.
    fn for_image(image: &JxlImage, format: PixelFormat) -> Option<Self> {
        let gray = format.is_grayscale();
        if !gray && !matches!(format, PixelFormat::Rgb | PixelFormat::Rgba) {
            return None;
        }
        let metadata = &image.image_header().metadata;
        let target = metadata.tone_mapping.intensity_target;
        let peak = |color: &SourceColor| {
            let set = !UNSET_INTENSITY_TARGETS.contains(&target);
            (hdr::is_pq(color) && set)
                .then(|| hdr::hdr10_peak(None, Some(f64::from(target))))
                .flatten()
        };
        match &metadata.colour_encoding {
            ColourEncoding::Enum(encoding) => {
                let transfer = match encoding.tf {
                    TransferFunction::Pq => 16,
                    TransferFunction::Hlg => 18,
                    _ => return None,
                };
                // A gray ramp stays neutral in each D65 space. BT.2100 is the code point that
                // `decode/hdr/` knows.
                let primaries = match (&encoding.primaries, &encoding.white_point) {
                    _ if gray && matches!(encoding.white_point, WhitePoint::D65) => 9,
                    (Primaries::Srgb, WhitePoint::D65) => 1,
                    (Primaries::Bt2100, WhitePoint::D65) => 9,
                    (Primaries::P3, WhitePoint::Dci) => 11,
                    (Primaries::P3, WhitePoint::D65) => 12,
                    _ => return None,
                };
                let color = SourceColor::Cicp {
                    primaries,
                    transfer,
                };
                Some(Self {
                    peak: peak(&color),
                    color,
                    encoding: Some(EnumColourEncoding {
                        rendering_intent: RenderingIntent::Relative,
                        ..encoding.clone()
                    }),
                })
            }
            ColourEncoding::IccProfile(_) if !metadata.xyb_encoded && !gray => {
                let color = SourceColor::icc(image.original_icc()?.to_vec());
                hdr::is_hdr(&color).then_some(Self {
                    peak: peak(&color),
                    color,
                    encoding: None,
                })
            }
            ColourEncoding::IccProfile(_) => None,
        }
    }

    fn decode(self, mut image: JxlImage) -> io::Result<Decoded> {
        if let Some(encoding) = self.encoding {
            image.set_cms(Moxcms);
            image.request_color_encoding(encoding);
        }
        let render = image.render_frame(0).map_err(invalid)?;
        let Samples {
            width,
            height,
            channels,
            data: samples,
        } = render_samples::<u16>(&image, &render);
        let samples = match channels {
            1 => samples.iter().flat_map(|&value| [value; 3]).collect(),
            2 => samples
                .as_chunks::<2>()
                .0
                .iter()
                .flat_map(|&[gray, alpha]| [gray, gray, gray, alpha])
                .collect(),
            _ => samples,
        };
        let decoded = match channels {
            1 | 3 => ImageBuffer::from_raw(width, height, samples).map(DynamicImage::ImageRgb16),
            2 | 4 => ImageBuffer::from_raw(width, height, samples).map(DynamicImage::ImageRgba16),
            _ => None,
        };
        Ok(Decoded {
            image: decoded
                .ok_or_else(|| invalid(format!("HDR JPEG XL image with {channels} channels")))?,
            color: self.color,
            opaque: false,
            peak: self.peak,
        })
    }
}

/// Open `path` with the memory limit, and refuse CMYK.
fn open_for_render(path: &Path) -> io::Result<JxlImage> {
    let image = JxlImage::builder()
        .alloc_tracker(AllocTracker::with_limit(DECODER_MEMORY_LIMIT))
        .open(path)
        .map_err(invalid)?;
    if matches!(image.pixel_format(), PixelFormat::Cmyk | PixelFormat::Cmyka) {
        return Err(invalid("CMYK JPEG XL images are not supported"));
    }
    Ok(image)
}

/// Check the buffers against the allocation limit of the `image` route.
fn reserve(image: &JxlImage, bytes_per_pixel: usize) -> io::Result<()> {
    let bytes = u64::from(image.width())
        .checked_mul(u64::from(image.height()))
        .and_then(|pixels| pixels.checked_mul(bytes_per_pixel as u64))
        .ok_or_else(|| invalid("JPEG XL image size overflows"))?;
    image::Limits::default().reserve(bytes).map_err(invalid)
}

fn decode_enum(mut image: JxlImage, target: ColorTarget) -> io::Result<DynamicImage> {
    request_target(&mut image, target);
    let render = image.render_frame(0).map_err(invalid)?;
    let samples = render_samples::<u8>(&image, &render);
    into_image(
        samples.width,
        samples.height,
        samples.channels,
        samples.data,
    )
}

/// Gray keeps the white point and the curve of sRGB in both targets.
fn request_target(image: &mut JxlImage, target: ColorTarget) {
    let intent = RenderingIntent::Relative;
    let encoding = match target {
        _ if image.pixel_format().is_grayscale() => EnumColourEncoding::gray_srgb(intent),
        ColorTarget::Srgb => EnumColourEncoding::srgb(intent),
        ColorTarget::DisplayP3 => EnumColourEncoding::display_p3(intent),
    };
    image.set_cms(Moxcms);
    image.request_color_encoding(encoding);
}

/// A transform error falls back to `jxl-oxide` with a new decoder. The first decoder has
/// rendered without a color request.
fn decode_icc_or_enum(
    path: &Path,
    image: &JxlImage,
    icc: &IccToTarget,
    target: ColorTarget,
) -> io::Result<DynamicImage> {
    icc.decode(image)
        .or_else(|_| decode_enum(open_for_render(path)?, target))
}

fn into_image(
    width: u32,
    height: u32,
    channels: usize,
    pixels: Vec<u8>,
) -> io::Result<DynamicImage> {
    let decoded = match channels {
        1 => GrayImage::from_raw(width, height, pixels).map(DynamicImage::ImageLuma8),
        2 => GrayAlphaImage::from_raw(width, height, pixels).map(DynamicImage::ImageLumaA8),
        3 => RgbImage::from_raw(width, height, pixels).map(DynamicImage::ImageRgb8),
        4 => RgbaImage::from_raw(width, height, pixels).map(DynamicImage::ImageRgba8),
        _ => None,
    };
    decoded.ok_or_else(|| invalid(format!("JPEG XL image with {channels} channels")))
}

/// The conversion of ICC samples into the target.
///
/// `jxl-oxide` 0.12 converts ICC v2 samples wrong: Adobe RGB blue comes out as (39, 19, 255).
/// `moxcms` gives the values of `djxl` and `magick`.
struct IccToTarget {
    transform: IccTransform,
    input_channels: usize,
    /// RGB or RGBA. A gray profile gives RGB.
    channels: usize,
}

enum IccTransform {
    Eight(Arc<Transform8BitExecutor>),
    /// Deeper files stay 16-bit up to the output and round only once.
    Sixteen(Arc<Transform16BitExecutor>),
}

impl IccToTarget {
    /// `None` for XYB, for a profile that `moxcms` cannot read and for a PQ or HLG profile.
    fn for_image(image: &JxlImage, format: PixelFormat, target: ColorTarget) -> Option<Self> {
        let metadata = &image.image_header().metadata;
        if metadata.xyb_encoded
            || !matches!(metadata.colour_encoding, ColourEncoding::IccProfile(_))
        {
            return None;
        }
        let (layout, output, input_channels) = match format {
            PixelFormat::Gray => (Layout::Gray, Layout::Rgb, 1),
            PixelFormat::Graya => (Layout::GrayAlpha, Layout::Rgba, 2),
            PixelFormat::Rgb => (Layout::Rgb, Layout::Rgb, 3),
            PixelFormat::Rgba => (Layout::Rgba, Layout::Rgba, 4),
            PixelFormat::Cmyk | PixelFormat::Cmyka => return None,
        };
        let profile = ColorProfile::new_from_slice(image.original_icc()?).ok()?;
        // moxcms does not tone-map. An RGB file with such a profile takes `HdrSignal` first.
        // jxl-oxide 0.12 does not tone-map gray or XYB with such a profile either.
        let hdr = profile.cicp.as_ref().is_some_and(|cicp| {
            matches!(
                cicp.transfer_characteristics,
                TransferCharacteristics::Smpte2084 | TransferCharacteristics::Hlg
            )
        });
        if hdr {
            return None;
        }
        let options = TransformOptions {
            rendering_intent: moxcms::RenderingIntent::RelativeColorimetric,
            ..TransformOptions::default()
        };
        let destination = target.profile();
        let transform = if metadata.bit_depth.bits_per_sample() > 8 {
            IccTransform::Sixteen(
                profile
                    .create_transform_16bit(layout, &destination, output, options)
                    .ok()?,
            )
        } else {
            IccTransform::Eight(
                profile
                    .create_transform_8bit(layout, &destination, output, options)
                    .ok()?,
            )
        };
        let channels = if output == Layout::Rgba { 4 } else { 3 };
        Some(Self {
            transform,
            input_channels,
            channels,
        })
    }

    /// The rendered samples and the output exist at the same time.
    const fn bytes_per_pixel(&self) -> usize {
        let sample_bytes = match self.transform {
            IccTransform::Eight(_) => 1,
            IccTransform::Sixteen(_) => 2,
        };
        self.input_channels * sample_bytes + self.channels
    }

    /// Render the first frame in the profile space and convert it to 8 bits in the target.
    fn decode(&self, image: &JxlImage) -> io::Result<DynamicImage> {
        let render = image.render_frame(0).map_err(invalid)?;
        if render.stream().channels() as usize != self.input_channels {
            return Err(invalid("JPEG XL channels do not match the ICC transform"));
        }
        match &self.transform {
            IccTransform::Eight(transform) => self
                .convert(&render_samples::<u8>(image, &render), |input, output| {
                    transform.transform(input, output)
                }),
            IccTransform::Sixteen(transform) => {
                self.convert(&render_samples::<u16>(image, &render), |input, output| {
                    let mut deep = vec![0u16; output.len()];
                    transform.transform(input, &mut deep)?;
                    for (value, sample) in output.iter_mut().zip(deep) {
                        *value = hdr::to_8bit(sample);
                    }
                    Ok(())
                })
            }
        }
    }

    /// Run `transform` on the rendered samples in tasks of rows.
    fn convert<S: Sync>(
        &self,
        source: &Samples<S>,
        transform: impl Fn(&[S], &mut [u8]) -> Result<(), CmsError> + Sync,
    ) -> io::Result<DynamicImage> {
        let row = source.width as usize;
        let mut pixels = vec![0u8; row * source.height as usize * self.channels];
        source
            .data
            .par_chunks(row * self.input_channels * ROWS_PER_TASK)
            .zip(pixels.par_chunks_mut(row * self.channels * ROWS_PER_TASK))
            .try_for_each(|(input, output)| transform(input, output))
            .map_err(invalid)?;
        into_image(source.width, source.height, self.channels, pixels)
    }
}

/// The samples of a render as `ImageStream` writes them: upright and interleaved, the color
/// channels and then the alpha channel.
struct Samples<S> {
    width: u32,
    height: u32,
    channels: usize,
    data: Vec<S>,
}

/// Copy the samples of `render` in tasks of rows on all cores. `ImageStream::write_to_buffer`
/// copies on one thread with an orientation and a region lookup per sample (166 ms for 12 MP).
/// The stream stays for a layout that only it knows.
fn render_samples<S: Sample>(image: &JxlImage, render: &Render) -> Samples<S> {
    let mut stream = render.stream();
    let (width, height) = (stream.width(), stream.height());
    let channels = stream.channels() as usize;
    let mut data = vec![S::default(); width as usize * height as usize * channels];
    match planes(image, render) {
        Some(planes) if planes.len() == channels => {
            copy_planes(
                &planes,
                render.orientation(),
                (width as usize, height as usize),
                &mut data,
            );
        }
        _ => {
            stream.write_to_buffer(&mut data);
        }
    }
    Samples {
        width,
        height,
        channels,
        data,
    }
}

/// An output sample, with the clamp and the rounding of `ImageStream`.
trait Sample: FrameBufferSample + Copy + Default + Send + Sync {
    /// The integer samples of this bit depth copy without a scale.
    const BITS: u32;
    /// An integer sample of the bit depth `BITS`, clamped.
    fn from_integer(value: i32) -> Self;
    /// A float sample with the nominal range 0 to 1, scaled, rounded and clamped.
    fn from_float(value: f32) -> Self;
}

impl Sample for u8 {
    const BITS: u32 = 8;

    fn from_integer(value: i32) -> Self {
        Self::try_from(value.clamp(0, 255)).unwrap_or(Self::MAX)
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the cast of jxl-oxide, NaN gives 0"
    )]
    fn from_float(value: f32) -> Self {
        (value * 255.0 + 0.5).clamp(0.0, 255.0) as Self
    }
}

impl Sample for u16 {
    const BITS: u32 = 16;

    fn from_integer(value: i32) -> Self {
        Self::try_from(value.clamp(0, 65535)).unwrap_or(Self::MAX)
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the cast of jxl-oxide, NaN gives 0"
    )]
    fn from_float(value: f32) -> Self {
        (value * 65535.0 + 0.5).clamp(0.0, 65535.0) as Self
    }
}

/// One rendered grid and the bit depth of its integer samples.
struct Plane<'r> {
    buffer: &'r Grid,
    depth: BitDepth,
}

/// The color grids and the first alpha grid, in the channel order of the stream.
///
/// `None` unless each grid starts at the image origin. The region of a grid is private in
/// `jxl-oxide` 0.12. `jxl-render` 0.12.4 starts each grid at the origin when the keyframe is the
/// first frame, the frame has no offset and the render is not cropped
/// (`parallel_copy_matches_stream` holds it). Patches put a reference frame before the keyframe,
/// so they take the stream, and so do layers. Spot colors mix into the color channels and take
/// the stream.
fn planes<'r>(image: &JxlImage, render: &'r Render) -> Option<Vec<Plane<'r>>> {
    let frame = image.frame_by_keyframe(render.keyframe_index())?;
    let header = frame.header();
    let region = image.current_image_region();
    let (extra, extra_buffers) = render.extra_channels();
    if frame.index() != 0
        || header.x0 != 0
        || header.y0 != 0
        || region.left != 0
        || region.top != 0
        || extra.iter().any(ExtraChannel::is_spot_colour)
    {
        return None;
    }
    let metadata = &image.image_header().metadata;
    let color = render.color_channels().iter().map(|buffer| Plane {
        buffer,
        depth: metadata.bit_depth,
    });
    let alpha = extra
        .iter()
        .zip(extra_buffers)
        .zip(&metadata.ec_info)
        .find(|((channel, _), _)| channel.is_alpha())
        .map(|((_, buffer), info)| Plane {
            buffer,
            depth: info.bit_depth,
        });
    Some(color.chain(alpha).collect())
}

/// Fill `data` in tasks of `ROWS_PER_TASK` output rows. `size` is the upright size.
fn copy_planes<S: Sample>(
    planes: &[Plane<'_>],
    orientation: u32,
    size: (usize, usize),
    data: &mut [S],
) {
    let row = (size.0 * planes.len()).max(1);
    data.par_chunks_mut(row * ROWS_PER_TASK)
        .enumerate()
        .for_each(|(task, rows)| {
            let first = task * ROWS_PER_TASK;
            if orientation >= 5 {
                copy_columns(planes, orientation, size, first, rows);
            } else {
                copy_rows(planes, orientation, size, first, rows);
            }
        });
}

/// Orientations 1 to 4: each output row is one grid row, read forward or backward.
fn copy_rows<S: Sample>(
    planes: &[Plane<'_>],
    orientation: u32,
    (width, height): (usize, usize),
    first: usize,
    rows: &mut [S],
) {
    let channels = planes.len();
    let reversed = matches!(orientation, 2 | 3);
    for (index, output) in rows.chunks_mut(width * channels).enumerate() {
        let y = first + index;
        let walk = Walk {
            x: if reversed { width - 1 } else { 0 },
            y: if matches!(orientation, 3 | 4) {
                height - 1 - y
            } else {
                y
            },
            reversed,
        };
        for (channel, plane) in planes.iter().enumerate() {
            plane.copy(walk, output.iter_mut().skip(channel).step_by(channels));
        }
    }
}

/// Orientations 5 to 8: each output column of the task is a part of one grid row. The task reads
/// the grid rows in order, so the reads stay in the cache.
fn copy_columns<S: Sample>(
    planes: &[Plane<'_>],
    orientation: u32,
    (width, height): (usize, usize),
    first: usize,
    rows: &mut [S],
) {
    let channels = planes.len();
    let row = width * channels;
    let reversed = matches!(orientation, 7 | 8);
    for x in 0..width {
        let walk = Walk {
            x: if reversed { height - 1 - first } else { first },
            y: if matches!(orientation, 5 | 8) {
                x
            } else {
                width - 1 - x
            },
            reversed,
        };
        for (channel, plane) in planes.iter().enumerate() {
            let column = x * channels + channel;
            plane.copy(walk, rows.chunks_mut(row).map(|output| &mut output[column]));
        }
    }
}

impl Plane<'_> {
    /// Convert the samples of one walk as `FrameBufferSample` converts a sample.
    fn copy<'s, S: Sample + 's>(&self, walk: Walk, samples: impl Iterator<Item = &'s mut S>) {
        let depth = self.depth;
        let native = depth
            == BitDepth::IntegerSample {
                bits_per_sample: S::BITS,
            };
        match self.buffer {
            Grid::F32(grid) => walk.copy(grid, samples, S::from_float),
            Grid::I32(grid) if native => walk.copy(grid, samples, S::from_integer),
            Grid::I32(grid) => walk.copy(grid, samples, |value| {
                S::from_float(depth.parse_integer_sample(value))
            }),
            Grid::I16(grid) if native => {
                walk.copy(grid, samples, |value| S::from_integer(i32::from(value)));
            }
            Grid::I16(grid) => walk.copy(grid, samples, |value| {
                S::from_float(depth.parse_integer_sample(i32::from(value)))
            }),
        }
    }
}

/// A walk along one grid row from `x`, forward or backward. The inverse of the codestream
/// orientation, as `ImageStream` maps an output sample.
#[derive(Clone, Copy)]
struct Walk {
    x: usize,
    y: usize,
    reversed: bool,
}

impl Walk {
    /// A sample outside the grid stays 0, as in the stream.
    fn copy<'s, T: Copy, S: 's>(
        self,
        grid: &AlignedGrid<T>,
        samples: impl Iterator<Item = &'s mut S>,
        convert: impl Fn(T) -> S,
    ) {
        let Some(row) = grid.try_get_row(self.y) else {
            return;
        };
        if self.reversed {
            let start = row.len().min(self.x + 1);
            let outside = self.x + 1 - start;
            for (sample, &value) in samples.skip(outside).zip(row[..start].iter().rev()) {
                *sample = convert(value);
            }
        } else {
            for (sample, &value) in samples.zip(row.get(self.x..).unwrap_or_default()) {
                *sample = convert(value);
            }
        }
    }
}

/// The times of one route: the render, the stream copy and the copy on all cores.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug)]
pub struct CopyTimes {
    /// The name of the copy route that the file took, for the report of a bench.
    pub route: &'static str,
    /// False when the file takes the stream, for example with patches.
    pub on_all_cores: bool,
    /// The time of each render of the frame.
    pub render: Vec<std::time::Duration>,
    /// The time of each copy on one core through the stream.
    pub stream: Vec<std::time::Duration>,
    /// The time of each copy on all cores.
    pub parallel: Vec<std::time::Duration>,
}

/// Render `path` as `decode` does, then time both copies, `runs` times with a new decoder each.
/// Panics if `render_samples` gives other samples than the stream.
#[cfg(any(test, feature = "test-support"))]
pub fn copy_times(path: &Path, target: ColorTarget, runs: usize) -> CopyTimes {
    let mut times = CopyTimes {
        route: "",
        on_all_cores: true,
        render: Vec::new(),
        stream: Vec::new(),
        parallel: Vec::new(),
    };
    for _ in 0..runs {
        let (route, deep, image) = prepare_like_decode(path, target);
        times.route = route;
        if deep {
            time_copy::<u16>(&image, &mut times);
        } else {
            time_copy::<u8>(&image, &mut times);
        }
    }
    times
}

/// The decoder of the route of `decode`, the route name and whether the copy writes 16 bits.
#[expect(clippy::unwrap_used, reason = "a bench helper panics on a bad file")]
#[cfg(any(test, feature = "test-support"))]
fn prepare_like_decode(path: &Path, target: ColorTarget) -> (&'static str, bool, JxlImage) {
    let mut image = open_for_render(path).unwrap();
    let format = image.pixel_format();
    if let Some(signal) = HdrSignal::for_image(&image, format) {
        if let Some(encoding) = signal.encoding {
            image.set_cms(Moxcms);
            image.request_color_encoding(encoding);
        }
        return ("HDR", true, image);
    }
    if let Some(icc) = IccToTarget::for_image(&image, format, target) {
        let deep = matches!(icc.transform, IccTransform::Sixteen(_));
        return (if deep { "ICC 16 bit" } else { "ICC 8 bit" }, deep, image);
    }
    request_target(&mut image, target);
    ("enum", false, image)
}

#[expect(clippy::unwrap_used, reason = "a bench helper panics on a bad file")]
#[cfg(any(test, feature = "test-support"))]
fn time_copy<S: Sample + PartialEq>(image: &JxlImage, times: &mut CopyTimes) {
    use std::time::Instant;
    let start = Instant::now();
    let render = image.render_frame(0).unwrap();
    times.render.push(start.elapsed());
    let start = Instant::now();
    let mut stream = render.stream();
    let length = stream.width() as usize * stream.height() as usize * stream.channels() as usize;
    let mut streamed = vec![S::default(); length];
    stream.write_to_buffer(&mut streamed);
    times.stream.push(start.elapsed());
    let start = Instant::now();
    let copied = render_samples::<S>(image, &render);
    times.parallel.push(start.elapsed());
    times.on_all_cores &= planes(image, &render).is_some();
    assert!(copied.data == streamed, "the samples differ");
}

/// The upright size from the header.
pub fn dimensions(path: &Path) -> Option<(u32, u32)> {
    let image = open(path).ok()?;
    Some((image.width(), image.height()))
}

/// The TIFF bytes of the Exif box.
pub fn exif(path: &Path) -> Option<Vec<u8>> {
    let image = open(path).ok()?;
    let AuxBoxData::Data(exif) = image.aux_boxes().first_exif().ok()? else {
        return None;
    };
    let start = usize::try_from(exif.tiff_header_offset()).ok()?;
    exif.payload().get(start..).map(<[u8]>::to_vec)
}

fn open(path: &Path) -> io::Result<JxlImage> {
    JxlImage::builder().open(path).map_err(invalid)
}

fn invalid(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests;
