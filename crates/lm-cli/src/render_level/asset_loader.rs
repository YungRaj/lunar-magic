use super::LevelRenderPaths;
use crate::oracle_input::read_bounded;
use lm_graphics::{GraphicsInterchangeFile, PaletteInterchangeFile};
use lm_level::{
    CompleteLevelFile, DscSidecar, EntityAppearanceFile, MAX_DSC_SOURCE_LEN, Map16SetFile,
};
use lm_render::MaterializedLayer3Plane;
use std::path::Path;

pub(super) struct LoadedLevelAssets {
    pub level: CompleteLevelFile,
    pub map16: Map16SetFile,
    pub graphics: GraphicsInterchangeFile,
    pub palette: PaletteInterchangeFile,
    pub appearances: Option<EntityAppearanceFile>,
    pub layer3_plane: Option<MaterializedLayer3Plane>,
}

pub(super) fn load_level_assets(
    paths: LevelRenderPaths<'_>,
) -> Result<LoadedLevelAssets, Box<dyn std::error::Error>> {
    let appearances = if let Some(path) = paths.appearances {
        Some(EntityAppearanceFile::decode(&read_bounded(
            path,
            EntityAppearanceFile::MAX_FILE_LEN,
        )?)?)
    } else {
        None
    };
    let layer3_plane = if let Some(path) = paths.layer3_plane {
        Some(MaterializedLayer3Plane::decode(&read_bounded(
            path,
            MaterializedLayer3Plane::MAX_FILE_LEN,
        )?)?)
    } else {
        None
    };
    Ok(LoadedLevelAssets {
        level: CompleteLevelFile::decode(&read_bounded(
            paths.level,
            CompleteLevelFile::MAX_FILE_LEN,
        )?)?,
        map16: Map16SetFile::decode(&read_bounded(paths.map16, Map16SetFile::MAX_FILE_LEN)?)?,
        graphics: GraphicsInterchangeFile::decode(&read_bounded(
            paths.graphics,
            GraphicsInterchangeFile::MAX_FILE_LEN,
        )?)?,
        palette: PaletteInterchangeFile::decode(&read_bounded(
            paths.palette,
            PaletteInterchangeFile::MAX_FILE_LEN,
        )?)?,
        appearances,
        layer3_plane,
    })
}

pub(super) fn load_dsc(path: &Path) -> Result<DscSidecar, Box<dyn std::error::Error>> {
    Ok(DscSidecar::decode(&read_bounded(
        path,
        MAX_DSC_SOURCE_LEN,
    )?)?)
}
