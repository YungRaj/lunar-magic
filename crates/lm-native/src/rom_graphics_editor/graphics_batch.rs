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
    JoinedFile(PathBuf),
}

impl GraphicsExportTarget {
    fn path(&self) -> &Path {
        match self {
            Self::Directory(path) | Self::JoinedFile(path) => path,
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
    let mut group = matches!(target, GraphicsExportTarget::Directory(_))
        .then_some(lm_app::file_persistence::NewFileGroup::new());
    let mut joined_files = Vec::with_capacity(if group.is_some() { 0 } else { total });
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
        let graphics = project
            .load_decompressed_graphics_file(slot, source.layout)
            .map_err(|error| format!("{}: {error}", graphics_file_name(file_number)))?;
        let bytes = graphics;
        match target {
            GraphicsExportTarget::Directory(directory) => group
                .as_mut()
                .expect("directory exports create a staging group")
                .stage(&batch_output_path(directory, file_number), &bytes)
                .map_err(|error| format!("GFX{file_number:02X}: {error}"))?,
            GraphicsExportTarget::JoinedFile(_) => joined_files.push(bytes),
        }
        completed.store(index + 1, Ordering::Relaxed);
    }
    if cancelled.load(Ordering::Relaxed) {
        return Ok(None);
    }
    match target {
        GraphicsExportTarget::Directory(_) => {
            group
                .expect("directory exports retain their staging group")
                .publish()
                .map_err(|error| error.to_string())?;
        }
        GraphicsExportTarget::JoinedFile(path) => {
            let joined = lm_graphics::JoinedGraphics {
                files: joined_files,
            }
            .join()
            .map_err(|error| error.to_string())?;
            lm_app::file_persistence::write_new(path, &joined)
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(Some(total))
}

fn batch_output_path(directory: &Path, slot: usize) -> PathBuf {
    directory.join(graphics_file_name(slot))
}

fn graphics_file_name(slot: usize) -> String {
    let prefix = if slot < 0x80 { "GFX" } else { "ExGFX" };
    format!("{prefix}{slot:02X}.bin")
}

#[cfg(test)]
mod tests {
    use super::{
        GraphicsBatchSource, GraphicsExportTarget, RunningBatch, batch_output_path, export_batch,
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
            },
            expected,
        )
    }

    #[test]
    fn batch_names_use_fixed_uppercase_standard_gfx_numbers() {
        assert_eq!(
            batch_output_path(Path::new("/tmp/Graphics"), 0),
            Path::new("/tmp/Graphics/GFX00.bin")
        );
        assert_eq!(
            batch_output_path(Path::new("/tmp/Graphics"), 0x31),
            Path::new("/tmp/Graphics/GFX31.bin")
        );
        assert_eq!(
            batch_output_path(Path::new("/tmp/Graphics"), 0x33),
            Path::new("/tmp/Graphics/GFX33.bin")
        );
        assert_eq!(
            batch_output_path(Path::new("/tmp/Graphics"), 0x80),
            Path::new("/tmp/Graphics/ExGFX80.bin")
        );
        assert_eq!(
            batch_output_path(Path::new("/tmp/Graphics"), 0xfff),
            Path::new("/tmp/Graphics/ExGFXFFF.bin")
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
                source,
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
    fn late_destination_collision_leaves_no_partial_batch() {
        let directory = temporary_directory();
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("GFX01.bin"), b"keep").unwrap();
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
        assert_eq!(fs::read(directory.join("GFX01.bin")).unwrap(), b"keep");
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
            fs::read(path).unwrap(),
            [expected[0].clone(), expected[1].clone()].concat()
        );
        fs::remove_dir_all(directory).unwrap();
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
}
