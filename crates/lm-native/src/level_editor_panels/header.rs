use eframe::egui;
use lm_app::CompleteLevelDocumentEdit;
use lm_level::{CompleteLevelFile, LegacyHeaderEdit, LevelPropertyEdit};

pub(super) fn show_header(
    ui: &mut egui::Ui,
    level: &CompleteLevelFile,
) -> Option<Result<Vec<CompleteLevelDocumentEdit>, String>> {
    let header = level.0.header.legacy;
    let mut level_number = level.0.number;
    let mut sprite_header = level.0.sprites.header;
    let mut values = [
        header.background_palette(),
        header.level_mode(),
        header.background_color(),
        header.sprite_palette(),
        header.foreground_palette(),
    ];
    let labels = [
        "Background palette",
        "Level mode",
        "Background color",
        "Sprite palette",
        "Foreground palette",
    ];
    let mut changed = false;
    changed |= ui
        .add(egui::Slider::new(&mut level_number, 0..=u16::MAX).text("Level number"))
        .changed();
    changed |= ui
        .add(egui::Slider::new(&mut sprite_header, 0..=u8::MAX).text("Sprite header"))
        .changed();
    for (index, value) in values.iter_mut().enumerate() {
        let maximum = if index == 1 { 31 } else { 7 };
        changed |= ui
            .add(egui::Slider::new(value, 0..=maximum).text(labels[index]))
            .changed();
    }
    changed.then(|| {
        let header_edits = [
            LegacyHeaderEdit::BackgroundPalette(values[0]),
            LegacyHeaderEdit::LevelMode(values[1]),
            LegacyHeaderEdit::BackgroundColor(values[2]),
            LegacyHeaderEdit::SpritePalette(values[3]),
            LegacyHeaderEdit::ForegroundPalette(values[4]),
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
