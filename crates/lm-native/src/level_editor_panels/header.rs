use eframe::egui;
use lm_app::CompleteLevelDocumentEdit;
use lm_level::{CompleteLevelFile, Layer1VerticalScrollMode, LegacyHeaderEdit, LevelPropertyEdit};

pub(super) fn show_header(
    ui: &mut egui::Ui,
    level: &CompleteLevelFile,
) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
    let header = level.0.header.legacy;
    let mut level_number = level.0.number;
    let mut sprite_header =
        crate::native_level_document_form::NativeSpriteHeaderForm::load(level.0.sprites.header);
    let mut values = [
        header.background_palette(),
        header.last_screen(),
        header.level_mode(),
        header.background_color(),
        header.sprite_tileset(),
        header.default_music_selector(),
        header.time_limit_selector(),
        header.sprite_palette(),
        header.foreground_palette(),
        header.object_tileset(),
    ];
    let labels = [
        "Background palette",
        "Last screen",
        "Level mode",
        "Background color",
        "Sprite tileset",
        "Default music selector",
        "Time limit selector",
        "Sprite palette",
        "Foreground palette",
        "Object tileset",
    ];
    let mut changed = false;
    let mut layer1_vertical_scroll = header.layer1_vertical_scroll().raw();
    changed |= ui
        .add(egui::Slider::new(&mut level_number, 0..=u16::MAX).text("Level number"))
        .changed();
    changed |= crate::native_level_document_form::show_sprite_header_form(
        ui,
        "complete-level-sprite-header",
        &mut sprite_header,
    );
    for (index, value) in values.iter_mut().enumerate() {
        let maximum = match index {
            1 | 2 => 31,
            4 | 9 => 15,
            6 => 3,
            _ => 7,
        };
        changed |= ui
            .add(egui::Slider::new(value, 0..=maximum).text(labels[index]))
            .changed();
    }
    changed |= ui
        .add(egui::Slider::new(&mut layer1_vertical_scroll, 0..=3).text("Layer 1 vertical scroll"))
        .changed();
    changed.then(|| {
        let sprite_header = sprite_header.header().map_err(|error| error.to_string())?;
        let header_edits = [
            LegacyHeaderEdit::BackgroundPalette(values[0]),
            LegacyHeaderEdit::LastScreen(values[1]),
            LegacyHeaderEdit::LevelMode(values[2]),
            LegacyHeaderEdit::BackgroundColor(values[3]),
            LegacyHeaderEdit::SpriteTileset(values[4]),
            LegacyHeaderEdit::DefaultMusicSelector(values[5]),
            LegacyHeaderEdit::TimeLimitSelector(values[6]),
            LegacyHeaderEdit::SpritePalette(values[7]),
            LegacyHeaderEdit::ForegroundPalette(values[8]),
            LegacyHeaderEdit::ObjectTileset(values[9]),
            LegacyHeaderEdit::Layer1VerticalScroll(Layer1VerticalScrollMode::from_raw(
                layer1_vertical_scroll,
            )),
        ];
        let mut edits = vec![
            CompleteLevelDocumentEdit::Property(LevelPropertyEdit::SetLevelNumber(level_number)),
            CompleteLevelDocumentEdit::Property(LevelPropertyEdit::SetSpriteHeader(sprite_header)),
        ];
        edits.extend(header_edits.into_iter().map(|edit| {
            CompleteLevelDocumentEdit::Property(LevelPropertyEdit::LegacyHeader(edit))
        }));
        Ok(edits)
    })
}
