use super::{Map16SidecarKind, PendingSidecarExport, RomMap16Editor};
use crate::{document_loader::BoundedRead, persistence_worker::PersistenceTarget};
use eframe::egui;
use lm_app::NativeMap16SidecarDocument;
use lm_level::{M16Sidecar, S16Sidecar};
use std::path::{Path, PathBuf};

impl RomMap16Editor {
    pub(super) fn initialize_associated_sidecars(&mut self, document_path: Option<PathBuf>) {
        self.associated_m16 =
            M16Sidecar::decode(include_bytes!("../assets/lm363-default-m16.bin")).ok();
        self.associated_s16 = S16Sidecar::decode(&[]).ok();
        self.associated_sidecar_paths = document_path.map(|path| {
            let m16 = path.with_extension("m16");
            let s16 = path.with_extension("s16");
            (m16, s16)
        });
        self.pending_sidecar_export = None;
        self.sidecar_export_in_flight = None;
        let Some((m16, s16)) = self.associated_sidecar_paths.clone() else {
            return;
        };
        if let Err(error) = self.associated_sidecar_loader.start(vec![
            BoundedRead::optional(
                m16,
                M16Sidecar::ENCODED_LEN as u64,
                "associated .m16 sidecar",
            ),
            BoundedRead::optional(s16, S16Sidecar::CAPACITY as u64, "associated .s16 sidecar"),
        ]) {
            self.error = Some(error);
        }
    }

    pub(super) fn poll_associated_sidecar_io(&mut self, context: &egui::Context) {
        if let Some(result) = self.associated_sidecar_loader.show(context) {
            let result = result.and_then(|loaded| {
                let paths = self
                    .associated_sidecar_paths
                    .as_ref()
                    .ok_or("associated Map16 sidecar paths are missing")?;
                for (path, bytes) in loaded.files {
                    if path == paths.0 {
                        self.associated_m16 =
                            Some(M16Sidecar::decode(&bytes).map_err(|error| error.to_string())?);
                    } else if path == paths.1 {
                        self.associated_s16 =
                            Some(S16Sidecar::decode(&bytes).map_err(|error| error.to_string())?);
                    } else {
                        return Err(format!(
                            "associated Map16 loader returned unexpected path {}",
                            path.display()
                        ));
                    }
                }
                Ok(())
            });
            if let Err(error) = result {
                self.error = Some(error);
            }
        }
        if let Some(completion) = self.associated_sidecar_persistence.show(context) {
            let in_flight = self.sidecar_export_in_flight.take();
            match completion.result {
                Err(error) => self.error = Some(error),
                Ok(()) => {
                    if let Some((kind, bytes)) = in_flight
                        && let Err(error) = self.install_exported_sidecar(kind, &bytes)
                    {
                        self.error = Some(error);
                    }
                }
            }
        }
    }

    pub(super) fn sidecar_export_controls(
        &mut self,
        ui: &mut egui::Ui,
        blocked: bool,
        project_revision: u64,
        active_sidecar: Option<&NativeMap16SidecarDocument>,
    ) {
        let shortcut = take_map16_sidecar_export_shortcut(ui);
        ui.horizontal_wrapped(|ui| {
            ui.label("Associated custom Map16");
            for (kind, label, shortcut_text) in [
                (Map16SidecarKind::M16, "Export .m16", "Ctrl+F9"),
                (Map16SidecarKind::S16, "Export .s16", "Ctrl+Shift+F9"),
            ] {
                let clicked = ui
                    .add_enabled(
                        !blocked,
                        egui::Button::new(label).shortcut_text(shortcut_text),
                    )
                    .clicked();
                if !blocked && (clicked || shortcut == Some(kind)) {
                    if let Err(error) =
                        self.prepare_sidecar_export(kind, project_revision, active_sidecar)
                    {
                        self.error = Some(error);
                    }
                }
            }
        });
    }

    fn prepare_sidecar_export(
        &mut self,
        kind: Map16SidecarKind,
        project_revision: u64,
        active_sidecar: Option<&NativeMap16SidecarDocument>,
    ) -> Result<(), String> {
        if self.pending_sidecar_export.is_some() || self.associated_sidecar_persistence.is_running()
        {
            return Err("an associated Map16 sidecar export is already pending".into());
        }
        let paths = self
            .associated_sidecar_paths
            .as_ref()
            .ok_or("save the ROM before exporting an associated Map16 sidecar")?;
        let (path, bytes) = match kind {
            Map16SidecarKind::M16 => (
                paths.0.clone(),
                match active_sidecar {
                    Some(NativeMap16SidecarDocument::M16(value)) => value.encode(),
                    _ => self
                        .associated_m16
                        .as_ref()
                        .ok_or("the active .m16 buffer is unavailable")?
                        .encode(),
                },
            ),
            Map16SidecarKind::S16 => (
                paths.1.clone(),
                match active_sidecar {
                    Some(NativeMap16SidecarDocument::S16(value)) => value.encode_canonical(),
                    _ => self
                        .associated_s16
                        .as_ref()
                        .ok_or("the active .s16 buffer is unavailable")?
                        .encode_canonical(),
                },
            ),
        };
        self.pending_sidecar_export = Some(PendingSidecarExport {
            kind,
            path,
            bytes,
            revision: project_revision,
        });
        Ok(())
    }

    pub(super) fn sidecar_export_confirmation(&mut self, context: &egui::Context) {
        let Some(pending) = self.pending_sidecar_export.as_ref() else {
            return;
        };
        let kind = pending.kind;
        let path = pending.path.clone();
        egui::Window::new("Export associated Map16 sidecar?")
            .collapsible(false)
            .resizable(false)
            .show(context, |ui| {
                ui.label(format!(
                    "Write the current {} buffer to {}?",
                    sidecar_extension(kind),
                    path.display()
                ));
                ui.horizontal(|ui| {
                    if ui.button("No").clicked() {
                        self.pending_sidecar_export = None;
                    }
                    if ui.button("Yes").clicked()
                        && let Some(pending) = self.pending_sidecar_export.take()
                    {
                        match sidecar_persistence_target(&pending.path).and_then(|target| {
                            self.associated_sidecar_persistence.start(
                                pending.revision,
                                target,
                                pending.bytes.clone(),
                            )
                        }) {
                            Ok(()) => {
                                self.sidecar_export_in_flight = Some((pending.kind, pending.bytes));
                            }
                            Err(error) => self.error = Some(error),
                        }
                    }
                });
            });
    }

    fn install_exported_sidecar(
        &mut self,
        kind: Map16SidecarKind,
        bytes: &[u8],
    ) -> Result<(), String> {
        match kind {
            Map16SidecarKind::M16 => {
                self.associated_m16 =
                    Some(M16Sidecar::decode(bytes).map_err(|error| error.to_string())?);
            }
            Map16SidecarKind::S16 => {
                self.associated_s16 =
                    Some(S16Sidecar::decode(bytes).map_err(|error| error.to_string())?);
            }
        }
        Ok(())
    }
}

fn take_map16_sidecar_export_shortcut(ui: &mut egui::Ui) -> Option<Map16SidecarKind> {
    ui.input_mut(|input| {
        let modifiers = input.modifiers;
        if !modifiers.ctrl || !input.consume_key(modifiers, egui::Key::F9) {
            None
        } else if modifiers.shift {
            Some(Map16SidecarKind::S16)
        } else {
            Some(Map16SidecarKind::M16)
        }
    })
}

fn sidecar_extension(kind: Map16SidecarKind) -> &'static str {
    match kind {
        Map16SidecarKind::M16 => ".m16",
        Map16SidecarKind::S16 => ".s16",
    }
}

fn sidecar_persistence_target(path: &Path) -> Result<PersistenceTarget, String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            Ok(PersistenceTarget::Replace(path.to_path_buf()))
        }
        Ok(_) => Err(format!(
            "associated Map16 sidecar target must be a regular file: {}",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(PersistenceTarget::Create(path.to_path_buf()))
        }
        Err(error) => Err(format!(
            "could not inspect associated Map16 sidecar {}: {error}",
            path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn temporary_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "lm-map16-sidecar-export-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn observed_shortcut(modifiers: egui::Modifiers) -> Option<Map16SidecarKind> {
        let context = egui::Context::default();
        let mut shortcut = None;
        let _ = context.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::F9,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                modifiers,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    shortcut = take_map16_sidecar_export_shortcut(ui);
                });
            },
        );
        shortcut
    }

    #[test]
    fn original_f9_sidecar_chords_split_m16_and_s16_with_alt_ignored() {
        assert_eq!(
            observed_shortcut(egui::Modifiers::CTRL),
            Some(Map16SidecarKind::M16)
        );
        assert_eq!(
            observed_shortcut(egui::Modifiers::CTRL | egui::Modifiers::ALT),
            Some(Map16SidecarKind::M16)
        );
        assert_eq!(
            observed_shortcut(egui::Modifiers::CTRL | egui::Modifiers::SHIFT),
            Some(Map16SidecarKind::S16)
        );
        assert_eq!(
            observed_shortcut(
                egui::Modifiers::CTRL | egui::Modifiers::SHIFT | egui::Modifiers::ALT
            ),
            Some(Map16SidecarKind::S16)
        );
        assert_eq!(observed_shortcut(egui::Modifiers::NONE), None);
        assert_eq!(observed_shortcut(egui::Modifiers::SHIFT), None);
    }

    #[test]
    fn sidecar_export_snapshots_matching_active_documents_and_exact_sibling_paths() {
        let rom = temporary_path("game.smc");
        let mut editor = RomMap16Editor::default();
        editor.associated_sidecar_paths =
            Some((rom.with_extension("m16"), rom.with_extension("s16")));
        editor.associated_m16 =
            Some(M16Sidecar::decode(&vec![0; M16Sidecar::ENCODED_LEN]).unwrap());
        editor.associated_s16 = Some(S16Sidecar::decode(&[]).unwrap());

        let mut m16 = M16Sidecar::decode(&vec![0; M16Sidecar::ENCODED_LEN]).unwrap();
        m16.set_entry(3, 0x1234_5678).unwrap();
        let active = NativeMap16SidecarDocument::M16(m16.clone());
        editor
            .prepare_sidecar_export(Map16SidecarKind::M16, 17, Some(&active))
            .unwrap();
        let pending = editor.pending_sidecar_export.take().unwrap();
        assert_eq!(pending.path, rom.with_extension("m16"));
        assert_eq!(pending.revision, 17);
        assert_eq!(pending.bytes, m16.encode());

        let mut s16 = S16Sidecar::decode(&[]).unwrap();
        s16.set_entry(0x401, 0x8765_4321).unwrap();
        let active = NativeMap16SidecarDocument::S16(s16.clone());
        editor
            .prepare_sidecar_export(Map16SidecarKind::S16, 18, Some(&active))
            .unwrap();
        let pending = editor.pending_sidecar_export.take().unwrap();
        assert_eq!(pending.path, rom.with_extension("s16"));
        assert_eq!(pending.revision, 18);
        assert_eq!(pending.bytes, s16.encode_canonical());
        assert_eq!(pending.bytes.len() % S16Sidecar::BLOCK_LEN, 0);
    }

    #[test]
    fn associated_sidecar_target_creates_or_atomically_replaces_only_regular_files() {
        let path = temporary_path("target.m16");
        assert_eq!(
            sidecar_persistence_target(&path).unwrap(),
            PersistenceTarget::Create(path.clone())
        );
        fs::write(&path, [1, 2, 3]).unwrap();
        assert_eq!(
            sidecar_persistence_target(&path).unwrap(),
            PersistenceTarget::Replace(path.clone())
        );
        fs::remove_file(&path).unwrap();

        fs::create_dir(&path).unwrap();
        assert!(sidecar_persistence_target(&path).is_err());
        fs::remove_dir(path).unwrap();
    }

    #[test]
    fn default_associated_sidecar_buffers_match_original_shapes() {
        let mut editor = RomMap16Editor::default();
        editor.initialize_associated_sidecars(None);
        assert_eq!(
            editor.associated_m16.as_ref().unwrap().encode().len(),
            M16Sidecar::ENCODED_LEN
        );
        assert_eq!(
            editor
                .associated_s16
                .as_ref()
                .unwrap()
                .encode_canonical()
                .len(),
            S16Sidecar::BLOCK_LEN
        );
        assert!(editor.associated_sidecar_paths.is_none());
    }
}
