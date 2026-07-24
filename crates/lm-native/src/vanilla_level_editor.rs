use eframe::egui;
use lm_app::{
    AppState, Command, EditorMode, LevelController, NativeLevelEdit, RomExpansionCommand,
};
use lm_level::{
    LegacyHeaderEdit, ObjectCoordinateNibbles, ObjectEdit, ObjectRecord, SpriteLengthTable,
};
use lm_project::LevelSaveOptions;
use lm_rats::{AllocationPolicy, ProtectedRange};
use lm_rom::{Mapper, Region, RomImage, SupportedGame};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EditorKey {
    revision: u64,
    level: u16,
}

#[derive(Clone, Copy, Debug, Default)]
struct HeaderForm {
    background_palette: u8,
    level_mode: u8,
    background_color: u8,
    sprite_palette: u8,
    foreground_palette: u8,
    object_tileset: u8,
}

impl HeaderForm {
    fn from_controller(controller: &LevelController) -> Self {
        let header = controller.level().layer1.header;
        Self {
            background_palette: header.background_palette(),
            level_mode: header.level_mode(),
            background_color: header.background_color(),
            sprite_palette: header.sprite_palette(),
            foreground_palette: header.foreground_palette(),
            object_tileset: header.object_tileset(),
        }
    }

    fn edits(self) -> [NativeLevelEdit; 6] {
        [
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::BackgroundPalette(
                self.background_palette,
            )),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::LevelMode(self.level_mode)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::BackgroundColor(self.background_color)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::SpritePalette(self.sprite_palette)),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::ForegroundPalette(
                self.foreground_palette,
            )),
            NativeLevelEdit::LegacyHeader(LegacyHeaderEdit::ObjectTileset(self.object_tileset)),
        ]
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ObjectForm {
    command_id: u8,
    parameter: u8,
    first_coordinate: u8,
    second_coordinate: u8,
    advances_screen: bool,
}

impl ObjectForm {
    fn from_record(record: &ObjectRecord) -> Self {
        let coordinates = record.coordinate_nibbles();
        Self {
            command_id: record.command_id(),
            parameter: record.parameter(),
            first_coordinate: coordinates.first,
            second_coordinate: coordinates.second,
            advances_screen: record.advances_screen(),
        }
    }
}

#[derive(Default)]
pub(crate) struct VanillaLevelEditor {
    key: Option<EditorKey>,
    controller: Option<LevelController>,
    form: HeaderForm,
    selected_object: usize,
    object_form: ObjectForm,
    error: Option<String>,
    map16_key: Option<(u64, u8)>,
    map16_texture: Option<egui::TextureHandle>,
    map16_summary: Option<([usize; 4], usize, usize)>,
    map16_error: Option<String>,
}

impl VanillaLevelEditor {
    pub(crate) fn handles(app: &AppState) -> bool {
        app.revision_profile().is_none()
            && app.controller_snapshot().is_ok_and(|snapshot| {
                matches!(snapshot.mode, EditorMode::Level(_)) && is_supported(&snapshot)
            })
    }

    pub(crate) fn show(&mut self, ui: &mut egui::Ui, app: &AppState) -> Option<Command> {
        let snapshot = app.controller_snapshot().ok()?;
        let EditorMode::Level(level) = snapshot.mode else {
            self.clear();
            return None;
        };
        if !is_supported(&snapshot) || app.revision_profile().is_some() {
            self.clear();
            return None;
        }
        let key = EditorKey {
            revision: snapshot.revision,
            level,
        };
        if self.key != Some(key) {
            self.load(&snapshot, key);
        }

        ui.heading(format!("Level {level:03X} — built-in SMW editor"));
        ui.label("Pristine SMW-US layout detected automatically.");
        ui.separator();
        let Some(controller) = self.controller.as_ref() else {
            ui.colored_label(
                egui::Color32::RED,
                self.error.as_deref().unwrap_or("load failed"),
            );
            return None;
        };
        let object_count = controller.level().layer1.objects.records.len();
        let sprite_count = controller.level().sprites.tokens.len();
        let object_tileset = controller.level().layer1.header.object_tileset();
        self.show_header_editor(ui, object_count, sprite_count);
        ui.separator();
        self.show_map16_preview(ui, &snapshot, object_tileset);
        ui.separator();
        self.object_canvas(ui);
        ui.separator();
        ui.columns(2, |columns| {
            self.object_list(&mut columns[0]);
            self.object_editor(&mut columns[1]);
        });
        ui.add_space(8.0);
        let expanded = snapshot.rom_bytes.len() > 0x80_000;
        if !expanded {
            ui.label("Level relocation needs one expanded free-space bank.");
            if ui.button("Expand ROM to 1 MiB").clicked() {
                return Some(Command::ExpandRom(RomExpansionCommand {
                    expected_revision: snapshot.revision,
                    mapper: snapshot.identity.mapper,
                    target_logical_len: 0x10_0000,
                    fill: 0xff,
                    checksum_field: snapshot.identity.internal_header_offset + 0x1c,
                }));
            }
        }
        if ui
            .add_enabled(
                expanded
                    && self
                        .controller
                        .as_ref()
                        .is_some_and(LevelController::is_modified),
                egui::Button::new("Commit level changes to ROM"),
            )
            .clicked()
        {
            match prepare_commit(
                self.controller
                    .as_ref()
                    .expect("controller presence checked above"),
                &snapshot,
            ) {
                Ok(command) => return Some(command),
                Err(error) => self.error = Some(error),
            }
        }
        None
    }

    fn show_header_editor(&mut self, ui: &mut egui::Ui, objects: usize, sprites: usize) {
        ui.label(format!("{objects} objects, {sprites} sprite records"));
        egui::Grid::new("vanilla-level-header").show(ui, |ui| {
            header_row(ui, "Level mode", &mut self.form.level_mode, 31);
            header_row(
                ui,
                "Background palette",
                &mut self.form.background_palette,
                7,
            );
            header_row(ui, "Background color", &mut self.form.background_color, 7);
            header_row(
                ui,
                "Foreground palette",
                &mut self.form.foreground_palette,
                7,
            );
            header_row(ui, "Sprite palette", &mut self.form.sprite_palette, 7);
            header_row(ui, "Object tileset", &mut self.form.object_tileset, 15);
        });
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::RED, error);
        }
        ui.horizontal(|ui| {
            if ui.button("Stage header changes").clicked() {
                match self
                    .controller
                    .as_mut()
                    .expect("controller presence checked above")
                    .apply_edits(&self.form.edits())
                {
                    Ok(()) => self.error = None,
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            if ui.button("Reset staged values").clicked() {
                self.form = HeaderForm::from_controller(
                    self.controller
                        .as_ref()
                        .expect("controller presence checked above"),
                );
                self.error = None;
            }
        });
    }

    fn load(&mut self, snapshot: &lm_app::ControllerSnapshot, key: EditorKey) {
        match LevelController::decode(
            snapshot,
            lm_profile::smw_us_v1_vanilla_level_layout(),
            &SpriteLengthTable::standard(),
        ) {
            Ok(controller) => {
                self.form = HeaderForm::from_controller(&controller);
                self.selected_object = 0;
                self.object_form = controller
                    .level()
                    .layer1
                    .objects
                    .records
                    .first()
                    .map_or_else(ObjectForm::default, ObjectForm::from_record);
                self.controller = Some(controller);
                self.error = None;
            }
            Err(error) => {
                self.controller = None;
                self.error = Some(error.to_string());
            }
        }
        self.key = Some(key);
    }

    fn clear(&mut self) {
        self.key = None;
        self.controller = None;
        self.error = None;
        self.map16_key = None;
        self.map16_texture = None;
        self.map16_summary = None;
        self.map16_error = None;
    }

    fn show_map16_preview(
        &mut self,
        ui: &mut egui::Ui,
        snapshot: &lm_app::ControllerSnapshot,
        object_tileset: u8,
    ) {
        egui::CollapsingHeader::new(format!(
            "Pristine Map16 graphics — object tileset {object_tileset:X}"
        ))
        .default_open(false)
        .show(ui, |ui| {
            let key = (snapshot.revision, object_tileset);
            if self.map16_key != Some(key) {
                self.map16_texture = None;
                self.map16_summary = None;
                self.map16_error = None;
                match crate::vanilla_map16_preview::render(
                    snapshot.rom_bytes.clone(),
                    self.controller
                        .as_ref()
                        .map_or(0, |controller| {
                            u16::try_from(controller.level().number).unwrap_or(0)
                        }),
                    self.controller
                        .as_ref()
                        .map_or_default(|controller| controller.level().layer1.header),
                ) {
                    Ok(preview) => {
                        self.map16_summary = Some((
                            preview.graphics_files,
                            preview.common_tiles,
                            preview.tileset_tiles,
                        ));
                        self.map16_texture = Some(ui.ctx().load_texture(
                            format!("vanilla-map16-{object_tileset:X}-{}", snapshot.revision),
                            preview.image,
                            egui::TextureOptions::NEAREST,
                        ));
                    }
                    Err(error) => self.map16_error = Some(error),
                }
                self.map16_key = Some(key);
            }
            if let Some((files, common, specific)) = self.map16_summary {
                ui.label(format!(
                    "GFX{:02X}/GFX{:02X}/GFX{:02X}/GFX{:02X}; {common} common and {specific} tileset-specific definitions",
                    files[0], files[1], files[2], files[3]
                ));
            }
            if let Some(texture) = &self.map16_texture {
                egui::ScrollArea::horizontal()
                    .id_salt("vanilla-map16-preview")
                    .show(ui, |ui| {
                        ui.image(texture);
                    });
                ui.small(
                    "Decoded 4bpp pixels, level palette, and flip attributes; pink marks tiles outside the four foreground slots.",
                );
            } else if let Some(error) = &self.map16_error {
                ui.colored_label(egui::Color32::RED, error);
            }
        });
    }

    fn object_list(&mut self, ui: &mut egui::Ui) {
        let Some(controller) = &self.controller else {
            return;
        };
        ui.label("Layer 1 objects");
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                for (index, record) in controller.level().layer1.objects.records.iter().enumerate()
                {
                    let encoded = record
                        .encoded()
                        .iter()
                        .map(|byte| format!("{byte:02X}"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    if ui
                        .selectable_label(
                            index == self.selected_object,
                            format!("{index:03}: {encoded}"),
                        )
                        .clicked()
                    {
                        self.selected_object = index;
                        self.object_form = ObjectForm::from_record(record);
                    }
                }
            });
    }

    fn object_canvas(&mut self, ui: &mut egui::Ui) {
        let records = self
            .controller
            .as_ref()
            .map(|controller| controller.level().layer1.objects.records.clone())
            .unwrap_or_default();
        let width = ui.available_width().max(320.0);
        let height = 260.0;
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::from_gray(20));
        let cell = 14.0;
        for x in 0_u8..=16 {
            let x = rect.left() + f32::from(x) * cell;
            painter.line_segment(
                [
                    egui::pos2(x, rect.top()),
                    egui::pos2(x, rect.bottom().min(rect.top() + 16.0 * cell)),
                ],
                egui::Stroke::new(1.0_f32, egui::Color32::from_gray(45)),
            );
        }
        for y in 0_u8..=16 {
            let y = rect.top() + f32::from(y) * cell;
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                egui::Stroke::new(1.0_f32, egui::Color32::from_gray(45)),
            );
        }
        let mut hit = None;
        for (index, record) in records.iter().enumerate() {
            let coordinate = record.coordinate_nibbles();
            let position = rect.min
                + egui::vec2(
                    f32::from(coordinate.first) * cell,
                    f32::from(coordinate.second) * cell,
                );
            let object_rect =
                egui::Rect::from_min_size(position, egui::vec2(cell.max(8.0), cell.max(8.0)));
            let selected = index == self.selected_object;
            painter.rect_filled(
                object_rect.shrink(1.0),
                1.0,
                if selected {
                    egui::Color32::YELLOW
                } else {
                    egui::Color32::from_rgb(80, 170, 230)
                },
            );
            painter.text(
                object_rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("{:X}", record.command_id()),
                egui::FontId::monospace(8.0),
                egui::Color32::BLACK,
            );
            if response
                .interact_pointer_pos()
                .is_some_and(|position| object_rect.contains(position))
            {
                hit = Some(index);
            }
        }
        if response.clicked()
            && let Some(index) = hit
            && let Some(record) = records.get(index)
        {
            self.selected_object = index;
            self.object_form = ObjectForm::from_record(record);
        }
        ui.label(
            "Schematic native-object coordinates (raw orientation-neutral nibbles; command ID shown).",
        );
    }

    fn object_editor(&mut self, ui: &mut egui::Ui) {
        let record_count = self.controller.as_ref().map_or(0, |controller| {
            controller.level().layer1.objects.records.len()
        });
        if self.selected_object >= record_count {
            ui.label("No selected object.");
            return;
        }
        ui.label(format!("Object {}", self.selected_object));
        egui::Grid::new("vanilla-object-fields").show(ui, |ui| {
            header_row(ui, "Command", &mut self.object_form.command_id, 0x3f);
            header_row(ui, "Parameter", &mut self.object_form.parameter, 0xff);
            header_row(
                ui,
                "Coordinate A",
                &mut self.object_form.first_coordinate,
                0x0f,
            );
            header_row(
                ui,
                "Coordinate B",
                &mut self.object_form.second_coordinate,
                0x0f,
            );
            ui.label("Advance screen");
            ui.checkbox(&mut self.object_form.advances_screen, "");
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.button("Apply object fields").clicked() {
                let edits = vec![
                    ObjectEdit::SetCommandId {
                        index: self.selected_object,
                        command_id: self.object_form.command_id,
                    },
                    ObjectEdit::SetParameter {
                        index: self.selected_object,
                        parameter: self.object_form.parameter,
                    },
                    ObjectEdit::SetCoordinateNibbles {
                        index: self.selected_object,
                        coordinates: ObjectCoordinateNibbles {
                            first: self.object_form.first_coordinate,
                            second: self.object_form.second_coordinate,
                        },
                    },
                    ObjectEdit::SetAdvancesScreen {
                        index: self.selected_object,
                        advances: self.object_form.advances_screen,
                    },
                ];
                if let Some(controller) = self.controller.as_mut() {
                    match controller.apply_edits(&[NativeLevelEdit::Objects(edits)]) {
                        Ok(()) => self.error = None,
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
            }
            if ui.button("Remove object").clicked()
                && let Some(controller) = self.controller.as_mut()
            {
                match controller.apply_edits(&[NativeLevelEdit::Objects(vec![
                    ObjectEdit::Remove {
                        index: self.selected_object,
                    },
                ])]) {
                    Ok(()) => {
                        self.selected_object =
                            self.selected_object.min(record_count.saturating_sub(2));
                        if let Some(record) = controller
                            .level()
                            .layer1
                            .objects
                            .records
                            .get(self.selected_object)
                        {
                            self.object_form = ObjectForm::from_record(record);
                        }
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
        });
    }
}

fn header_row(ui: &mut egui::Ui, label: &str, value: &mut u8, maximum: u8) {
    ui.label(label);
    ui.add(egui::DragValue::new(value).range(0..=maximum));
    ui.end_row();
}

fn is_supported(snapshot: &lm_app::ControllerSnapshot) -> bool {
    snapshot.identity.game == SupportedGame::SuperMarioWorld
        && snapshot.identity.region == Region::NorthAmerica
        && snapshot.identity.revision == 0
        && snapshot.identity.mapper == Mapper::LoRom
}

fn prepare_commit(
    controller: &LevelController,
    snapshot: &lm_app::ControllerSnapshot,
) -> Result<Command, String> {
    let image =
        RomImage::from_bytes(snapshot.rom_bytes.clone()).map_err(|error| error.to_string())?;
    let logical_len = image.logical_len();
    if logical_len <= 0x80_000 {
        return Err("expand the ROM before committing level changes".into());
    }
    let layout = lm_profile::smw_us_v1_vanilla_level_layout();
    let allocation = AllocationPolicy {
        search: 0x80_000..logical_len,
        bank_size: Some(0x8000),
        fill_bytes: vec![0xff],
        protected: vec![
            ProtectedRange(
                layout.layer1.offset
                    ..layout.layer1.offset + layout.layer1.entries * layout.layer1.stride,
            ),
            ProtectedRange(
                snapshot.identity.internal_header_offset
                    ..snapshot.identity.internal_header_offset + 0x40,
            ),
        ],
    };
    controller
        .prepare_commit(
            format!("Edit pristine SMW level {:03X}", controller.level().number),
            &LevelSaveOptions {
                layer1_allocation: allocation.clone(),
                sprite_allocation: allocation,
                previous_layer1: None,
                previous_sprites: None,
                reuse_identical: true,
                erase_fill: 0xff,
            },
        )
        .map(lm_app::PreparedRomCommit::into_command)
        .map_err(|error| error.to_string())
}
