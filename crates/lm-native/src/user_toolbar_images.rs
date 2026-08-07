//! Bounded Lunar Magic user-toolbar bitmap-strip loading and image-base resolution.

use eframe::egui;
use lm_app::{
    DecodedMap16Bitmap, UserToolbar, UserToolbarButton, UserToolbarImageBase, UserToolbarTarget,
    decode_map16_bitmap_bmp_image,
};
use std::{fs, io::Read, path::Path};

const MAX_STRIP_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ICON_SIZE: usize = 256;
const MAX_ICONS: usize = 4096;

#[derive(Default)]
pub(crate) struct UserToolbarImageSet {
    images: Vec<egui::ColorImage>,
    entry_starts: Vec<usize>,
    textures: Vec<egui::TextureHandle>,
    icon_size: Option<usize>,
}

impl UserToolbarImageSet {
    pub(crate) fn load(directory: &Path, toolbar: &UserToolbar) -> Result<Self, String> {
        let mut result = Self {
            icon_size: toolbar.image_size.map(usize::from),
            ..Self::default()
        };
        if result
            .icon_size
            .is_some_and(|size| !(1..=MAX_ICON_SIZE).contains(&size))
        {
            return Err(format!(
                "user toolbar icon size must be 1..={MAX_ICON_SIZE}"
            ));
        }
        for entry in &toolbar.images {
            result.entry_starts.push(result.images.len());
            let path = resolve_path(directory, &entry.path);
            let bytes = read_bounded(&path)?;
            let decoded = decode_map16_bitmap_bmp_image(&bytes).map_err(|error| {
                format!(
                    "cannot decode user toolbar bitmap {}: {error}",
                    path.display()
                )
            })?;
            let size = result.icon_size.unwrap_or(decoded.height);
            if result.icon_size.is_none() {
                if !(1..=MAX_ICON_SIZE).contains(&size) {
                    return Err(format!(
                        "user toolbar bitmap icon height must be 1..={MAX_ICON_SIZE}"
                    ));
                }
                result.icon_size = Some(size);
            }
            let images = split_strip(&decoded, size)?;
            if result.images.len().saturating_add(images.len()) > MAX_ICONS {
                return Err(format!("user toolbar image list exceeds {MAX_ICONS} icons"));
            }
            result.images.extend(images);
        }
        Ok(result)
    }

    pub(crate) fn ensure_textures(&mut self, context: &egui::Context) {
        if self.textures.len() == self.images.len() {
            return;
        }
        self.textures = self
            .images
            .iter()
            .enumerate()
            .map(|(index, image)| {
                context.load_texture(
                    format!("user-toolbar-icon-{index}"),
                    image.clone(),
                    egui::TextureOptions::NEAREST,
                )
            })
            .collect();
    }

    pub(crate) fn texture_for(&self, button: &UserToolbarButton) -> Option<&egui::TextureHandle> {
        if !uses_image_list(button) {
            return None;
        }
        let relative = isize::try_from(button.icon?).ok()?;
        let base = match button.image_base {
            UserToolbarImageBase::Global => 0,
            UserToolbarImageBase::Image(entry) => *self.entry_starts.get(entry)?,
        };
        let index = base.checked_add_signed(relative)?;
        self.textures.get(index)
    }

    pub(crate) fn icon_size(&self) -> Option<f32> {
        self.icon_size.map(|size| size as f32)
    }
}

fn uses_image_list(button: &UserToolbarButton) -> bool {
    matches!(button.target, UserToolbarTarget::Internal(_))
        || button
            .options
            .iter()
            .any(|option| option == "LM_USEIMAGE_LIST")
}

fn resolve_path(directory: &Path, value: &str) -> std::path::PathBuf {
    let executable_prefix = format!("{}{}", directory.display(), std::path::MAIN_SEPARATOR);
    let expanded = value.replace("%4", &executable_prefix);
    let path = std::path::PathBuf::from(expanded);
    if path.is_absolute() {
        path
    } else {
        directory.join(path)
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|error| {
        format!(
            "cannot open user toolbar bitmap {}: {error}",
            path.display()
        )
    })?;
    let metadata = file.metadata().map_err(|error| {
        format!(
            "cannot inspect user toolbar bitmap {}: {error}",
            path.display()
        )
    })?;
    if !metadata.is_file() || metadata.len() > MAX_STRIP_BYTES {
        return Err(format!(
            "user toolbar bitmap {} is not a bounded regular file",
            path.display()
        ));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.take(MAX_STRIP_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            format!(
                "cannot read user toolbar bitmap {}: {error}",
                path.display()
            )
        })?;
    if bytes.len() as u64 != metadata.len() {
        return Err(format!(
            "user toolbar bitmap {} changed while loading",
            path.display()
        ));
    }
    Ok(bytes)
}

fn split_strip(decoded: &DecodedMap16Bitmap, size: usize) -> Result<Vec<egui::ColorImage>, String> {
    if decoded.height != size || decoded.width == 0 || decoded.width % size != 0 {
        return Err(format!(
            "user toolbar bitmap must be {size} pixels high with width divisible by {size}, got {}x{}",
            decoded.width, decoded.height
        ));
    }
    let transparent = decoded.pixels[0];
    let mut images = Vec::with_capacity(decoded.width / size);
    for icon in 0..decoded.width / size {
        let mut rgba = Vec::with_capacity(size * size * 4);
        for y in 0..size {
            for x in 0..size {
                let pixel = decoded.pixels[y * decoded.width + icon * size + x];
                rgba.extend_from_slice(&[
                    pixel.red,
                    pixel.green,
                    pixel.blue,
                    u8::from(
                        pixel.red != transparent.red
                            || pixel.green != transparent.green
                            || pixel.blue != transparent.blue,
                    ) * pixel.alpha,
                ]);
            }
        }
        images.push(egui::ColorImage::from_rgba_unmultiplied(
            [size, size],
            &rgba,
        ));
    }
    Ok(images)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::Rgba8;

    #[test]
    fn horizontal_strip_splits_and_uses_first_pixel_as_transparency_key() {
        let key = Rgba8 {
            red: 1,
            green: 2,
            blue: 3,
            alpha: 255,
        };
        let red = Rgba8 {
            red: 200,
            green: 0,
            blue: 0,
            alpha: 255,
        };
        let decoded = DecodedMap16Bitmap {
            width: 4,
            height: 2,
            pixels: vec![key, key, red, key, key, key, key, red],
        };
        let images = split_strip(&decoded, 2).unwrap();
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].pixels[0].a(), 0);
        assert_eq!(images[1].pixels[0], egui::Color32::from_rgb(200, 0, 0));
        assert_eq!(images[1].pixels[1].a(), 0);
    }

    #[test]
    fn base_resolution_supports_global_new_and_negative_indexes() {
        let mut set = UserToolbarImageSet {
            images: vec![egui::ColorImage::default(); 4],
            entry_starts: vec![0, 2],
            textures: Vec::new(),
            icon_size: Some(16),
        };
        let context = egui::Context::default();
        set.ensure_textures(&context);
        let toolbar = UserToolbar::parse(
            "LM_ADDIMAGE \"a.bmp\"\nLM_NEWIMAGE \"b.bmp\"\n***START***\nLM_VIEW_16x16\n-1,test\n***END***",
        )
        .unwrap();
        assert_eq!(
            set.texture_for(&toolbar.buttons[0]).unwrap().id(),
            set.textures[1].id()
        );
    }

    #[test]
    fn bounded_bmp_files_load_in_directive_order_and_resolve_new_base() {
        let directory = tempfile::tempdir().unwrap();
        for (name, width) in [("first.bmp", 32), ("second.bmp", 48)] {
            let mut canvas = lm_render::Canvas::try_new(width, 16).unwrap();
            canvas.set(
                16,
                0,
                lm_render::Rgba {
                    red: 250,
                    green: 20,
                    blue: 30,
                    alpha: 255,
                },
            );
            fs::write(
                directory.path().join(name),
                lm_render::encode_bmp(&canvas).unwrap(),
            )
            .unwrap();
        }
        let toolbar = UserToolbar::parse(
            "LM_ADDIMAGE \"first.bmp\"\nLM_NEWIMAGE \"second.bmp\"\n***START***\nLM_VIEW_16x16\n1,new icon\n***END***",
        )
        .unwrap();
        let mut set = UserToolbarImageSet::load(directory.path(), &toolbar).unwrap();
        assert_eq!(set.entry_starts, [0, 2]);
        assert_eq!(set.images.len(), 5);
        assert_eq!(set.icon_size, Some(16));
        let context = egui::Context::default();
        set.ensure_textures(&context);
        assert_eq!(
            set.texture_for(&toolbar.buttons[0]).unwrap().id(),
            set.textures[3].id()
        );
    }

    #[test]
    fn malformed_strip_geometry_and_missing_files_reject_without_partial_publication() {
        let directory = tempfile::tempdir().unwrap();
        let canvas = lm_render::Canvas::try_new(17, 16).unwrap();
        fs::write(
            directory.path().join("bad.bmp"),
            lm_render::encode_bmp(&canvas).unwrap(),
        )
        .unwrap();
        let malformed = UserToolbar::parse("LM_ADDIMAGE \"bad.bmp\"").unwrap();
        assert!(UserToolbarImageSet::load(directory.path(), &malformed).is_err());
        let missing = UserToolbar::parse("LM_ADDIMAGE \"missing.bmp\"").unwrap();
        assert!(UserToolbarImageSet::load(directory.path(), &missing).is_err());
    }
}
