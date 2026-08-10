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
const MAX_TILED_IMAGE_SIDE: usize = 4096;
const MAX_TILED_IMAGE_PIXELS: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OriginalToolbarImages {
    Overworld8x8,
    Overworld,
    LevelPalette,
    Map16,
    OverworldPalette,
    AddObject,
    AddSprite,
    Background,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OriginalToolbarAction {
    Save,
    Undo,
    Redo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum OriginalCatalogAction {
    PreviewIcons,
    CompatibleGraphicsOnly,
    VerticalLayout,
    Zoom,
    PreviewArea,
}

impl OriginalToolbarImages {
    const ALL: [Self; 8] = [
        Self::Overworld8x8,
        Self::Overworld,
        Self::LevelPalette,
        Self::Map16,
        Self::OverworldPalette,
        Self::AddObject,
        Self::AddSprite,
        Self::Background,
    ];

    const fn specification(self) -> (&'static str, usize) {
        match self {
            Self::Overworld8x8 => ("Lunar Magic.ff1", 6),
            Self::Overworld => ("Lunar Magic.ff2", 32),
            Self::LevelPalette => ("Lunar Magic.ff3", 12),
            Self::Map16 => ("Lunar Magic.ff5", 20),
            Self::OverworldPalette => ("Lunar Magic.ff6", 12),
            Self::AddObject => ("Lunar Magic.ff7", 6),
            Self::AddSprite => ("Lunar Magic.ff8", 6),
            Self::Background => ("Lunar Magic.ff9", 10),
        }
    }

    const fn index(self) -> usize {
        self as usize
    }

    const fn action_index(self, action: OriginalToolbarAction) -> Option<usize> {
        match (self, action) {
            (Self::Overworld, OriginalToolbarAction::Save) => Some(1),
            (Self::Overworld, OriginalToolbarAction::Undo) => Some(2),
            (Self::Overworld, OriginalToolbarAction::Redo) => Some(3),
            (Self::Map16, OriginalToolbarAction::Save) => Some(2),
            (Self::Map16, OriginalToolbarAction::Undo) => Some(3),
            (Self::Map16, OriginalToolbarAction::Redo) => Some(4),
            (Self::LevelPalette | Self::OverworldPalette, OriginalToolbarAction::Save) => Some(11),
            (Self::LevelPalette | Self::OverworldPalette, OriginalToolbarAction::Undo) => Some(2),
            (Self::LevelPalette | Self::OverworldPalette, OriginalToolbarAction::Redo) => Some(3),
            _ => None,
        }
    }

    const fn catalog_action_index(self, action: OriginalCatalogAction) -> Option<usize> {
        if !matches!(self, Self::AddObject | Self::AddSprite) {
            return None;
        }
        Some(match action {
            OriginalCatalogAction::PreviewIcons => 1,
            OriginalCatalogAction::CompatibleGraphicsOnly => 2,
            OriginalCatalogAction::VerticalLayout => 3,
            OriginalCatalogAction::Zoom => 4,
            OriginalCatalogAction::PreviewArea => 5,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OriginalTiledImage {
    LevelCanvas,
    OverworldCanvas,
    BackgroundCanvas,
    LevelToolbar,
    OverworldToolbar,
}

impl OriginalTiledImage {
    const ALL: [Self; 5] = [
        Self::LevelCanvas,
        Self::OverworldCanvas,
        Self::BackgroundCanvas,
        Self::LevelToolbar,
        Self::OverworldToolbar,
    ];

    const fn filename(self) -> &'static str {
        match self {
            Self::LevelCanvas => "Lunar Magic.ffx",
            Self::OverworldCanvas => "Lunar Magic.ffx2",
            Self::BackgroundCanvas => "Lunar Magic.ffxhd",
            Self::LevelToolbar => "Lunar Magic.ffxi",
            Self::OverworldToolbar => "Lunar Magic.ffxii",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Default)]
struct OriginalStripImageSet {
    images: Vec<egui::ColorImage>,
    textures: Vec<egui::TextureHandle>,
    icon_size: Option<usize>,
}

#[derive(Default)]
struct OriginalTiledImageSet {
    image: Option<egui::ColorImage>,
    texture: Option<egui::TextureHandle>,
}

#[derive(Default)]
pub(crate) struct UserToolbarImageSet {
    images: Vec<egui::ColorImage>,
    entry_starts: Vec<usize>,
    textures: Vec<egui::TextureHandle>,
    executable_images: Vec<Option<egui::ColorImage>>,
    executable_textures: Vec<Option<egui::TextureHandle>>,
    icon_size: Option<usize>,
}

#[derive(Default)]
pub(crate) struct MainToolbarImageSet {
    images: Vec<egui::ColorImage>,
    textures: Vec<egui::TextureHandle>,
    icon_size: Option<usize>,
    original_strips: Vec<OriginalStripImageSet>,
    original_tiles: Vec<OriginalTiledImageSet>,
}

impl MainToolbarImageSet {
    const IMAGE_COUNT: usize = 41;

    pub(crate) fn load(directory: &Path) -> Result<Self, String> {
        let mut result = Self {
            original_strips: OriginalToolbarImages::ALL
                .iter()
                .map(|kind| load_original_strip(directory, *kind))
                .collect::<Result<_, _>>()?,
            original_tiles: OriginalTiledImage::ALL
                .iter()
                .map(|kind| load_original_tile(directory, *kind))
                .collect::<Result<_, _>>()?,
            ..Self::default()
        };
        let path = directory.join("Lunar Magic.ff4");
        if !path.exists() {
            return Ok(result);
        }
        let bytes = read_bounded(&path)?;
        let decoded = decode_map16_bitmap_bmp_image(&bytes).map_err(|error| {
            format!(
                "cannot decode custom main toolbar {}: {error}",
                path.display()
            )
        })?;
        let size = decoded.height;
        if !(1..=MAX_ICON_SIZE).contains(&size) || decoded.width != Self::IMAGE_COUNT * size {
            return Err(format!(
                "custom main toolbar must contain {} square images, got {}x{}",
                Self::IMAGE_COUNT,
                decoded.width,
                decoded.height
            ));
        }
        result.images = split_strip(&decoded, size)?;
        result.icon_size = Some(size);
        Ok(result)
    }

    pub(crate) fn ensure_textures(&mut self, context: &egui::Context) {
        if self.textures.len() != self.images.len() {
            self.textures = self
                .images
                .iter()
                .enumerate()
                .map(|(index, image)| {
                    context.load_texture(
                        format!("main-toolbar-icon-{index}"),
                        image.clone(),
                        egui::TextureOptions::NEAREST,
                    )
                })
                .collect();
        }
        for (kind, strip) in OriginalToolbarImages::ALL
            .iter()
            .zip(&mut self.original_strips)
        {
            if strip.textures.len() != strip.images.len() {
                strip.textures = strip
                    .images
                    .iter()
                    .enumerate()
                    .map(|(index, image)| {
                        context.load_texture(
                            format!("original-toolbar-{kind:?}-{index}"),
                            image.clone(),
                            egui::TextureOptions::NEAREST,
                        )
                    })
                    .collect();
            }
        }
        for (kind, tile) in OriginalTiledImage::ALL.iter().zip(&mut self.original_tiles) {
            if tile.texture.is_none()
                && let Some(image) = &tile.image
            {
                tile.texture = Some(context.load_texture(
                    format!("original-tiled-image-{kind:?}"),
                    image.clone(),
                    egui::TextureOptions::NEAREST_REPEAT,
                ));
            }
        }
    }

    pub(crate) fn texture(&self, index: usize) -> Option<&egui::TextureHandle> {
        self.textures.get(index)
    }

    pub(crate) fn icon_size(&self) -> f32 {
        self.icon_size.unwrap_or(16) as f32
    }

    pub(crate) fn original_texture(
        &self,
        kind: OriginalToolbarImages,
        index: usize,
    ) -> Option<&egui::TextureHandle> {
        self.original_strips.get(kind.index())?.textures.get(index)
    }

    pub(crate) fn original_icon_size(&self, kind: OriginalToolbarImages) -> f32 {
        self.original_strips
            .get(kind.index())
            .and_then(|strip| strip.icon_size)
            .unwrap_or(16) as f32
    }

    pub(crate) fn original_action_button(
        &self,
        ui: &mut egui::Ui,
        kind: OriginalToolbarImages,
        action: OriginalToolbarAction,
        label: &str,
        enabled: bool,
    ) -> egui::Response {
        let texture = kind
            .action_index(action)
            .and_then(|index| self.original_texture(kind, index));
        let button = texture.map_or_else(
            || egui::Button::new(label),
            |texture| {
                let size = self.original_icon_size(kind);
                egui::Button::image(egui::Image::new((texture.id(), egui::vec2(size, size))))
            },
        );
        ui.add_enabled(enabled, button).on_hover_text(label)
    }

    pub(crate) fn original_catalog_button(
        &self,
        ui: &mut egui::Ui,
        kind: OriginalToolbarImages,
        action: OriginalCatalogAction,
        label: &str,
        selected: bool,
    ) -> egui::Response {
        let texture = kind
            .catalog_action_index(action)
            .and_then(|index| self.original_texture(kind, index));
        let button = texture.map_or_else(
            || egui::Button::new(label).selected(selected),
            |texture| {
                let size = self.original_icon_size(kind);
                egui::Button::image(egui::Image::new((texture.id(), egui::vec2(size, size))))
                    .selected(selected)
            },
        );
        ui.add(button).on_hover_text(label)
    }

    pub(crate) fn tiled_texture(&self, kind: OriginalTiledImage) -> Option<&egui::TextureHandle> {
        self.original_tiles.get(kind.index())?.texture.as_ref()
    }

    pub(crate) fn paint_tiled_surface(
        &self,
        painter: &egui::Painter,
        kind: OriginalTiledImage,
        target: egui::Rect,
        origin: egui::Pos2,
    ) -> bool {
        let Some(texture) = self.tiled_texture(kind) else {
            return false;
        };
        let size = texture.size_vec2();
        let uv = tiled_surface_uv(target, origin, size);
        painter.image(texture.id(), target, uv, egui::Color32::WHITE);
        true
    }
}

fn tiled_surface_uv(target: egui::Rect, origin: egui::Pos2, tile_size: egui::Vec2) -> egui::Rect {
    debug_assert!(tile_size.x > 0.0 && tile_size.y > 0.0);
    egui::Rect::from_min_max(
        egui::pos2(
            (target.min.x - origin.x) / tile_size.x,
            (target.min.y - origin.y) / tile_size.y,
        ),
        egui::pos2(
            (target.max.x - origin.x) / tile_size.x,
            (target.max.y - origin.y) / tile_size.y,
        ),
    )
}

pub(crate) fn tiled_surface_canvas_size(content: egui::Vec2, available: egui::Vec2) -> egui::Vec2 {
    egui::vec2(content.x.max(available.x), content.y.max(available.y))
}

fn load_original_strip(
    directory: &Path,
    kind: OriginalToolbarImages,
) -> Result<OriginalStripImageSet, String> {
    let (filename, image_count) = kind.specification();
    let path = directory.join(filename);
    if !path.exists() {
        return Ok(OriginalStripImageSet::default());
    }
    let decoded = decode_custom_bitmap(&path)?;
    let size = decoded.height;
    if !(1..=MAX_ICON_SIZE).contains(&size) || decoded.width != image_count * size {
        return Err(format!(
            "custom {filename} toolbar must contain {image_count} square images, got {}x{}",
            decoded.width, decoded.height
        ));
    }
    Ok(OriginalStripImageSet {
        images: split_strip(&decoded, size)?,
        textures: Vec::new(),
        icon_size: Some(size),
    })
}

fn load_original_tile(
    directory: &Path,
    kind: OriginalTiledImage,
) -> Result<OriginalTiledImageSet, String> {
    let filename = kind.filename();
    let path = directory.join(filename);
    if !path.exists() {
        return Ok(OriginalTiledImageSet::default());
    }
    let decoded = decode_custom_bitmap(&path)?;
    let pixels = decoded
        .width
        .checked_mul(decoded.height)
        .ok_or_else(|| format!("custom tiled image {filename} dimensions overflow"))?;
    if decoded.width == 0
        || decoded.height == 0
        || decoded.width > MAX_TILED_IMAGE_SIDE
        || decoded.height > MAX_TILED_IMAGE_SIDE
        || pixels > MAX_TILED_IMAGE_PIXELS
    {
        return Err(format!(
            "custom tiled image {filename} has invalid bounded dimensions {}x{}",
            decoded.width, decoded.height
        ));
    }
    let mut rgba = Vec::with_capacity(pixels * 4);
    for pixel in decoded.pixels {
        rgba.extend_from_slice(&[pixel.red, pixel.green, pixel.blue, pixel.alpha]);
    }
    Ok(OriginalTiledImageSet {
        image: Some(egui::ColorImage::from_rgba_unmultiplied(
            [decoded.width, decoded.height],
            &rgba,
        )),
        texture: None,
    })
}

fn decode_custom_bitmap(path: &Path) -> Result<DecodedMap16Bitmap, String> {
    let bytes = read_bounded(path)?;
    decode_map16_bitmap_bmp_image(&bytes).map_err(|error| {
        format!(
            "cannot decode custom GUI bitmap {}: {error}",
            path.display()
        )
    })
}

impl UserToolbarImageSet {
    pub(crate) fn load(directory: &Path, toolbar: &UserToolbar) -> Result<Self, String> {
        Self::load_with_icon_extractor(directory, toolbar, platform_executable_icon)
    }

    fn load_with_icon_extractor(
        directory: &Path,
        toolbar: &UserToolbar,
        mut extract: impl FnMut(&Path, i32, usize) -> Option<egui::ColorImage>,
    ) -> Result<Self, String> {
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
        let executable_size = result.icon_size.unwrap_or(16);
        result.executable_images = toolbar
            .buttons
            .iter()
            .enumerate()
            .map(|(index, button)| {
                executable_icon_request(directory, toolbar, index, button)
                    .and_then(|(path, icon)| extract(&path, icon, executable_size))
            })
            .collect();
        Ok(result)
    }

    pub(crate) fn ensure_textures(&mut self, context: &egui::Context) {
        if self.textures.len() != self.images.len() {
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
        if self.executable_textures.len() != self.executable_images.len() {
            self.executable_textures = self
                .executable_images
                .iter()
                .enumerate()
                .map(|(index, image)| {
                    image.as_ref().map(|image| {
                        context.load_texture(
                            format!("user-toolbar-executable-icon-{index}"),
                            image.clone(),
                            egui::TextureOptions::LINEAR,
                        )
                    })
                })
                .collect();
        }
    }

    pub(crate) fn texture_for(
        &self,
        toolbar: &UserToolbar,
        button_index: usize,
    ) -> Option<&egui::TextureHandle> {
        let button = toolbar.buttons.get(button_index)?;
        let forced = forced_image_index(toolbar, button_index);
        if forced.is_none() && !uses_image_list(button) {
            return self
                .executable_textures
                .get(button_index)
                .and_then(Option::as_ref);
        }
        let relative = isize::try_from(forced.map(i32::from).or(button.icon)?).ok()?;
        let base = match forced.map_or(button.image_base, |_| UserToolbarImageBase::Global) {
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

fn executable_icon_request(
    directory: &Path,
    toolbar: &UserToolbar,
    button_index: usize,
    button: &UserToolbarButton,
) -> Option<(std::path::PathBuf, i32)> {
    if forced_image_index(toolbar, button_index).is_some() || uses_image_list(button) {
        return None;
    }
    let UserToolbarTarget::External(command_line) = &button.target else {
        return None;
    };
    let executable = first_command_word(command_line)?;
    Some((
        resolve_path(directory, &executable),
        button.icon.unwrap_or(0),
    ))
}

fn first_command_word(value: &str) -> Option<String> {
    let value = value.trim_start();
    if let Some(rest) = value.strip_prefix('"') {
        let end = rest.find('"')?;
        (!rest[..end].is_empty()).then(|| rest[..end].to_owned())
    } else {
        let end = value.find(char::is_whitespace).unwrap_or(value.len());
        (end != 0).then(|| value[..end].to_owned())
    }
}

#[cfg(windows)]
fn platform_executable_icon(path: &Path, icon: i32, size: usize) -> Option<egui::ColorImage> {
    let decoded = lm_windows::executable_icon(path, icon, u32::try_from(size).ok()?).ok()?;
    let width = usize::try_from(decoded.width).ok()?;
    let height = usize::try_from(decoded.height).ok()?;
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [width, height],
        &decoded.rgba,
    ))
}

#[cfg(not(windows))]
fn platform_executable_icon(_: &Path, _: i32, _: usize) -> Option<egui::ColorImage> {
    None
}

fn forced_image_index(toolbar: &UserToolbar, button_index: usize) -> Option<u16> {
    let (all, first) = toolbar
        .global_options
        .iter()
        .find_map(|option| match option {
            lm_app::UserToolbarGlobalOption::ForceImages { all, first_index } => {
                Some((*all, first_index.unwrap_or(1)))
            }
            _ => None,
        })?;
    let eligible = |button: &UserToolbarButton| {
        !matches!(button.target, UserToolbarTarget::Spacer)
            && (all
                || matches!(button.target, UserToolbarTarget::External(_))
                    && !uses_image_list(button))
    };
    let button = toolbar.buttons.get(button_index)?;
    if !eligible(button) {
        return None;
    }
    let preceding = toolbar.buttons[..button_index]
        .iter()
        .filter(|button| eligible(button))
        .count();
    first.checked_add(u16::try_from(preceding).ok()?)
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
            ..UserToolbarImageSet::default()
        };
        let context = egui::Context::default();
        set.ensure_textures(&context);
        let toolbar = UserToolbar::parse(
            "LM_ADDIMAGE \"a.bmp\"\nLM_NEWIMAGE \"b.bmp\"\n***START***\nLM_VIEW_16x16\n-1,test\n***END***",
        )
        .unwrap();
        assert_eq!(
            set.texture_for(&toolbar, 0).unwrap().id(),
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
            set.texture_for(&toolbar, 0).unwrap().id(),
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

    #[test]
    fn custom_main_toolbar_requires_the_original_41_cell_shape() {
        let directory = tempfile::tempdir().unwrap();
        let good = lm_render::Canvas::try_new(41 * 3, 3).unwrap();
        fs::write(
            directory.path().join("Lunar Magic.ff4"),
            lm_render::encode_bmp(&good).unwrap(),
        )
        .unwrap();
        let loaded = MainToolbarImageSet::load(directory.path()).unwrap();
        assert_eq!(loaded.images.len(), 41);
        assert_eq!(loaded.icon_size, Some(3));
        let bad = lm_render::Canvas::try_new(40 * 3, 3).unwrap();
        fs::write(
            directory.path().join("Lunar Magic.ff4"),
            lm_render::encode_bmp(&bad).unwrap(),
        )
        .unwrap();
        assert!(MainToolbarImageSet::load(directory.path()).is_err());
    }

    #[test]
    fn every_authenticated_editor_strip_and_tiled_gui_image_loads_at_its_exact_shape() {
        let directory = tempfile::tempdir().unwrap();
        for kind in OriginalToolbarImages::ALL {
            let (filename, count) = kind.specification();
            let canvas = lm_render::Canvas::try_new(count * 3, 3).unwrap();
            fs::write(
                directory.path().join(filename),
                lm_render::encode_bmp(&canvas).unwrap(),
            )
            .unwrap();
        }
        for kind in OriginalTiledImage::ALL {
            let canvas = lm_render::Canvas::try_new(3, 5).unwrap();
            fs::write(
                directory.path().join(kind.filename()),
                lm_render::encode_bmp(&canvas).unwrap(),
            )
            .unwrap();
        }
        let mut loaded = MainToolbarImageSet::load(directory.path()).unwrap();
        assert_eq!(loaded.original_strips.len(), 8);
        for kind in OriginalToolbarImages::ALL {
            let (_, count) = kind.specification();
            let strip = &loaded.original_strips[kind.index()];
            assert_eq!(strip.images.len(), count);
            assert_eq!(strip.icon_size, Some(3));
        }
        assert_eq!(loaded.original_tiles.len(), 5);
        for kind in OriginalTiledImage::ALL {
            assert_eq!(
                loaded.original_tiles[kind.index()]
                    .image
                    .as_ref()
                    .map(|image| image.size),
                Some([3, 5])
            );
        }
        let context = egui::Context::default();
        loaded.ensure_textures(&context);
        assert!(
            loaded
                .original_texture(OriginalToolbarImages::Map16, 19)
                .is_some()
        );
        assert_eq!(loaded.original_icon_size(OriginalToolbarImages::Map16), 3.0);
        assert!(
            loaded
                .tiled_texture(OriginalTiledImage::BackgroundCanvas)
                .is_some()
        );
    }

    #[test]
    fn original_tiled_surfaces_repeat_from_the_valid_content_edge() {
        assert_eq!(
            tiled_surface_canvas_size(egui::vec2(512.0, 256.0), egui::vec2(640.0, 200.0)),
            egui::vec2(640.0, 256.0)
        );
        let target = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(640.0, 480.0));
        let uv = tiled_surface_uv(target, egui::pos2(512.0, 0.0), egui::vec2(32.0, 16.0));
        assert_eq!(uv.min, egui::pos2(-16.0, 0.0));
        assert_eq!(uv.max, egui::pos2(4.0, 30.0));
        assert_eq!(
            egui::TextureOptions::NEAREST_REPEAT.wrap_mode,
            egui::TextureWrapMode::Repeat
        );
    }

    #[test]
    fn authenticated_editor_action_cells_match_the_decompiled_command_tables() {
        use OriginalToolbarAction::{Redo, Save, Undo};

        assert_eq!(OriginalToolbarImages::Overworld.action_index(Save), Some(1));
        assert_eq!(OriginalToolbarImages::Overworld.action_index(Undo), Some(2));
        assert_eq!(OriginalToolbarImages::Overworld.action_index(Redo), Some(3));
        assert_eq!(OriginalToolbarImages::Map16.action_index(Save), Some(2));
        assert_eq!(OriginalToolbarImages::Map16.action_index(Undo), Some(3));
        assert_eq!(OriginalToolbarImages::Map16.action_index(Redo), Some(4));
        assert_eq!(
            OriginalToolbarImages::LevelPalette.action_index(Save),
            Some(11)
        );
        assert_eq!(
            OriginalToolbarImages::LevelPalette.action_index(Undo),
            Some(2)
        );
        assert_eq!(
            OriginalToolbarImages::LevelPalette.action_index(Redo),
            Some(3)
        );
        assert_eq!(OriginalToolbarImages::AddObject.action_index(Save), None);
    }

    #[test]
    fn authenticated_catalog_action_cells_match_the_decompiled_command_tables() {
        use OriginalCatalogAction::{
            CompatibleGraphicsOnly, PreviewArea, PreviewIcons, VerticalLayout, Zoom,
        };

        for kind in [
            OriginalToolbarImages::AddObject,
            OriginalToolbarImages::AddSprite,
        ] {
            assert_eq!(kind.catalog_action_index(PreviewIcons), Some(1));
            assert_eq!(kind.catalog_action_index(CompatibleGraphicsOnly), Some(2));
            assert_eq!(kind.catalog_action_index(VerticalLayout), Some(3));
            assert_eq!(kind.catalog_action_index(Zoom), Some(4));
            assert_eq!(kind.catalog_action_index(PreviewArea), Some(5));
        }
        assert_eq!(
            OriginalToolbarImages::Map16.catalog_action_index(PreviewIcons),
            None
        );
    }

    #[test]
    fn malformed_authenticated_editor_strip_rejects_without_publishing_any_set() {
        let directory = tempfile::tempdir().unwrap();
        let bad = lm_render::Canvas::try_new(31 * 4, 4).unwrap();
        fs::write(
            directory.path().join("Lunar Magic.ff2"),
            lm_render::encode_bmp(&bad).unwrap(),
        )
        .unwrap();
        assert!(MainToolbarImageSet::load(directory.path()).is_err());
    }

    #[test]
    fn force_image_modes_assign_sequential_global_indexes_to_exact_eligible_buttons() {
        let external = UserToolbar::parse(
            "LM_USEIMAGE_FORCE 2\n***START***\n\"one.exe\"\n0,one\n***START***\nLM_VIEW_16x16\n0,internal\n***START***\n\"two.exe\"\n0,two\n***END***",
        )
        .unwrap();
        assert_eq!(forced_image_index(&external, 0), Some(2));
        assert_eq!(forced_image_index(&external, 1), None);
        assert_eq!(forced_image_index(&external, 2), Some(3));
        let all = UserToolbar::parse(
            "LM_USEIMAGE_FORCE_ALL 4\n***START***\nLM_VIEW_16x16\n0,one\n***START***\nLM_SPACER\n***START***\n\"two.exe\"\n0,two\n***END***",
        )
        .unwrap();
        assert_eq!(forced_image_index(&all, 0), Some(4));
        assert_eq!(forced_image_index(&all, 1), None);
        assert_eq!(forced_image_index(&all, 2), Some(5));
    }

    #[test]
    fn external_buttons_extract_the_requested_executable_icon_only_without_image_overrides() {
        let directory = tempfile::tempdir().unwrap();
        let toolbar = UserToolbar::parse(
            "***START***\n\"tool one.exe\" argument\n3,external\n***START***\nLM_VIEW_16x16\n0,internal\n***START***\ntool-two.exe\n2,list\nLM_USEIMAGE_LIST\n***END***",
        )
        .unwrap();
        let mut requests = Vec::new();
        let marker = egui::ColorImage::new([16, 16], egui::Color32::RED);
        let mut loaded = UserToolbarImageSet::load_with_icon_extractor(
            directory.path(),
            &toolbar,
            |path, icon, size| {
                requests.push((path.to_owned(), icon, size));
                Some(marker.clone())
            },
        )
        .unwrap();
        assert_eq!(
            requests,
            vec![(directory.path().join("tool one.exe"), 3, 16)]
        );
        let context = egui::Context::default();
        loaded.ensure_textures(&context);
        assert!(loaded.texture_for(&toolbar, 0).is_some());
        assert!(loaded.texture_for(&toolbar, 1).is_none());
        assert!(loaded.texture_for(&toolbar, 2).is_none());
    }

    #[test]
    fn forced_images_suppress_executable_extraction_and_percent_four_resolves() {
        let directory = tempfile::tempdir().unwrap();
        let toolbar =
            UserToolbar::parse("LM_USEIMAGE_FORCE 1\n***START***\n%4tool.exe\n0,forced\n***END***")
                .unwrap();
        let mut called = false;
        UserToolbarImageSet::load_with_icon_extractor(directory.path(), &toolbar, |_, _, _| {
            called = true;
            None
        })
        .unwrap();
        assert!(!called);

        let plain = UserToolbar::parse("***START***\n%4tool.exe\nLM_DEFAULT\n***END***").unwrap();
        let mut path = None;
        UserToolbarImageSet::load_with_icon_extractor(
            directory.path(),
            &plain,
            |value, icon, _| {
                path = Some((value.to_owned(), icon));
                None
            },
        )
        .unwrap();
        assert_eq!(path, Some((directory.path().join("tool.exe"), 0)));
    }
}
