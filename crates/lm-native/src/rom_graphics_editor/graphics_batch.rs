use eframe::egui;
use lm_project::{GraphicsRomLayout, Project};
use lm_rom::RomImage;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};

#[derive(Clone)]
pub(super) struct GraphicsBatchSource {
    pub(super) image: RomImage,
    pub(super) layout: GraphicsRomLayout,
    pub(super) slots: Vec<usize>,
    pub(super) file_numbers: Vec<usize>,
    pub(super) family: &'static str,
    /// Uses Lunar Magic's `ExGFX` namespace even for reserved files `$60` through `$63`.
    pub(super) exgraphics_names: bool,
    pub(super) encoding: GraphicsBatchEncoding,
    pub(super) raw_4bpp_overrides: Vec<(usize, Vec<u8>)>,
    /// Per-file `(slot, layout)` mappings for non-tabular sources; empty uses `slots` and `layout`.
    pub(super) file_layouts: Vec<(usize, GraphicsRomLayout)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GraphicsBatchEncoding {
    Native,
    Decoded4Bpp,
    LunarMagicStandard,
}

struct RunningBatch {
    family: &'static str,
    target: GraphicsExportTarget,
    total: usize,
    completed: Arc<AtomicUsize>,
    cancelled: Arc<AtomicBool>,
    result: Receiver<Result<Option<usize>, String>>,
}

#[derive(Clone)]
enum GraphicsExportTarget {
    Directory(PathBuf),
    ReplaceDirectory {
        standard_path: PathBuf,
        exgraphics_path: PathBuf,
        required_existing: Vec<PathBuf>,
    },
    ReplaceJoinedFile {
        path: PathBuf,
        exgraphics_path: PathBuf,
        required_existing: Vec<PathBuf>,
        file_sizes: Vec<usize>,
    },
    JoinedFile(PathBuf),
}

impl GraphicsExportTarget {
    fn path(&self) -> &Path {
        match self {
            Self::Directory(path)
            | Self::JoinedFile(path)
            | Self::ReplaceDirectory {
                standard_path: path,
                ..
            }
            | Self::ReplaceJoinedFile { path, .. } => path,
        }
    }
}

impl RunningBatch {
    fn request_cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub(super) struct GraphicsBatchWorker {
    running: Option<RunningBatch>,
}

impl GraphicsBatchWorker {
    pub(super) const fn is_running(&self) -> bool {
        self.running.is_some()
    }

    pub(super) fn start(
        &mut self,
        source: GraphicsBatchSource,
        directory: PathBuf,
    ) -> Result<(), String> {
        self.start_target(source, GraphicsExportTarget::Directory(directory))
    }

    pub(super) fn start_joined(
        &mut self,
        source: GraphicsBatchSource,
        path: PathBuf,
    ) -> Result<(), String> {
        self.start_target(source, GraphicsExportTarget::JoinedFile(path))
    }

    pub(super) fn start_replace(
        &mut self,
        source: GraphicsBatchSource,
        standard_directory: PathBuf,
        exgraphics_directory: PathBuf,
        required_existing: Vec<PathBuf>,
    ) -> Result<(), String> {
        self.start_target(
            source,
            GraphicsExportTarget::ReplaceDirectory {
                standard_path: standard_directory,
                exgraphics_path: exgraphics_directory,
                required_existing,
            },
        )
    }

    pub(super) fn start_replace_joined(
        &mut self,
        source: GraphicsBatchSource,
        path: PathBuf,
        exgraphics_directory: PathBuf,
        required_existing: Vec<PathBuf>,
        file_sizes: Vec<usize>,
    ) -> Result<(), String> {
        self.start_target(
            source,
            GraphicsExportTarget::ReplaceJoinedFile {
                path,
                exgraphics_path: exgraphics_directory,
                required_existing,
                file_sizes,
            },
        )
    }

    fn start_target(
        &mut self,
        source: GraphicsBatchSource,
        target: GraphicsExportTarget,
    ) -> Result<(), String> {
        if self.running.is_some() {
            return Err("a graphics extraction is already running".into());
        }
        let family = source.family;
        let total = source.file_numbers.len();
        if total != source.slots.len() {
            return Err("graphics slot and filename mappings have different lengths".into());
        }
        if !source.file_layouts.is_empty() && source.file_layouts.len() != total {
            return Err("per-file graphics layouts have a different length".into());
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
            .name(format!("lm-{}-gfx-export", source.family))
            .spawn(move || {
                let result =
                    export_batch(source, &worker_target, &worker_completed, &worker_cancelled);
                let _send_result = sender.send(result);
            })
            .map_err(|error| format!("could not create standard-GFX worker: {error}"))?;
        self.running = Some(RunningBatch {
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
    ) -> Option<Result<Option<usize>, String>> {
        let completion = self.poll();
        if let Some(running) = &self.running {
            let completed = running.completed.load(Ordering::Relaxed);
            let cancellation_requested = running.cancelled.load(Ordering::Relaxed);
            egui::Window::new(format!("Extracting {} GFX", running.family))
                .collapsible(false)
                .resizable(false)
                .show(context, |ui| {
                    ui.label(format!("Staging {}", running.target.path().display()));
                    ui.add(
                        egui::ProgressBar::new(completed as f32 / running.total as f32)
                            .text(format!("{completed} / {}", running.total)),
                    );
                    ui.label("Files become visible only after the complete set is staged.");
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

    fn poll(&mut self) -> Option<Result<Option<usize>, String>> {
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
                    "graphics worker stopped without reporting a result".into()
                ))
            }
        }
    }
}

fn export_batch(
    source: GraphicsBatchSource,
    target: &GraphicsExportTarget,
    completed: &AtomicUsize,
    cancelled: &AtomicBool,
) -> Result<Option<usize>, String> {
    let total = source.file_numbers.len();
    let project = Project::new(source.image);
    if let GraphicsExportTarget::ReplaceDirectory {
        required_existing, ..
    }
    | GraphicsExportTarget::ReplaceJoinedFile {
        required_existing, ..
    } = target
    {
        validate_existing_graphics_set(required_existing)?;
    }
    let mut joined_replacement = match target {
        GraphicsExportTarget::ReplaceJoinedFile {
            path, file_sizes, ..
        } => {
            let expected = file_sizes.iter().try_fold(0_usize, |total, size| {
                total.checked_add(*size).ok_or("AllGFX.bin size overflow")
            })?;
            let maximum = u64::try_from(expected).map_err(|_| "AllGFX.bin size overflow")?;
            let bytes = crate::dialogs::read_regular_bounded(path, maximum, "AllGFX.bin")
                .map_err(|error| format!("AllGFX.bin: {error}"))?;
            if bytes.len() != expected {
                return Err(format!(
                    "AllGFX.bin has {:#X} bytes instead of the expected {expected:#X}",
                    bytes.len()
                ));
            }
            Some(bytes)
        }
        _ => None,
    };
    let mut replacements = Vec::with_capacity(
        if matches!(
            target,
            GraphicsExportTarget::Directory(_)
                | GraphicsExportTarget::ReplaceDirectory { .. }
                | GraphicsExportTarget::ReplaceJoinedFile { .. }
        ) {
            total
        } else {
            0
        },
    );
    let mut joined_files = Vec::with_capacity(
        usize::from(matches!(target, GraphicsExportTarget::JoinedFile(_))) * total,
    );
    for (index, (slot, file_number)) in source
        .slots
        .iter()
        .copied()
        .zip(source.file_numbers.iter().copied())
        .enumerate()
    {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let (load_slot, load_layout) = source
            .file_layouts
            .get(index)
            .copied()
            .unwrap_or((slot, source.layout));
        let bytes = if let Some((_, bytes)) = source
            .raw_4bpp_overrides
            .iter()
            .find(|(override_slot, _)| *override_slot == slot)
        {
            bytes.clone()
        } else {
            match source.encoding {
                GraphicsBatchEncoding::Native => project
                    .load_decompressed_graphics_file(load_slot, load_layout)
                    .map_err(|error| {
                        format!(
                            "{}: {error}",
                            graphics_file_name(file_number, source.exgraphics_names)
                        )
                    })?,
                GraphicsBatchEncoding::Decoded4Bpp => {
                    let loaded = project
                        .load_super_graphics_file(
                            u16::try_from(load_slot).map_err(|_| {
                                format!(
                                    "graphics slot {load_slot:X} exceeds the supported file range"
                                )
                            })?,
                            load_layout,
                        )
                        .map_err(|error| {
                            format!(
                                "{}: {error}",
                                graphics_file_name(file_number, source.exgraphics_names)
                            )
                        })?;
                    lm_graphics::GraphicsFile4bpp {
                        tiles: loaded.tiles,
                    }
                    .encode()
                    .map_err(|error| {
                        format!(
                            "{}: {error}",
                            graphics_file_name(file_number, source.exgraphics_names)
                        )
                    })?
                }
                GraphicsBatchEncoding::LunarMagicStandard => {
                    let native = project
                        .load_decompressed_graphics_file(load_slot, load_layout)
                        .map_err(|error| {
                            format!(
                                "{}: {error}",
                                graphics_file_name(file_number, source.exgraphics_names)
                            )
                        })?;
                    lunar_magic_standard_export_bytes(file_number, native)?
                }
            }
        };
        if source.encoding == GraphicsBatchEncoding::Decoded4Bpp && bytes.len() != 0x1000 {
            return Err(format!(
                "{}: decoded level GFX has {:#X} bytes instead of 0x1000",
                graphics_file_name(file_number, source.exgraphics_names),
                bytes.len()
            ));
        }
        match target {
            GraphicsExportTarget::Directory(directory) => {
                replacements.push((
                    batch_output_path(directory, file_number, source.exgraphics_names),
                    bytes,
                ));
            }
            GraphicsExportTarget::ReplaceDirectory {
                standard_path,
                exgraphics_path,
                ..
            } => replacements.push((
                replacement_output_path(
                    standard_path,
                    exgraphics_path,
                    file_number,
                    source.exgraphics_names,
                ),
                bytes,
            )),
            GraphicsExportTarget::ReplaceJoinedFile {
                exgraphics_path,
                file_sizes,
                ..
            } => {
                if !source.exgraphics_names && file_number < 0x34 {
                    patch_joined_graphics_file(
                        joined_replacement
                            .as_mut()
                            .expect("joined replacement target loads its existing image"),
                        file_sizes,
                        file_number,
                        &bytes,
                    )?;
                } else {
                    replacements
                        .push((batch_output_path(exgraphics_path, file_number, true), bytes));
                }
            }
            GraphicsExportTarget::JoinedFile(_) => joined_files.push(bytes),
        }
        completed.store(index + 1, Ordering::Relaxed);
    }
    if cancelled.load(Ordering::Relaxed) {
        return Ok(None);
    }
    match target {
        GraphicsExportTarget::Directory(_) => {
            let documents = replacements
                .iter()
                .map(|(path, bytes)| (path.as_path(), bytes.as_slice()))
                .collect::<Vec<_>>();
            lm_app::file_persistence::replace_or_create_group(&documents)
                .map_err(|error| error.to_string())?;
        }
        GraphicsExportTarget::ReplaceDirectory { .. } => {
            let documents = replacements
                .iter()
                .map(|(path, bytes)| (path.as_path(), bytes.as_slice()))
                .collect::<Vec<_>>();
            lm_app::file_persistence::replace_existing_group(&documents)
                .map_err(|error| error.to_string())?;
        }
        GraphicsExportTarget::ReplaceJoinedFile { path, .. } => {
            replacements.insert(
                0,
                (
                    path.clone(),
                    joined_replacement
                        .expect("joined replacement target retains its patched image"),
                ),
            );
            let documents = replacements
                .iter()
                .map(|(path, bytes)| (path.as_path(), bytes.as_slice()))
                .collect::<Vec<_>>();
            lm_app::file_persistence::replace_existing_group(&documents)
                .map_err(|error| error.to_string())?;
        }
        GraphicsExportTarget::JoinedFile(path) => {
            let joined = lm_graphics::JoinedGraphics {
                files: joined_files,
            }
            .join()
            .map_err(|error| error.to_string())?;
            lm_app::file_persistence::replace_or_create_group(&[(path.as_path(), &joined)])
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(Some(total))
}

fn validate_existing_graphics_set(paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("replacement export requires an existing graphics-file set".into());
    }
    for path in paths {
        let metadata = std::fs::symlink_metadata(path).map_err(|error| {
            format!(
                "required extracted graphics file {} is unavailable: {error}",
                path.display()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err(format!(
                "required extracted graphics path must be a regular file: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn patch_joined_graphics_file(
    joined: &mut [u8],
    file_sizes: &[usize],
    file_number: usize,
    decoded_4bpp: &[u8],
) -> Result<(), String> {
    let size = *file_sizes
        .get(file_number)
        .ok_or_else(|| format!("GFX{file_number:02X} has no declared AllGFX.bin file size"))?;
    if decoded_4bpp.len() < size {
        return Err(format!(
            "GFX{file_number:02X} has {:#X} decoded bytes but AllGFX.bin requires {size:#X}",
            decoded_4bpp.len()
        ));
    }
    let offset = file_sizes[..file_number]
        .iter()
        .try_fold(0_usize, |total, size| total.checked_add(*size))
        .ok_or("AllGFX.bin offset overflow")?;
    let end = offset
        .checked_add(size)
        .ok_or("AllGFX.bin offset overflow")?;
    let joined_len = joined.len();
    let destination = joined.get_mut(offset..end).ok_or_else(|| {
        format!(
            "GFX{file_number:02X} range {offset:#X}..{end:#X} exceeds AllGFX.bin length {:#X}",
            joined_len
        )
    })?;
    destination.copy_from_slice(&decoded_4bpp[..size]);
    Ok(())
}

fn lunar_magic_standard_export_bytes(
    file_number: usize,
    native: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let expands_to_editable_4bpp = matches!(
        file_number,
        0x00..=0x26 | 0x2c..=0x2e | 0x30..=0x31 | 0x33
    );
    let mut editable = if !expands_to_editable_4bpp || native.len() == 0x1000 {
        native
    } else {
        if native.len() % 24 != 0 {
            return Err(format!(
                "GFX{file_number:02X}: Lunar Magic's packed 3bpp source requires complete 24-byte tiles; got {:#X} bytes",
                native.len()
            ));
        }
        let tiles = lm_graphics::decode_planar_tiles(&native, 3)
            .map_err(|error| format!("GFX{file_number:02X}: {error}"))?;
        lm_graphics::encode_planar_tiles(&tiles, 4)
            .map_err(|error| format!("GFX{file_number:02X}: {error}"))?
    };
    if matches!(file_number, 0x01 | 0x17 | 0x31) {
        synthesize_missing_fourth_graphics_bitplane(&mut editable);
    }
    match file_number {
        0x08 => synthesize_selected_tiles(
            &mut editable,
            &[
                0x37, 0x38, 0x39, 0x3a, 0x3b, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x56, 0x57, 0x58, 0x59,
                0x5a, 0x5b, 0x7a, 0x7b, 0x60, 0x70, 0x6e, 0x6f, 0x7e, 0x7f,
            ],
        ),
        0x1e => synthesize_selected_tiles(&mut editable, &(0..0x80).collect::<Vec<_>>()),
        _ => {}
    }
    Ok(editable)
}

fn synthesize_selected_tiles(bytes: &mut [u8], tiles: &[usize]) {
    for tile in tiles {
        let base = tile * 0x20;
        for row in 0..8 {
            let offset = base + 0x10 + row * 2;
            if offset + 1 >= bytes.len() {
                return;
            }
            bytes[offset + 1] = bytes[offset - 0x10] | bytes[offset - 0x0f] | bytes[offset];
        }
    }
}

fn synthesize_missing_fourth_graphics_bitplane(bytes: &mut [u8]) {
    let ranges = [0x10..0x40, 0x210..0x240];
    if ranges
        .iter()
        .flat_map(Clone::clone)
        .filter(|offset| offset & 0x10 != 0 && offset & 1 == 0)
        .any(|offset| bytes.get(offset + 1).is_some_and(|value| *value != 0))
    {
        return;
    }
    for offset in ranges
        .into_iter()
        .flatten()
        .filter(|offset| offset & 0x10 != 0 && offset & 1 == 0)
    {
        if offset + 1 >= bytes.len() {
            return;
        }
        bytes[offset + 1] = bytes[offset - 0x10] | bytes[offset - 0x0f] | bytes[offset];
    }
}

fn batch_output_path(directory: &Path, slot: usize, exgraphics: bool) -> PathBuf {
    directory.join(graphics_file_name(slot, exgraphics))
}

fn replacement_output_path(
    standard_directory: &Path,
    exgraphics_directory: &Path,
    slot: usize,
    exgraphics: bool,
) -> PathBuf {
    let directory = if exgraphics || slot >= 0x34 {
        exgraphics_directory
    } else {
        standard_directory
    };
    batch_output_path(directory, slot, exgraphics)
}

fn graphics_file_name(slot: usize, exgraphics: bool) -> String {
    let prefix = if exgraphics || slot >= 0x80 {
        "ExGFX"
    } else {
        "GFX"
    };
    format!("{prefix}{slot:02X}.bin")
}

#[cfg(test)]
mod tests {
    use super::{
        GraphicsBatchEncoding, GraphicsBatchSource, GraphicsExportTarget, RunningBatch,
        batch_output_path, export_batch, lunar_magic_standard_export_bytes,
        patch_joined_graphics_file, replacement_output_path,
    };
    use lm_graphics::{GraphicsFile4bpp, IndexedTile};
    use lm_project::{
        GraphicsCompression, GraphicsRomLayout, GraphicsSaveOptions, LevelPointerTable, Project,
    };
    use lm_rats::{AllocationPolicy, ProtectedRange};
    use lm_rom::{Mapper, RomImage};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    };

    static NEXT_PATH: AtomicUsize = AtomicUsize::new(0);

    fn temporary_directory() -> PathBuf {
        std::env::temp_dir().join(format!(
            "lm-standard-gfx-export-{}-{}",
            std::process::id(),
            NEXT_PATH.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn fixture_source() -> (GraphicsBatchSource, [Vec<u8>; 2]) {
        let layout = GraphicsRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x200,
                entries: 2,
                stride: 3,
            },
            split_pointer_planes: None,
            compression: GraphicsCompression::Lz2,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x10000,
        };
        let mut project = Project::new(RomImage::from_bytes(vec![0xff; 0x8000]).unwrap());
        let files = [
            GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([1; 64])],
            },
            GraphicsFile4bpp {
                tiles: vec![IndexedTile::new([2; 64]), IndexedTile::new([3; 64])],
            },
        ];
        let options = GraphicsSaveOptions {
            allocation: AllocationPolicy {
                search: 0x1000..0x7000,
                bank_size: Some(0x8000),
                fill_bytes: vec![0xff],
                protected: vec![ProtectedRange(0x200..0x206)],
            },
            previous_block: None,
            reuse_identical: true,
            erase_fill: 0xff,
        };
        for (slot, file) in files.iter().enumerate() {
            project
                .save_graphics_file(slot, file, layout, &options)
                .unwrap();
        }
        let expected = files.map(|file| file.encode().unwrap());
        (
            GraphicsBatchSource {
                image: project.rom,
                layout,
                slots: vec![0, 1],
                file_numbers: vec![0, 1],
                family: "standard",
                exgraphics_names: false,
                encoding: GraphicsBatchEncoding::Native,
                raw_4bpp_overrides: Vec::new(),
                file_layouts: Vec::new(),
            },
            expected,
        )
    }

    #[test]
    fn batch_names_use_fixed_uppercase_standard_gfx_numbers() {
        assert_eq!(
            batch_output_path(Path::new("/tmp/Graphics"), 0, false),
            Path::new("/tmp/Graphics/GFX00.bin")
        );
        assert_eq!(
            batch_output_path(Path::new("/tmp/Graphics"), 0x31, false),
            Path::new("/tmp/Graphics/GFX31.bin")
        );
        assert_eq!(
            batch_output_path(Path::new("/tmp/Graphics"), 0x33, false),
            Path::new("/tmp/Graphics/GFX33.bin")
        );
        assert_eq!(
            batch_output_path(Path::new("/tmp/Graphics"), 0x80, true),
            Path::new("/tmp/Graphics/ExGFX80.bin")
        );
        assert_eq!(
            batch_output_path(Path::new("/tmp/Graphics"), 0xfff, true),
            Path::new("/tmp/Graphics/ExGFXFFF.bin")
        );
        assert_eq!(
            batch_output_path(Path::new("/tmp/Graphics"), 0x60, true),
            Path::new("/tmp/Graphics/ExGFX60.bin")
        );
    }

    #[test]
    fn replacement_paths_split_standard_and_extended_names_like_lunar_magic() {
        let standard = Path::new("project/Graphics");
        let extended = Path::new("project/ExGraphics");
        assert_eq!(
            replacement_output_path(standard, extended, 0x14, false),
            standard.join("GFX14.bin")
        );
        assert_eq!(
            replacement_output_path(standard, extended, 0x80, false),
            extended.join("ExGFX80.bin")
        );
        assert_eq!(
            replacement_output_path(standard, extended, 0x123, false),
            extended.join("ExGFX123.bin")
        );
    }

    #[test]
    fn cancellation_request_is_shared_with_the_export_worker() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let (_sender, result) = mpsc::channel();
        let running = RunningBatch {
            family: "standard",
            target: GraphicsExportTarget::Directory(PathBuf::from("Graphics")),
            total: 0x32,
            completed: Arc::new(AtomicUsize::new(0)),
            cancelled: Arc::clone(&cancelled),
            result,
        };
        running.request_cancel();
        assert!(cancelled.load(Ordering::Relaxed));
    }

    #[test]
    fn batch_decodes_every_compressed_slot_and_publishes_exact_raw_files() {
        let directory = temporary_directory();
        fs::create_dir(&directory).unwrap();
        let (source, expected) = fixture_source();
        let completed = AtomicUsize::new(0);
        let cancelled = AtomicBool::new(false);
        assert_eq!(
            export_batch(
                source.clone(),
                &GraphicsExportTarget::Directory(directory.clone()),
                &completed,
                &cancelled,
            )
            .unwrap(),
            Some(2)
        );
        assert_eq!(completed.load(Ordering::Relaxed), 2);
        assert_eq!(fs::read(directory.join("GFX00.bin")).unwrap(), expected[0]);
        assert_eq!(fs::read(directory.join("GFX01.bin")).unwrap(), expected[1]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn replacement_batch_requires_the_complete_declared_set_and_replaces_atomically() {
        let directory = temporary_directory();
        fs::create_dir(&directory).unwrap();
        let required = (0..=0x33)
            .map(|file| directory.join(format!("GFX{file:02X}.bin")))
            .collect::<Vec<_>>();
        for path in &required {
            fs::write(path, b"old").unwrap();
        }
        let zero = required[0].clone();
        let one = required[1].clone();
        let (source, expected) = fixture_source();
        let target = GraphicsExportTarget::ReplaceDirectory {
            standard_path: directory.clone(),
            exgraphics_path: directory.clone(),
            required_existing: required.clone(),
        };
        assert_eq!(
            export_batch(
                source.clone(),
                &target,
                &AtomicUsize::new(0),
                &AtomicBool::new(false),
            )
            .unwrap(),
            Some(2)
        );
        assert_eq!(fs::read(&zero).unwrap(), expected[0]);
        assert_eq!(fs::read(&one).unwrap(), expected[1]);
        assert_eq!(fs::read(&required[2]).unwrap(), b"old");

        fs::write(&zero, b"sentinel-zero").unwrap();
        fs::remove_file(required.last().unwrap()).unwrap();
        let target = GraphicsExportTarget::ReplaceDirectory {
            standard_path: directory.clone(),
            exgraphics_path: directory.clone(),
            required_existing: required,
        };
        assert!(
            export_batch(
                source,
                &target,
                &AtomicUsize::new(0),
                &AtomicBool::new(false),
            )
            .is_err()
        );
        assert_eq!(fs::read(&zero).unwrap(), b"sentinel-zero");
        assert_eq!(fs::read(&one).unwrap(), expected[1]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn batch_uses_per_file_layouts_for_noncontiguous_special_sources() {
        let directory = temporary_directory();
        fs::create_dir(&directory).unwrap();
        let (mut source, expected) = fixture_source();
        let mut gfx33 = source.layout;
        gfx33.pointers.entries = 1;
        let mut gfx32 = gfx33;
        gfx32.pointers.offset += 3;
        source.layout = gfx33;
        source.file_numbers = vec![0x33, 0x32];
        source.file_layouts = vec![(0, gfx33), (0, gfx32)];
        assert_eq!(
            export_batch(
                source,
                &GraphicsExportTarget::Directory(directory.clone()),
                &AtomicUsize::new(0),
                &AtomicBool::new(false),
            )
            .unwrap(),
            Some(2)
        );
        assert_eq!(fs::read(directory.join("GFX33.bin")).unwrap(), expected[0]);
        assert_eq!(fs::read(directory.join("GFX32.bin")).unwrap(), expected[1]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn invalid_late_destination_leaves_no_partial_batch() {
        let directory = temporary_directory();
        fs::create_dir(&directory).unwrap();
        fs::create_dir(directory.join("GFX01.bin")).unwrap();
        let (source, _) = fixture_source();
        assert!(
            export_batch(
                source,
                &GraphicsExportTarget::Directory(directory.clone()),
                &AtomicUsize::new(0),
                &AtomicBool::new(false),
            )
            .is_err()
        );
        assert!(!directory.join("GFX00.bin").exists());
        assert!(directory.join("GFX01.bin").is_dir());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn repeated_directory_extraction_atomically_replaces_every_existing_file() {
        let oracle = include_str!(
            "../../../../docs/oracle-work/lm363/pristine-us/graphics-extraction-publication/oracle.tsv"
        );
        let fields = oracle
            .lines()
            .skip(1)
            .map(|line| line.split_once('\t').expect("oracle row has two columns"))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(fields["standard_function"], "0047DA40");
        assert_eq!(fields["extended_function"], "0047EFF0");
        assert_eq!(fields["write_mode_address"], "005B2D3C");
        assert_eq!(fields["write_mode_bytes"], "776200");
        assert_eq!(fields["write_mode"], "wb");
        assert_eq!(fields["standard_separate_name"], "GFX%02X.bin");
        assert_eq!(fields["extended_high_name"], "ExGFX%03X.bin");

        let directory = temporary_directory();
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("GFX00.bin"), b"old-zero").unwrap();
        fs::write(directory.join("GFX01.bin"), b"old-one").unwrap();
        let (source, expected) = fixture_source();
        assert_eq!(
            export_batch(
                source,
                &GraphicsExportTarget::Directory(directory.clone()),
                &AtomicUsize::new(0),
                &AtomicBool::new(false),
            )
            .unwrap(),
            Some(2)
        );
        assert_eq!(fs::read(directory.join("GFX00.bin")).unwrap(), expected[0]);
        assert_eq!(fs::read(directory.join("GFX01.bin")).unwrap(), expected[1]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn joined_batch_concatenates_slots_in_pointer_table_order() {
        let directory = temporary_directory();
        fs::create_dir(&directory).unwrap();
        let path = directory.join("AllGFX.bin");
        let (source, expected) = fixture_source();
        assert_eq!(
            export_batch(
                source,
                &GraphicsExportTarget::JoinedFile(path.clone()),
                &AtomicUsize::new(0),
                &AtomicBool::new(false),
            )
            .unwrap(),
            Some(2)
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            [expected[0].clone(), expected[1].clone()].concat()
        );
        let (source, expected) = fixture_source();
        fs::write(&path, b"stale joined extraction").unwrap();
        export_batch(
            source,
            &GraphicsExportTarget::JoinedFile(path.clone()),
            &AtomicUsize::new(0),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            [expected[0].clone(), expected[1].clone()].concat()
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn joined_replacement_patches_declared_ranges_and_preserves_every_other_byte() {
        let directory = temporary_directory();
        fs::create_dir(&directory).unwrap();
        let path = directory.join("AllGFX.bin");
        let (source, expected) = fixture_source();
        let sizes = expected.iter().map(Vec::len).collect::<Vec<_>>();
        fs::write(&path, vec![0x7a; sizes.iter().sum()]).unwrap();
        let target = GraphicsExportTarget::ReplaceJoinedFile {
            path: path.clone(),
            exgraphics_path: directory.clone(),
            required_existing: vec![path.clone()],
            file_sizes: sizes,
        };
        assert_eq!(
            export_batch(
                source.clone(),
                &target,
                &AtomicUsize::new(0),
                &AtomicBool::new(false),
            )
            .unwrap(),
            Some(2)
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            [expected[0].clone(), expected[1].clone()].concat()
        );
        let sentinel = vec![0x6b; expected.iter().map(Vec::len).sum()];
        fs::write(&path, &sentinel).unwrap();
        let target = GraphicsExportTarget::ReplaceJoinedFile {
            path: path.clone(),
            exgraphics_path: directory.clone(),
            required_existing: vec![path.clone(), directory.join("ExGFX80.bin")],
            file_sizes: expected.iter().map(Vec::len).collect(),
        };
        assert!(
            export_batch(
                source,
                &target,
                &AtomicUsize::new(0),
                &AtomicBool::new(false),
            )
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), sentinel);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn joined_range_patch_uses_the_original_size_table_prefix_and_bounds() {
        let mut joined = vec![0x55; 12];
        patch_joined_graphics_file(&mut joined, &[4, 2, 6], 1, &[1, 2, 3, 4]).unwrap();
        assert_eq!(
            joined,
            [
                0x55, 0x55, 0x55, 0x55, 1, 2, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55
            ]
        );
        assert!(patch_joined_graphics_file(&mut joined, &[4, 2, 6], 2, &[0; 5]).is_err());
        assert!(patch_joined_graphics_file(&mut joined, &[4, 2, 6], 3, &[0; 8]).is_err());
    }

    #[test]
    fn extended_batch_preserves_native_two_bitplane_bytes_and_name() {
        let directory = temporary_directory();
        fs::create_dir(&directory).unwrap();
        let layout = GraphicsRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x200,
                entries: 0x81,
                stride: 3,
            },
            split_pointer_planes: None,
            compression: GraphicsCompression::Lz2,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x1000,
        };
        let raw = vec![0x5a; 0x800];
        let mut project = Project::new(RomImage::from_bytes(vec![0; 0x8000]).unwrap());
        project
            .save_decompressed_graphics_slots_with_checksum(
                &[0x80],
                std::slice::from_ref(&raw),
                layout,
                0x7fdc,
                &GraphicsSaveOptions {
                    allocation: AllocationPolicy {
                        search: 0x1000..0x7000,
                        bank_size: Some(0x8000),
                        fill_bytes: vec![0],
                        protected: vec![
                            ProtectedRange(0x200..0x383),
                            ProtectedRange(0x7fdc..0x7fe0),
                        ],
                    },
                    previous_block: None,
                    reuse_identical: true,
                    erase_fill: 0,
                },
            )
            .unwrap();
        let source = GraphicsBatchSource {
            image: project.rom,
            layout,
            slots: vec![0x80],
            file_numbers: vec![0x80],
            family: "extended",
            exgraphics_names: true,
            encoding: GraphicsBatchEncoding::Native,
            raw_4bpp_overrides: Vec::new(),
            file_layouts: Vec::new(),
        };
        assert_eq!(
            export_batch(
                source,
                &GraphicsExportTarget::Directory(directory.clone()),
                &AtomicUsize::new(0),
                &AtomicBool::new(false),
            )
            .unwrap(),
            Some(1)
        );
        assert_eq!(fs::read(directory.join("ExGFX80.bin")).unwrap(), raw);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn level_batch_expands_native_two_bitplane_source_to_one_4bpp_slot() {
        let directory = temporary_directory();
        fs::create_dir(&directory).unwrap();
        let layout = GraphicsRomLayout {
            mapper: Mapper::LoRom,
            pointers: LevelPointerTable {
                offset: 0x200,
                entries: 1,
                stride: 3,
            },
            split_pointer_planes: None,
            compression: GraphicsCompression::Lz2,
            maximum_compressed_len: 0x8000,
            maximum_decompressed_len: 0x1000,
        };
        let tiles = vec![IndexedTile::new([3; 64]); 128];
        let raw_2bpp = lm_graphics::encode_planar_tiles(&tiles, 2).unwrap();
        let expected_4bpp = GraphicsFile4bpp { tiles }.encode().unwrap();
        let mut project = Project::new(RomImage::from_bytes(vec![0; 0x8000]).unwrap());
        project
            .save_decompressed_graphics_slots_with_checksum(
                &[0],
                &[raw_2bpp],
                layout,
                0x7fdc,
                &GraphicsSaveOptions {
                    allocation: AllocationPolicy {
                        search: 0x1000..0x7000,
                        bank_size: Some(0x8000),
                        fill_bytes: vec![0],
                        protected: vec![
                            ProtectedRange(0x200..0x203),
                            ProtectedRange(0x7fdc..0x7fe0),
                        ],
                    },
                    previous_block: None,
                    reuse_identical: true,
                    erase_fill: 0,
                },
            )
            .unwrap();
        let source = GraphicsBatchSource {
            image: project.rom,
            layout,
            slots: vec![0],
            file_numbers: vec![0],
            family: "level",
            exgraphics_names: false,
            encoding: GraphicsBatchEncoding::Decoded4Bpp,
            raw_4bpp_overrides: Vec::new(),
            file_layouts: Vec::new(),
        };
        export_batch(
            source,
            &GraphicsExportTarget::Directory(directory.clone()),
            &AtomicUsize::new(0),
            &AtomicBool::new(false),
        )
        .unwrap();
        let exported = fs::read(directory.join("GFX00.bin")).unwrap();
        assert_eq!(exported.len(), 0x1000);
        assert_eq!(exported, expected_4bpp);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn lunar_magic_standard_export_expands_only_the_editable_ordinary_range() {
        let tiles = vec![IndexedTile::new([7; 64]); 128];
        let native = lm_graphics::encode_planar_tiles(&tiles, 3).unwrap();
        let expected = lm_graphics::encode_planar_tiles(&tiles, 4).unwrap();
        assert_eq!(
            lunar_magic_standard_export_bytes(0x00, native.clone()).unwrap(),
            expected
        );
        assert_eq!(
            lunar_magic_standard_export_bytes(0x27, native.clone()).unwrap(),
            native
        );
        for file_number in [0x2c, 0x2e, 0x30, 0x33] {
            assert_eq!(
                lunar_magic_standard_export_bytes(file_number, native.clone()).unwrap(),
                expected,
                "GFX{file_number:02X}"
            );
        }
        assert_eq!(
            lunar_magic_standard_export_bytes(0x31, native.clone())
                .unwrap()
                .len(),
            expected.len()
        );
        for file_number in [0x27, 0x2b, 0x2f, 0x32] {
            assert_eq!(
                lunar_magic_standard_export_bytes(file_number, native.clone()).unwrap(),
                native,
                "GFX{file_number:02X}"
            );
        }
        assert!(lunar_magic_standard_export_bytes(0x00, vec![0; 17]).is_err());
    }

    #[test]
    #[ignore = "requires a retained pristine ROM and Lunar Magic -ExportGFX directory"]
    fn retained_lunar_magic_standard_export_matches_every_file() {
        let rom = std::env::var_os("LM_PRISTINE_GFX_ROM").expect("LM_PRISTINE_GFX_ROM");
        let directory =
            PathBuf::from(std::env::var_os("LM_GFX_EXPORT_DIR").expect("LM_GFX_EXPORT_DIR"));
        let image = RomImage::from_bytes(fs::read(rom).unwrap()).unwrap();
        let project = Project::new(image.clone());
        let ordinary = lm_profile::smw_us_v1_vanilla_graphics_layout();
        let special = lm_profile::smw_us_v1_special_graphics_layouts(&image).unwrap();
        for file_number in 0..0x34 {
            let (slot, layout) = match file_number {
                0x00..=0x31 => (file_number, ordinary),
                0x32 => (0, special.gfx32),
                0x33 => (0, special.gfx33),
                _ => unreachable!(),
            };
            let native = project
                .load_decompressed_graphics_file(slot, layout)
                .unwrap();
            let actual = lunar_magic_standard_export_bytes(file_number, native).unwrap();
            let expected = fs::read(directory.join(format!("GFX{file_number:02X}.bin"))).unwrap();
            assert_eq!(actual, expected, "GFX{file_number:02X}");
        }
    }

    #[test]
    #[ignore = "requires a retained Lunar Magic first-import ROM and its Graphics directory"]
    fn retained_lunar_magic_standard_import_reopens_every_file() {
        let rom = std::env::var_os("LM_IMPORTED_GFX_ROM").expect("LM_IMPORTED_GFX_ROM");
        let directory =
            PathBuf::from(std::env::var_os("LM_GFX_EXPORT_DIR").expect("LM_GFX_EXPORT_DIR"));
        let image = RomImage::from_bytes(fs::read(rom).unwrap()).unwrap();
        let project = Project::new(image.clone());
        let ordinary = lm_profile::smw_us_v1_vanilla_graphics_layout();
        let special = lm_profile::smw_us_v1_special_graphics_layouts(&image).unwrap();
        for file_number in 0..0x34 {
            let (slot, layout) = match file_number {
                0x00..=0x31 => (file_number, ordinary),
                0x32 => (0, special.gfx32),
                0x33 => (0, special.gfx33),
                _ => unreachable!(),
            };
            let actual = project
                .load_decompressed_graphics_file(slot, layout)
                .unwrap();
            let expected = fs::read(directory.join(format!("GFX{file_number:02X}.bin"))).unwrap();
            assert_eq!(actual, expected, "GFX{file_number:02X}");
        }
    }

    #[test]
    fn level_batch_prefers_the_staged_4bpp_override_for_an_active_file() {
        let directory = temporary_directory();
        fs::create_dir(&directory).unwrap();
        let (mut source, _) = fixture_source();
        let staged = vec![0xa5; 0x1000];
        source.family = "level";
        source.encoding = GraphicsBatchEncoding::Decoded4Bpp;
        source.slots.truncate(1);
        source.file_numbers.truncate(1);
        source.raw_4bpp_overrides = vec![(0, staged.clone())];
        export_batch(
            source,
            &GraphicsExportTarget::Directory(directory.clone()),
            &AtomicUsize::new(0),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(fs::read(directory.join("GFX00.bin")).unwrap(), staged);
        fs::remove_dir_all(directory).unwrap();
    }
}
