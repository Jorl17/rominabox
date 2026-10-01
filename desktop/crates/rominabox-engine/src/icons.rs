//! Platform icon generation for exported games.

use image::{imageops, DynamicImage, ImageReader, Limits, Rgba, RgbaImage};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::export_error::{ErrorStage, ExportError};

/// Decode a common raster image with limits suitable for interactive authoring.
pub fn read_image(source: &Path) -> Result<DynamicImage, ExportError> {
    let bytes = fs::metadata(source)
        .map_err(|error| ExportError::io(ErrorStage::Image, source, error))?
        .len();
    if bytes > 32 * 1024 * 1024 {
        return Err(ExportError::new(
            ErrorStage::Image,
            format!("{} is larger than the 32 MiB image limit", source.display()),
        )
        .about(source));
    }
    let mut reader = ImageReader::open(source)
        .map_err(|error| {
            ExportError::new(
                ErrorStage::Image,
                format!("could not read {}: {error}", source.display()),
            )
            .about(source)
        })?
        .with_guessed_format()
        .map_err(|error| {
            ExportError::new(
                ErrorStage::Image,
                format!("could not identify {}: {error}", source.display()),
            )
            .about(source)
        })?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|error| {
        ExportError::new(
            ErrorStage::Image,
            format!("could not decode {}: {error}", source.display()),
        )
        .about(source)
    })
}

/// How far from square an opaque cover may be for us to crop it to fill the
/// macOS icon. We cut off at most 6% of its longer side.
const NEARLY_SQUARE: f32 = 1.06;
/// The picture from which macOS shows a game's icon. From macOS 26, a
/// full-bleed square appears cut to the system squircle, and a picture with
/// transparent padding appears shrunk inside a grey squircle. So we crop an
/// opaque cover that is nearly square to fill the square. We use a picture
/// that is already a squircle as it is, with or without padding. We pad
/// anything else and keep its proportions.
fn macos_face(source_image: &DynamicImage) -> DynamicImage {
    let picture = source_image.to_rgba8();
    let (width, height) = picture.dimensions();
    let opaque = picture.pixels().all(|pixel| pixel[3] == 255);
    let ratio = width.max(height) as f32 / width.min(height).max(1) as f32;
    if !opaque || ratio > NEARLY_SQUARE {
        return source_image.clone();
    }
    let side = width.min(height);
    let cropped = imageops::crop_imm(&picture, (width - side) / 2, (height - side) / 2, side, side);
    DynamicImage::ImageRgba8(cropped.to_image())
}

fn square_icon(source_image: &DynamicImage, size: u32) -> RgbaImage {
    let resized = source_image
        .resize(size, size, imageops::FilterType::Lanczos3)
        .to_rgba8();
    let mut canvas = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 0]));
    let x = (size.saturating_sub(resized.width()) / 2) as i64;
    let y = (size.saturating_sub(resized.height()) / 2) as i64;
    imageops::overlay(&mut canvas, &resized, x, y);
    canvas
}

/// Generate a macOS `.icns` icon: full bleed from a nearly square cover,
/// aspect-preserving transparent padding otherwise.
pub fn create_macos_icon(
    source: &Path,
    destination: &Path,
    _retained_work_dir: &Path,
) -> Result<(), ExportError> {
    let source_image = macos_face(&read_image(source)?);
    let mut family = icns::IconFamily::new();
    for icon_type in [
        icns::IconType::RGBA32_16x16,
        icns::IconType::RGBA32_16x16_2x,
        icns::IconType::RGBA32_32x32,
        icns::IconType::RGBA32_32x32_2x,
        icns::IconType::RGBA32_128x128,
        icns::IconType::RGBA32_128x128_2x,
        icns::IconType::RGBA32_256x256,
        icns::IconType::RGBA32_256x256_2x,
        icns::IconType::RGBA32_512x512,
        icns::IconType::RGBA32_512x512_2x,
    ] {
        let size = icon_type.pixel_width();
        let image = icns::Image::from_data(
            icns::PixelFormat::RGBA,
            size,
            size,
            square_icon(&source_image, size).into_raw(),
        )
        .map_err(|e| ExportError::new(ErrorStage::Icon, e.to_string()))?;
        family
            .add_icon_with_type(&image, icon_type)
            .map_err(|e| ExportError::new(ErrorStage::Icon, e.to_string()))?;
    }
    let file = fs::File::create(destination)
        .map_err(|e| ExportError::io(ErrorStage::Icon, destination, e))?;
    let mut output = BufWriter::new(file);
    family
        .write(&mut output)
        .map_err(|e| ExportError::new(ErrorStage::Icon, e.to_string()))?;
    output
        .flush()
        .map_err(|e| ExportError::new(ErrorStage::Icon, e.to_string()))
}

/// A Windows `.ico` with every size needed in Explorer, the taskbar and
/// Alt+Tab, scaled as for the macOS icon, without stretching the artwork.
pub fn windows_icon(source: &Path) -> Result<Vec<u8>, ExportError> {
    let source_image = read_image(source)?;
    let failed = |error: image::ImageError| {
        ExportError::new(ErrorStage::Icon, format!("could not make an icon of {}: {error}", source.display()))
            .about(source)
    };
    let frames = [16, 24, 32, 48, 64, 128, 256]
        .into_iter()
        .map(|size| {
            let square = square_icon(&source_image, size);
            image::codecs::ico::IcoFrame::as_png(square.as_raw(), size, size, image::ExtendedColorType::Rgba8)
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(failed)?;
    let mut bytes = Vec::new();
    image::codecs::ico::IcoEncoder::new(&mut bytes)
        .encode_images(&frames)
        .map_err(failed)?;
    Ok(bytes)
}

pub(crate) fn default_icon_path(runtime_kit: &Path) -> Option<PathBuf> {
    let path = runtime_kit.join("default-icon.png");
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COVER: Rgba<u8> = Rgba([200, 40, 30, 255]);

    fn cover(width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(width, height, COVER))
    }

    fn full_bleed(icon: &RgbaImage) -> bool {
        icon.pixels().all(|pixel| pixel[3] == 255)
    }

    /// We extend a square cover, or one a few pixels from square, to the edges
    /// of the macOS icon, so it appears cut to the squircle and not shrunk
    /// inside a grey one.
    #[test]
    fn a_square_or_nearly_square_cover_fills_the_icon() {
        for (width, height) in [(100, 100), (100, 104), (104, 100)] {
            let icon = square_icon(&macos_face(&cover(width, height)), 64);
            assert!(full_bleed(&icon), "{width}x{height} was padded");
        }
    }

    /// We pad a tall cover and keep its proportions, because cropping it to a
    /// square would cut away most of its art.
    #[test]
    fn a_tall_cover_keeps_its_shape() {
        let tall = square_icon(&macos_face(&cover(100, 140)), 64);
        assert_eq!(tall.get_pixel(0, 32)[3], 0, "a tall cover was not padded");
    }

    /// The Windows icon of a tall cover is the whole cover in its proportions,
    /// sized to fit the square of a Windows icon, with every pixel beside it
    /// fully transparent. We do not stretch it, crop it or draw around it.
    #[test]
    fn a_rectangular_cover_is_padded_to_a_square_with_transparent_pixels() {
        let root = rominabox_scratch::Scratch::dir("rominabox-windows-icon");
        let cover = root.join("tall.png");
        RgbaImage::from_pixel(100, 140, COVER).save(&cover).unwrap();
        let icon = image::load_from_memory_with_format(
            &windows_icon(&cover).unwrap(),
            image::ImageFormat::Ico,
        )
        .unwrap()
        .to_rgba8();
        assert_eq!(icon.dimensions(), (256, 256));
        let opaque: Vec<u32> = (0..256)
            .filter(|&x| icon.get_pixel(x, 128)[3] == 255)
            .collect();
        let (left, right) = (opaque[0], *opaque.last().unwrap());
        assert_eq!(right - left + 1, 183, "the cover keeps its shape: 256 × 100/140 wide");
        assert!((0..left).chain(right + 1..256).all(|x| icon.get_pixel(x, 128)[3] == 0));
        assert!((0..256).all(|y| icon.get_pixel(128, y)[3] == 255), "the cover's full height");
        assert_eq!(*icon.get_pixel(128, 128), COVER);
    }
}
