//! Platform icon generation for exported games.

use image::{imageops, DynamicImage, ImageFormat, ImageReader, Limits, Rgba, RgbaImage};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::packaging::ExportError;

/// Decode a common raster image with limits suitable for interactive authoring.
pub fn read_image(source: &Path) -> Result<DynamicImage, ExportError> {
    let bytes = fs::metadata(source)
        .map_err(|error| ExportError::io("image", source, error))?
        .len();
    if bytes > 32 * 1024 * 1024 {
        return Err(ExportError::new(
            "image",
            format!("{} is larger than the 32 MiB image limit", source.display()),
        ));
    }
    let mut reader = ImageReader::open(source)
        .map_err(|error| {
            ExportError::new(
                "image",
                format!("could not read {}: {error}", source.display()),
            )
        })?
        .with_guessed_format()
        .map_err(|error| {
            ExportError::new(
                "image",
                format!("could not identify {}: {error}", source.display()),
            )
        })?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().map_err(|error| {
        ExportError::new(
            "image",
            format!("could not decode {}: {error}", source.display()),
        )
    })
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

/// Generate a macOS `.icns` icon with aspect-preserving transparent padding.
pub fn create_macos_icon(
    source: &Path,
    destination: &Path,
    _retained_work_dir: &Path,
) -> Result<(), ExportError> {
    let source_image = read_image(source)?;
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
        .map_err(|e| ExportError::new("icon", e.to_string()))?;
        family
            .add_icon_with_type(&image, icon_type)
            .map_err(|e| ExportError::new("icon", e.to_string()))?;
    }
    let file =
        fs::File::create(destination).map_err(|e| ExportError::io("icon", destination, e))?;
    let mut output = BufWriter::new(file);
    family
        .write(&mut output)
        .map_err(|e| ExportError::new("icon", e.to_string()))?;
    output
        .flush()
        .map_err(|e| ExportError::new("icon", e.to_string()))
}

/// Generate a Windows `.ico` icon without stretching the supplied artwork.
pub fn create_windows_icon(source: &Path, destination: &Path) -> Result<(), ExportError> {
    let source_image = read_image(source)?;
    DynamicImage::ImageRgba8(square_icon(&source_image, 256))
        .save_with_format(destination, ImageFormat::Ico)
        .map_err(|error| {
            ExportError::new(
                "icon",
                format!("could not write {}: {error}", destination.display()),
            )
        })
}

pub(crate) fn default_icon_path(runtime_kit: &Path) -> Option<PathBuf> {
    let path = runtime_kit.join("default-icon.png");
    path.is_file().then_some(path)
}
