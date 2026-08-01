use super::RomOverworldEditor;
use crate::{dialogs, document_loader::BoundedRead, persistence_worker::PersistenceTarget};
use eframe::egui;
use lm_project::CompleteOverworldFile;

impl RomOverworldEditor {
    pub(super) fn poll_transfer_file_io(&mut self, context: &egui::Context, revision: u64) {
        if let Some(result) = self.transfer_loader.show(context) {
            let result = result.and_then(|loaded| {
                let [(_, bytes)] = loaded.into_exact::<1>("complete overworld")?;
                let workspace = self
                    .workspace
                    .as_mut()
                    .ok_or("overworld workspace is closed")?;
                if workspace.controller.revision() != revision {
                    return Err("the ROM changed while the complete overworld was loading".into());
                }
                let profile = &workspace.profiled.profile;
                let file = decode_complete_file(
                    &bytes,
                    profile.overworld.animation.maximum_records,
                    &profile.exanimation_double_size_modes,
                )?;
                workspace
                    .controller
                    .replace_complete_file(&file, profile.overworld_shape)
                    .map_err(|error| error.to_string())?;
                self.invalidate();
                Ok(())
            });
            if let Err(error) = result {
                self.error = Some(error);
            }
        }
        if let Some(completion) = self.transfer_persistence.show(context)
            && let Err(error) = completion.result
        {
            self.error = Some(error);
        }
    }

    pub(super) fn complete_file_controls(&mut self, ui: &mut egui::Ui, stale: bool, revision: u64) {
        let busy = self.transfer_loader.is_running() || self.transfer_persistence.is_running();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Import complete .lmow…"))
                .clicked()
                && let Some(path) = dialogs::choose_complete_overworld_document()
                && let Err(error) = self.transfer_loader.start(vec![BoundedRead::new(
                    path,
                    CompleteOverworldFile::MAX_FILE_LEN as u64,
                    "complete overworld file",
                )])
            {
                self.error = Some(error);
            }
            if ui
                .add_enabled(!stale && !busy, egui::Button::new("Export complete .lmow…"))
                .clicked()
            {
                self.start_complete_export(revision);
            }
        });
        ui.small(
            "Complete transfer stages or exports all nine modeled overworld domains together.",
        );
    }

    fn start_complete_export(&mut self, revision: u64) {
        let Some(workspace) = self.workspace.as_ref() else {
            self.error = Some("overworld workspace is closed".into());
            return;
        };
        let Some(path) = dialogs::choose_complete_overworld_save_path(workspace.slot) else {
            return;
        };
        let profile = &workspace.profiled.profile;
        let file = CompleteOverworldFile {
            source_slot: workspace.slot,
            shape: profile.overworld_shape,
            data: workspace.controller.data().clone(),
        };
        match encode_complete_file(&file, &profile.exanimation_double_size_modes) {
            Ok(bytes) => {
                if let Err(error) = self.transfer_persistence.start(
                    revision,
                    PersistenceTarget::Create(path),
                    bytes,
                ) {
                    self.error = Some(error);
                }
            }
            Err(error) => self.error = Some(error),
        }
    }
}

fn decode_complete_file(
    bytes: &[u8],
    maximum_animation_records: usize,
    double_size_modes: &[bool],
) -> Result<CompleteOverworldFile, String> {
    CompleteOverworldFile::decode(bytes, maximum_animation_records, double_size_modes)
        .map_err(|error| error.to_string())
}

fn encode_complete_file(
    file: &CompleteOverworldFile,
    double_size_modes: &[bool],
) -> Result<Vec<u8>, String> {
    file.encode(double_size_modes)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_graphics::{Bgr555, CompactExAnimation, Palette};
    use lm_overworld::{
        EventReveal, EventRevealTable, OverworldEndpoint, OverworldLayer, OverworldMessage,
        OverworldSprite, Submap,
    };
    use lm_project::{CompleteOverworldData, CompleteOverworldShape, OverworldLayers};

    const MODES: [bool; 256] = [false; 256];

    fn file() -> CompleteOverworldFile {
        CompleteOverworldFile {
            source_slot: 0x1ff,
            shape: CompleteOverworldShape {
                width: 1,
                height: 1,
                event_reveals: 1,
                endpoints: 1,
                messages: 1,
                sprites: 1,
                sprite_record_len: 7,
                palette_colors: 2,
            },
            data: CompleteOverworldData {
                layers: OverworldLayers {
                    layer1: OverworldLayer::new(1, 1, vec![1]).unwrap(),
                    layer2: OverworldLayer::new(1, 1, vec![2]).unwrap(),
                },
                event_reveals: EventRevealTable {
                    entries: vec![EventReveal {
                        source_tile: 3,
                        destination_tile: 4,
                    }],
                },
                endpoints: vec![OverworldEndpoint {
                    x: 5,
                    y: 6,
                    submap: 0,
                }],
                messages: vec![
                    OverworldMessage::decode(&[7; OverworldMessage::ENCODED_LEN]).unwrap(),
                ],
                sprites: vec![OverworldSprite {
                    id: 8,
                    x: 9,
                    y: 10,
                    submap: Submap::Main,
                    extra: Vec::new(),
                }],
                palette: Palette {
                    colors: vec![Bgr555(0), Bgr555(1)],
                },
                animation: CompactExAnimation {
                    setting: 0,
                    header_value: 0,
                    trigger_mask: 0,
                    trigger_values: [0; 16],
                    records: Vec::new(),
                },
            },
        }
    }

    #[test]
    fn native_transfer_helpers_round_trip_all_nine_domains() {
        let expected = file();
        let bytes = encode_complete_file(&expected, &MODES).unwrap();
        assert_eq!(decode_complete_file(&bytes, 32, &MODES).unwrap(), expected);
        assert!(decode_complete_file(&bytes[..bytes.len() - 1], 32, &MODES).is_err());
    }
}
