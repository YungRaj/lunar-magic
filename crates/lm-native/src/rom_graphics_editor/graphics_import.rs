use eframe::egui;
use lm_app::PreparedRomCommit;
use lm_project::{GraphicsRomLayout, GraphicsSaveOptions};
use lm_rom::RomImage;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};

#[derive(Clone)]
pub(super) struct GraphicsImportSource {
    pub(super) expected_revision: u64,
    pub(super) image: RomImage,
    pub(super) layout: GraphicsRomLayout,
    pub(super) checksum_field: usize,
    pub(super) options: GraphicsSaveOptions,
    pub(super) slots: Vec<usize>,
    pub(super) file_numbers: Vec<usize>,
    pub(super) family: &'static str,
    pub(super) description: &'static str,
    pub(super) smw_us_v1_special: bool,
    pub(super) smw_us_v1_standard_install: bool,
    pub(super) smw_us_v1_exgraphics: bool,
    /// Uses Lunar Magic's `ExGFX` namespace even for reserved files `$60` through `$63`.
    pub(super) exgraphics_names: bool,
    pub(super) ordinary_options: Option<OrdinaryGraphicsImportOptions>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct OrdinaryGraphicsImportOptions {
    pub(super) logical_pc_address: usize,
    pub(super) expansion_target: Option<usize>,
}

struct RunningImport {
    family: &'static str,
    target: GraphicsImportTarget,
    total: usize,
    completed: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    result: Receiver<Result<Option<PreparedRomCommit>, String>>,
}

#[derive(Clone)]
enum GraphicsImportTarget {
    Directory(PathBuf),
    JoinedFile(PathBuf),
}

impl GraphicsImportTarget {
    fn path(&self) -> &Path {
        match self {
            Self::Directory(path) | Self::JoinedFile(path) => path,
        }
    }
}

impl RunningImport {
    fn request_cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub(super) struct GraphicsImportWorker {
    running: Option<RunningImport>,
}

impl GraphicsImportWorker {
    pub(super) const fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub(super) fn start(
        &mut self,
        source: GraphicsImportSource,
        directory: PathBuf,
    ) -> Result<(), String> {
        self.start_target(source, GraphicsImportTarget::Directory(directory))
    }

    pub(super) fn start_joined(
        &mut self,
        source: GraphicsImportSource,
        path: PathBuf,
    ) -> Result<(), String> {
        self.start_target(source, GraphicsImportTarget::JoinedFile(path))
    }

    fn start_target(
        &mut self,
        source: GraphicsImportSource,
        target: GraphicsImportTarget,
    ) -> Result<(), String> {
        if self.running.is_some() {
            return Err("a graphics insertion is already running".into());
        }
        let family = source.family;
        let total = source.file_numbers.len();
        if total != source.slots.len() {
            return Err("graphics slot and filename mappings have different lengths".into());
        }
        if total == 0 || total > 0x1000 {
            return Err(format!("unsupported {} GFX count {total}", source.family));
        }
        let completed = Arc::new(AtomicUsize::new(0));
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_completed = Arc::clone(&completed);
        let worker_cancelled = Arc::clone(&cancelled);
        let worker_target = target.clone();
        let (sender, result) = mpsc::channel();
        std::thread::Builder::new()
            .name(format!("lm-{}-gfx-import", source.family))
            .spawn(move || {
                let result =
                    prepare_import(source, &worker_target, &worker_completed, &worker_cancelled);
                let _send_result = sender.send(result);
            })
            .map_err(|error| format!("could not create standard-GFX import worker: {error}"))?;
        self.running = Some(RunningImport {
            family,
            target,
            total,
            completed,
            cancelled,
            result,
        });
        Ok(())
    }

    pub(super) fn show(
        &mut self,
        context: &egui::Context,
    ) -> Option<Result<Option<PreparedRomCommit>, String>> {
        let completion = self.poll();
        if let Some(running) = &self.running {
            let completed = running.completed.load(Ordering::Relaxed);
            let cancellation_requested = running.cancelled.load(Ordering::Relaxed);
            egui::Window::new(format!("Inserting {} GFX", running.family))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format!("Reading {}", running.target.path().display()));
                    ui.add(
                        egui::ProgressBar::new(completed as f32 / running.total as f32)
                            .text(format!("{completed} / {}", running.total)),
                    );
                    ui.label("The ROM changes only after the complete set validates.");
                    if cancellation_requested {
                        ui.label("Cancelling after the current file…");
                    } else if ui.button("Cancel").clicked()
                        || context.input(|input| input.key_pressed(egui::Key::Escape))
                    {
                        running.request_cancel();
                    }
                });
            context.request_repaint_after(std::time::Duration::from_millis(100));
        }
        completion
    }

    pub(super) fn poll(&mut self) -> Option<Result<Option<PreparedRomCommit>, String>> {
        let running = self.running.as_ref()?;
        match running.result.try_recv() {
            Ok(result) => {
                self.running = None;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.running = None;
                Some(Err(
                    "graphics import worker stopped without reporting a result".into(),
                ))
            }
        }
    }
}

fn prepare_import(
    mut source: GraphicsImportSource,
    target: &GraphicsImportTarget,
    completed: &AtomicUsize,
    cancelled: &AtomicBool,
) -> Result<Option<PreparedRomCommit>, String> {
    let original_image = source.image.clone();
    if let Some(ordinary) = source.ordinary_options {
        apply_ordinary_options(
            &mut source.image,
            source.layout.mapper,
            source.checksum_field,
            &mut source.options,
            ordinary,
        )?;
    }
    let prepared_image = source.image.clone();
    let total = source.file_numbers.len();
    let result = match target {
        GraphicsImportTarget::Directory(directory) => {
            let mut files = Vec::with_capacity(total);
            for file_number in source.file_numbers.iter().copied() {
                if cancelled.load(Ordering::Relaxed) {
                    return Ok(None);
                }
                let name = graphics_file_name(file_number, source.exgraphics_names);
                let description = format!("{name} raw graphics");
                let bytes = crate::dialogs::read_regular_bounded(
                    &directory.join(&name),
                    u64::try_from(source.layout.maximum_decompressed_len).unwrap_or(u64::MAX),
                    &description,
                )
                .map_err(|error| format!("{description}: {error}"))?;
                files.push(bytes);
                completed.fetch_add(1, Ordering::Relaxed);
            }
            if cancelled.load(Ordering::Relaxed) {
                return Ok(None);
            }
            if source.smw_us_v1_standard_install {
                lm_app::prepare_smw_us_v1_standard_graphics_install(
                    source.expected_revision,
                    source.image,
                    &files,
                )
                .map(Some)
            } else if source.smw_us_v1_exgraphics {
                let files = source
                    .file_numbers
                    .iter()
                    .copied()
                    .zip(files)
                    .map(|(number, bytes)| {
                        u16::try_from(number)
                            .map(|number| (number, bytes))
                            .map_err(|_| format!("ExGFX file number {number:X} exceeds $FFFF"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                lm_app::prepare_smw_us_v1_exgraphics_directory_install(
                    source.expected_revision,
                    source.image,
                    &files,
                )
                .map(Some)
            } else if source.smw_us_v1_special {
                lm_app::prepare_smw_us_v1_special_graphics_import(
                    source.expected_revision,
                    source.image,
                    source.checksum_field,
                    &files,
                    &source.options,
                )
                .map(Some)
            } else {
                lm_app::prepare_named_graphics_import(
                    source.expected_revision,
                    source.image,
                    source.layout,
                    source.checksum_field,
                    &files,
                    lm_app::NamedGraphicsImport {
                        slots: &source.slots,
                        file_numbers: &source.file_numbers,
                        description: source.description,
                    },
                    &source.options,
                )
                .map(Some)
            }
        }
        GraphicsImportTarget::JoinedFile(path) => {
            if source.smw_us_v1_special {
                return Err("special GFX32/GFX33 insertion requires a directory".into());
            }
            if cancelled.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let maximum = source
                .layout
                .maximum_decompressed_len
                .checked_mul(total)
                .and_then(|value| u64::try_from(value).ok())
                .ok_or("AllGFX.bin read bound overflow")?;
            let joined = crate::dialogs::read_regular_bounded(path, maximum, "AllGFX.bin")
                .map_err(|error| format!("AllGFX.bin: {error}"))?;
            completed.store(total, Ordering::Relaxed);
            if cancelled.load(Ordering::Relaxed) {
                return Ok(None);
            }
            if source.smw_us_v1_standard_install {
                lm_app::prepare_smw_us_v1_joined_standard_graphics_install(
                    source.expected_revision,
                    source.image,
                    &joined,
                )
                .map(Some)
            } else {
                lm_app::prepare_joined_standard_graphics_import(
                    source.expected_revision,
                    source.image,
                    source.layout,
                    source.checksum_field,
                    &joined,
                    &source.options,
                )
                .map(Some)
            }
        }
    }?;
    let Some(prepared) = result else {
        return Ok(None);
    };
    if original_image.logical_len() == prepared_image.logical_len() {
        return Ok(Some(prepared));
    }
    combine_expansion_commit(original_image, prepared_image, prepared).map(Some)
}

fn combine_expansion_commit(
    original_image: RomImage,
    prepared_image: RomImage,
    prepared: PreparedRomCommit,
) -> Result<PreparedRomCommit, String> {
    let mut combined = lm_project::Project::new(prepared_image);
    combined
        .apply_mutation(prepared.description.clone(), &prepared.mutation)
        .map_err(|error| format!("could not combine graphics insertion with expansion: {error}"))?;
    let mutation = lm_project::RomMutation::between(
        prepared.mutation.mapper,
        original_image.logical_bytes(),
        combined.rom.logical_bytes(),
    )
    .map_err(|error| error.to_string())?;
    Ok(PreparedRomCommit {
        expected_revision: prepared.expected_revision,
        description: prepared.description,
        mutation,
    })
}

fn apply_ordinary_options(
    image: &mut RomImage,
    fallback_mapper: lm_rom::Mapper,
    checksum_field: usize,
    options: &mut GraphicsSaveOptions,
    ordinary: OrdinaryGraphicsImportOptions,
) -> Result<(), String> {
    if let Some(target_len) = ordinary.expansion_target
        && target_len > image.logical_len()
    {
        let mapper = lm_rom::detect_identity(image)
            .map(|identity| identity.mapper)
            .unwrap_or(fallback_mapper);
        let mut expanded = lm_project::Project::new(image.clone());
        expanded
            .expand_rom(mapper, target_len, 0, checksum_field)
            .map_err(|error| format!("could not expand ROM before graphics insertion: {error}"))?;
        *image = expanded.rom;
    }
    let end = image.logical_len();
    if ordinary.logical_pc_address >= end {
        return Err(format!(
            "graphics insertion address ${:X} is outside the ${end:X}-byte ROM",
            ordinary.logical_pc_address
        ));
    }
    options.allocation.search = ordinary.logical_pc_address..end;
    Ok(())
}

fn graphics_file_name(slot: usize, exgraphics: bool) -> String {
    let prefix = if exgraphics || slot >= 0x80 {
        "ExGFX"
    } else {
        "GFX"
    };
    format!("{prefix}{slot:02X}.bin")
}

pub(super) fn enumerate_exgraphics_files(
    directory: &Path,
    table_entries: usize,
) -> Result<Vec<usize>, String> {
    if table_entries <= 0x80 || table_entries > 0x1000 {
        return Err(format!(
            "profile graphics table has {table_entries} entries; ExGFX requires 129 through 4096"
        ));
    }
    let mut slots = Vec::new();
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("could not enumerate {}: {error}", directory.display()))?;
    for (index, entry) in entries.enumerate() {
        if index >= 0x2000 {
            return Err(format!(
                "{} contains more than 8192 directory entries",
                directory.display()
            ));
        }
        let entry = entry.map_err(|error| {
            format!(
                "could not enumerate an entry in {}: {error}",
                directory.display()
            )
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let folded = name.to_ascii_lowercase();
        if !folded.starts_with("exgfx") || !folded.ends_with(".bin") {
            continue;
        }
        let digits = &name[5..name.len() - 4];
        let slot = usize::from_str_radix(digits, 16)
            .map_err(|_| format!("invalid ExGFX filename {name}"))?;
        let native_slot =
            (0x60..=0x63).contains(&slot) || (0x80..table_entries.min(0x1000)).contains(&slot);
        if !native_slot || graphics_file_name(slot, true) != name {
            return Err(format!(
                "ExGFX filename {name} is noncanonical or outside the profile table"
            ));
        }
        slots.push(slot);
    }
    slots.sort_unstable();
    if slots.is_empty() {
        return Err(format!(
            "{} contains no canonical ExGFX60.bin through ExGFX63.bin or ExGFX80.bin through ExGFXFFF.bin files",
            directory.display()
        ));
    }
    Ok(slots)
}

#[cfg(test)]
mod tests {
    use super::{
        GraphicsImportTarget, OrdinaryGraphicsImportOptions, RunningImport, apply_ordinary_options,
        combine_expansion_commit, enumerate_exgraphics_files,
    };
    use lm_project::GraphicsSaveOptions;
    use lm_rats::AllocationPolicy;
    use lm_rom::{Mapper, RomImage};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    };
    use std::{fs, path::PathBuf};

    #[test]
    fn cancellation_request_is_shared_with_the_import_worker() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let (_sender, result) = mpsc::channel();
        let running = RunningImport {
            family: "standard",
            target: GraphicsImportTarget::Directory(PathBuf::from("Graphics")),
            total: 0x32,
            completed: Arc::new(AtomicUsize::new(0)),
            cancelled: Arc::clone(&cancelled),
            result,
        };
        running.request_cancel();
        assert!(cancelled.load(Ordering::Relaxed));
    }

    #[test]
    fn exgraphics_directory_enumeration_is_sorted_bounded_and_canonical() {
        let directory =
            std::env::temp_dir().join(format!("lm-exgfx-enumeration-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("ExGFX123.bin"), []).unwrap();
        fs::write(directory.join("ExGFX80.bin"), []).unwrap();
        fs::write(directory.join("ExGFX60.bin"), []).unwrap();
        fs::write(directory.join("notes.txt"), []).unwrap();
        assert_eq!(
            enumerate_exgraphics_files(&directory, 0x200).unwrap(),
            [0x60, 0x80, 0x123]
        );
        fs::write(directory.join("ExGFX081.bin"), []).unwrap();
        assert!(enumerate_exgraphics_files(&directory, 0x200).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn ordinary_options_expand_before_binding_the_exact_allocation_cursor() {
        let mut image = RomImage::from_bytes(vec![0xff; 0x8000]).unwrap();
        let mut options = GraphicsSaveOptions {
            allocation: AllocationPolicy::lorom(0..0x8000),
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        apply_ordinary_options(
            &mut image,
            Mapper::LoRom,
            0x7fdc,
            &mut options,
            OrdinaryGraphicsImportOptions {
                logical_pc_address: 0x9000,
                expansion_target: Some(0x10000),
            },
        )
        .unwrap();
        assert_eq!(image.logical_len(), 0x10000);
        assert_eq!(options.allocation.search, 0x9000..0x10000);

        let error = apply_ordinary_options(
            &mut image,
            Mapper::LoRom,
            0x7fdc,
            &mut options,
            OrdinaryGraphicsImportOptions {
                logical_pc_address: 0x10000,
                expansion_target: None,
            },
        )
        .unwrap_err();
        assert!(error.contains("outside"));
    }

    #[test]
    fn expansion_and_prepared_graphics_write_combine_into_one_original_length_mutation() {
        let original = RomImage::from_bytes(vec![0xff; 0x8000]).unwrap();
        let mut expanded_project = lm_project::Project::new(original.clone());
        expanded_project
            .expand_rom(Mapper::LoRom, 0x10000, 0, 0x7fdc)
            .unwrap();
        let expanded = expanded_project.rom.clone();
        let mut final_bytes = expanded.logical_bytes().to_vec();
        final_bytes[0x9000..0x9004].copy_from_slice(&[1, 2, 3, 4]);
        let prepared = lm_app::PreparedRomCommit {
            expected_revision: 7,
            description: "graphics".into(),
            mutation: lm_project::RomMutation::between(
                Mapper::LoRom,
                expanded.logical_bytes(),
                &final_bytes,
            )
            .unwrap(),
        };
        let combined = combine_expansion_commit(original.clone(), expanded, prepared).unwrap();
        assert_eq!(combined.expected_revision, 7);
        assert_eq!(combined.mutation.expected_len, 0x8000);
        let mut reopened = lm_project::Project::new(original);
        reopened
            .apply_mutation("combined", &combined.mutation)
            .unwrap();
        assert_eq!(reopened.rom.logical_len(), 0x10000);
        assert_eq!(&reopened.rom.logical_bytes()[0x9000..0x9004], &[1, 2, 3, 4]);
    }
}
