use crate::error::{Result, RustMsptError};
use crate::types::RenderedImage;
use std::path::Path;

// AI-FUNC-SUMMARY:
// Purpose: Save an RGBA8 rendered image to disk, choosing the encoder from the file extension.
// Inputs: destination path, rendered image.
// Returns: Ok(()) on success.
// Side effects: Creates parent directories if needed; creates/overwrites the output file.
// Notes: Only ".png" is currently supported; other extensions return InvalidConfig.
pub fn save_image(path: &Path, image: &RenderedImage) -> Result<()> {
    if image.width == 0 || image.height == 0 {
        return Err(RustMsptError::InvalidConfig(
            "rendered image dimensions must be positive".to_string(),
        ));
    }
    if image.rgba.len() != image.width * image.height * 4 {
        return Err(RustMsptError::InvalidConfig(format!(
            "rendered image buffer has {} bytes, expected {} ({}x{} RGBA)",
            image.rgba.len(),
            image.width * image.height * 4,
            image.width,
            image.height
        )));
    }

    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match extension.as_deref() {
        Some("png") => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            ::image::save_buffer(
                path,
                &image.rgba,
                image.width as u32,
                image.height as u32,
                ::image::ColorType::Rgba8,
            )?;
            Ok(())
        }
        _ => Err(RustMsptError::InvalidConfig(format!(
            "unsupported image output extension for '{}'; supported: .png",
            path.display()
        ))),
    }
}
