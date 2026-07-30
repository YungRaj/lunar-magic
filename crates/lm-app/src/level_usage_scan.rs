use std::ops::ControlFlow;

use lm_level::NativeLayer2Data;
use lm_profile::{
    VanillaObjectFamily, load_smw_us_v1_standard_object_definition_map, smw_us_v1_level_mode,
    smw_us_v1_object_family, smw_us_v1_object_tileset_graphics_files,
    smw_us_v1_sprite_tileset_graphics_files,
};
use lm_project::Project;
use lm_render::{
    NativeLevelMap16Layout, StandardObjectDefinitionSet,
    install_lunar_magic_shared_extended_objects, install_lunar_magic_shared_standard_objects,
    install_lunar_magic_tileset_extended_objects, render_mapped_standard_object_stream,
};
use lm_rom::RomImage;

use crate::{
    LevelUsageAccumulator, LevelUsageAnalysisError, LevelUsageReport, ProfiledControllerSnapshot,
};

/// Resource domains selected in Lunar Magic's level-usage dialog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct LevelUsageScanOptions {
    pub map16: bool,
    pub only_unused_defined_map16: bool,
    pub graphics: bool,
    pub only_unused_inserted_graphics: bool,
    pub sprites: bool,
}

impl Default for LevelUsageScanOptions {
    fn default() -> Self {
        Self {
            map16: true,
            only_unused_defined_map16: false,
            graphics: true,
            only_unused_inserted_graphics: false,
            sprites: true,
        }
    }
}

/// One nonfatal per-level failure retained by an all-level scan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LevelUsageScanDiagnostic {
    pub level: usize,
    pub stage: LevelUsageScanStage,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LevelUsageScanStage {
    Load,
    RenderLayer1,
    LoadLayer2,
    Graphics,
    Sprites,
}

/// Progress delivered before each slot and once after the final slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelUsageScanProgress {
    pub completed: usize,
    pub total: usize,
    pub current_level: Option<usize>,
    pub loaded: usize,
    pub skipped: usize,
}

/// Completed all-level analysis plus evidence about slots Lunar Magic could not load completely.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LevelUsageScanResult {
    pub report: LevelUsageReport,
    pub loaded_levels: Vec<bool>,
    pub diagnostics: Vec<LevelUsageScanDiagnostic>,
}

impl LevelUsageScanResult {
    #[must_use]
    pub fn loaded_count(&self) -> usize {
        self.loaded_levels.iter().filter(|loaded| **loaded).count()
    }

    #[must_use]
    pub fn skipped_count(&self) -> usize {
        self.loaded_levels.len() - self.loaded_count()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LevelUsageScanError {
    Profile(String),
    Rom(String),
    ObjectDefinitions(String),
    Definitions(String),
    Analysis(LevelUsageAnalysisError),
    Cancelled { completed: usize, total: usize },
}

impl std::fmt::Display for LevelUsageScanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "cannot scan level usage: {self:?}")
    }
}

impl std::error::Error for LevelUsageScanError {}

impl From<LevelUsageAnalysisError> for LevelUsageScanError {
    fn from(value: LevelUsageAnalysisError) -> Self {
        Self::Analysis(value)
    }
}

/// Scans every profile-declared level slot using the native SMW-US loaders and object renderer.
///
/// Lunar Magic continues after an individual slot cannot be loaded. This function does the same:
/// initialization failures are returned, while slot-local failures are retained as diagnostics.
/// The callback runs before every slot, making cancellation deterministic and toolkit-neutral.
///
/// Music is deliberately not included yet. Lunar Magic resolves an explicit zero through a
/// runtime-populated default table; that resolver must be recovered rather than guessed.
///
/// # Errors
///
/// Rejects a mismatched profile/ROM, malformed global object maps, counter failures, or callback
/// cancellation. Individual level failures do not abort the scan.
#[allow(clippy::too_many_lines)]
pub fn scan_smw_us_v1_level_usage(
    source: &ProfiledControllerSnapshot,
    options: LevelUsageScanOptions,
    mut progress: impl FnMut(LevelUsageScanProgress) -> ControlFlow<()>,
) -> Result<LevelUsageScanResult, LevelUsageScanError> {
    source
        .profile
        .ensure_identity(&source.snapshot.identity)
        .map_err(|error| LevelUsageScanError::Profile(error.to_string()))?;
    let image = RomImage::from_bytes(source.snapshot.rom_bytes.clone())
        .map_err(|error| LevelUsageScanError::Rom(error.to_string()))?;
    let layout = source
        .profile
        .level_layout_for_rom(&image)
        .map_err(|error| LevelUsageScanError::Rom(error.to_string()))?;
    let object_map = load_smw_us_v1_standard_object_definition_map(&image)
        .map_err(|error| LevelUsageScanError::ObjectDefinitions(error.to_string()))?;
    let mut shared_definitions = StandardObjectDefinitionSet::empty();
    install_lunar_magic_shared_extended_objects(&mut shared_definitions)
        .and_then(|()| install_lunar_magic_shared_standard_objects(&mut shared_definitions))
        .map_err(|error| LevelUsageScanError::Definitions(error.to_string()))?;

    let project = Project::new(image);
    let total = layout.layer1.entries;
    let mut accumulator = LevelUsageAccumulator::new(total)?;
    let mut loaded_levels = vec![false; total];
    let mut diagnostics = Vec::new();
    let mut loaded = 0;

    for (level, loaded_level) in loaded_levels.iter_mut().enumerate() {
        let scan_progress = LevelUsageScanProgress {
            completed: level,
            total,
            current_level: Some(level),
            loaded,
            skipped: level - loaded,
        };
        if progress(scan_progress).is_break() {
            return Err(LevelUsageScanError::Cancelled {
                completed: level,
                total,
            });
        }

        let slot = match project.load_level_slot(level, layout, &source.profile.sprite_lengths) {
            Ok(slot) => slot,
            Err(error) => {
                diagnostics.push(diagnostic(level, LevelUsageScanStage::Load, error));
                continue;
            }
        };
        let mode = smw_us_v1_level_mode(slot.layer1.header.level_mode());
        let has_editor_canvas = mode.editor_major_screens != 0;
        if !has_editor_canvas {
            diagnostics.push(LevelUsageScanDiagnostic {
                level,
                stage: LevelUsageScanStage::Load,
                detail: format!(
                    "level mode {:02X} has no editor canvas",
                    slot.layer1.header.level_mode()
                ),
            });
        }
        *loaded_level = true;
        loaded += 1;

        if options.map16 {
            let tileset = slot.layer1.header.object_tileset();
            let family = object_family_index(smw_us_v1_object_family(tileset));
            let mut definitions = shared_definitions.clone();
            let layer1_cache =
                match install_lunar_magic_tileset_extended_objects(&mut definitions, tileset)
                    .and_then(|()| {
                        let handler_map = object_map.family(family).ok_or(
                            lm_render::StandardObjectRenderError::InvalidCommand(tileset),
                        )?;
                        render_mapped_standard_object_stream(
                            &slot.layer1.objects,
                            &definitions,
                            handler_map,
                            maximum_level_layout(slot.layer1.header.level_mode()),
                            0x25,
                        )
                    }) {
                    Ok(rendered) => {
                        if !rendered.missing_commands.is_empty()
                            || !rendered.missing_extended_objects.is_empty()
                        {
                            diagnostics.push(LevelUsageScanDiagnostic {
                                level,
                                stage: LevelUsageScanStage::RenderLayer1,
                                detail: format!(
                                    "unresolved commands {:?}, extended objects {:?}",
                                    rendered.missing_commands, rendered.missing_extended_objects
                                ),
                            });
                        }
                        Some(rendered.cache)
                    }
                    Err(error) => {
                        diagnostics.push(diagnostic(
                            level,
                            LevelUsageScanStage::RenderLayer1,
                            error,
                        ));
                        None
                    }
                };

            let layer2 = source.profile.layer2.and_then(|layer2_layout| {
                match project.load_level_layer2_with_descriptor(
                    level,
                    slot.layer1.header.level_mode(),
                    layer2_layout,
                ) {
                    Ok(loaded) => match loaded.data {
                        NativeLayer2Data::Tilemap(bytes) => Some((
                            bytes
                                .chunks_exact(2)
                                .map(|word| u16::from_le_bytes([word[0], word[1]]))
                                .collect::<Vec<_>>(),
                            loaded
                                .descriptor
                                .map_or(0, lm_level::MwlLayer2Descriptor::active_bank),
                        )),
                        NativeLayer2Data::Objects(_) => None,
                    },
                    Err(error) => {
                        diagnostics.push(diagnostic(level, LevelUsageScanStage::LoadLayer2, error));
                        None
                    }
                }
            });
            if let Some(cache) = layer1_cache {
                accumulator.observe_map16(
                    level,
                    cache.cells(),
                    layer2
                        .as_ref()
                        .map(|(words, bank)| (words.as_slice(), *bank)),
                )?;
            }
        }

        if options.graphics && has_editor_canvas {
            let object_tileset = usize::from(slot.layer1.header.object_tileset());
            let sprite_tileset = usize::from(slot.layer1.header.sprite_tileset());
            match (
                smw_us_v1_object_tileset_graphics_files(&project.rom, object_tileset),
                smw_us_v1_sprite_tileset_graphics_files(&project.rom, sprite_tileset),
            ) {
                (Ok(object), Ok(sprite)) => {
                    let files = object
                        .into_iter()
                        .chain(sprite)
                        .chain([0x33, 0x32, 0x28, 0x29, 0x2a, 0x2b])
                        .filter(|file| *file != 0x7f)
                        .filter_map(|file| u16::try_from(file).ok());
                    accumulator.observe_graphics(level, files)?;
                }
                (object, sprite) => diagnostics.push(LevelUsageScanDiagnostic {
                    level,
                    stage: LevelUsageScanStage::Graphics,
                    detail: format!("object assignment {object:?}; sprite assignment {sprite:?}"),
                }),
            }
        }

        if options.sprites {
            if let Err(error) = accumulator.observe_sprites(level, &slot.sprites) {
                diagnostics.push(diagnostic(level, LevelUsageScanStage::Sprites, error));
            }
        }
    }

    let _ = progress(LevelUsageScanProgress {
        completed: total,
        total,
        current_level: None,
        loaded,
        skipped: total - loaded,
    });
    let mut report = accumulator.finish(
        options
            .map16
            .then_some((0_u32..0x200).chain(0x8000..0x8200))
            .into_iter()
            .flatten(),
        options
            .graphics
            .then_some(0_u32..=0x33)
            .into_iter()
            .flatten(),
    );
    if options.only_unused_defined_map16 {
        report.map16_tiles.retain(|entry| entry.count == 0);
    }
    if options.only_unused_inserted_graphics {
        report.graphics_files.retain(|entry| entry.count == 0);
    }
    if !options.map16 {
        report.map16_tiles.clear();
    }
    if !options.graphics {
        report.graphics_files.clear();
    }
    if !options.sprites {
        report.sprites.clear();
    }
    Ok(LevelUsageScanResult {
        report,
        loaded_levels,
        diagnostics,
    })
}

fn maximum_level_layout(level_mode: u8) -> NativeLevelMap16Layout {
    let vertical = smw_us_v1_level_mode(level_mode).vertical;
    NativeLevelMap16Layout {
        width: if vertical { 32 } else { 512 },
        height: if vertical { 512 } else { 32 },
        page_stride: 0x1b0,
        base_cell: 0,
        vertical,
    }
}

const fn object_family_index(family: VanillaObjectFamily) -> usize {
    match family {
        VanillaObjectFamily::Normal => 0,
        VanillaObjectFamily::Castle => 1,
        VanillaObjectFamily::Rope => 2,
        VanillaObjectFamily::Underground => 3,
        VanillaObjectFamily::GhostHouse => 4,
    }
}

fn diagnostic(
    level: usize,
    stage: LevelUsageScanStage,
    error: impl std::fmt::Display,
) -> LevelUsageScanDiagnostic {
    LevelUsageScanDiagnostic {
        level,
        stage,
        detail: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ControllerSnapshot, EditorMode};

    #[test]
    fn maximum_cache_layout_tracks_native_orientation() {
        assert_eq!(
            maximum_level_layout(0),
            NativeLevelMap16Layout {
                width: 512,
                height: 32,
                page_stride: 0x1b0,
                base_cell: 0,
                vertical: false,
            }
        );
        assert_eq!(
            maximum_level_layout(3),
            NativeLevelMap16Layout {
                width: 32,
                height: 512,
                page_stride: 0x1b0,
                base_cell: 0,
                vertical: true,
            }
        );
    }

    #[test]
    fn recovered_family_order_matches_definition_map_order() {
        assert_eq!(object_family_index(VanillaObjectFamily::Normal), 0);
        assert_eq!(object_family_index(VanillaObjectFamily::Castle), 1);
        assert_eq!(object_family_index(VanillaObjectFamily::Rope), 2);
        assert_eq!(object_family_index(VanillaObjectFamily::Underground), 3);
        assert_eq!(object_family_index(VanillaObjectFamily::GhostHouse), 4);
    }

    #[test]
    #[ignore = "requires the locally supplied Lunar Magic working ROM"]
    fn working_rom_scan_matches_authenticated_usage_domains() {
        let rom_bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../SMW-working.sfc"),
        )
        .unwrap();
        let image = RomImage::from_bytes(rom_bytes.clone()).unwrap();
        let identity = lm_rom::detect_identity(&image).unwrap();
        let mut profile = lm_profile::test_support::profile();
        profile.mapper = identity.mapper;
        profile.level = lm_profile::smw_us_v1_vanilla_level_layout();
        profile.layer2 = Some(lm_profile::smw_us_v1_vanilla_layer2_layout());
        let snapshot = ProfiledControllerSnapshot {
            snapshot: ControllerSnapshot {
                revision: 0,
                mode: EditorMode::Level(0x105),
                identity,
                document_path: None,
                rom_bytes,
            },
            profile,
        };
        let result =
            scan_smw_us_v1_level_usage(&snapshot, LevelUsageScanOptions::default(), |_| {
                ControlFlow::Continue(())
            })
            .unwrap();
        assert_eq!(result.loaded_count(), 0x200);
        for graphics in [0x28, 0x29, 0x2a, 0x2b, 0x32, 0x33] {
            let entry = result
                .report
                .graphics_files
                .iter()
                .find(|entry| entry.resource == graphics)
                .unwrap();
            assert_eq!(entry.count, 0x1e8);
        }
        let oracle = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../oracle-work/lm363/working-level-analysis/LevelAnalysis.txt"),
        )
        .unwrap();
        let expected = oracle
            .lines()
            .filter_map(|line| {
                let fields = line
                    .strip_prefix("Graphics File ")?
                    .split_whitespace()
                    .collect::<Vec<_>>();
                Some((
                    u32::from_str_radix(fields[0], 16).unwrap(),
                    u64::from_str_radix(fields[2], 16).unwrap(),
                ))
            })
            .collect::<Vec<_>>();
        let actual = result
            .report
            .graphics_files
            .iter()
            .map(|entry| (entry.resource, entry.count))
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        for (prefix, actual) in [("Sprite ", &result.report.sprites)] {
            let expected = oracle
                .lines()
                .filter_map(|line| {
                    let fields = line
                        .strip_prefix(prefix)?
                        .split_whitespace()
                        .collect::<Vec<_>>();
                    (fields.get(1) == Some(&"count:")).then_some(())?;
                    Some((
                        u32::from_str_radix(fields[0], 16).unwrap(),
                        u64::from_str_radix(fields[2], 16).unwrap(),
                    ))
                })
                .collect::<Vec<_>>();
            let actual = actual
                .iter()
                .map(|entry| (entry.resource, entry.count))
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{prefix}usage differs");
        }
        let expected_map16 = oracle
            .lines()
            .filter_map(|line| {
                let fields = line
                    .strip_prefix("Tile ")?
                    .split_whitespace()
                    .collect::<Vec<_>>();
                (fields.get(1) == Some(&"count:")).then_some(())?;
                Some((
                    u32::from_str_radix(fields[0], 16).unwrap(),
                    u64::from_str_radix(fields[2], 16).unwrap(),
                ))
            })
            .collect::<Vec<_>>();
        let actual_map16 = result
            .report
            .map16_tiles
            .iter()
            .map(|entry| (entry.resource, entry.count))
            .collect::<Vec<_>>();
        assert_eq!(
            actual_map16
                .iter()
                .map(|(resource, _)| *resource)
                .collect::<Vec<_>>(),
            expected_map16
                .iter()
                .map(|(resource, _)| *resource)
                .collect::<Vec<_>>()
        );
        let mismatched_counts = actual_map16
            .iter()
            .zip(&expected_map16)
            .filter(|(actual, expected)| actual.1 != expected.1)
            .count();
        eprintln!("Map16 resource counts still differing: {mismatched_counts}");
        // The audit deliberately keeps the remaining Layer 1 renderer gap visible while proving
        // that the scanner, Layer 2 namespace, graphics, and sprite domains use the oracle corpus.
        assert!(mismatched_counts < 200);
    }
}
