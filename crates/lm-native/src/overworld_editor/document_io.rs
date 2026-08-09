use super::{OverworldAssets, OverworldDocument, PendingOpen};
use crate::{
    animation_modes, dialogs,
    document_loader::{BoundedRead, LoadedDocument},
};
use lm_app::OverworldDocumentController;
use lm_graphics::{GraphicsInterchangeFile, PaletteOwnership};
use lm_level::Map16SetFile;
use lm_project::CompleteOverworldFile;

pub(super) fn choose_requests() -> Option<Vec<BoundedRead>> {
    let path = dialogs::choose_complete_overworld_document()?;
    let mode_path = dialogs::choose_exanimation_size_modes()?;
    let map16_path = dialogs::choose_map16_set_document()?;
    let graphics_path = dialogs::choose_graphics_document()?;
    Some(vec![
        BoundedRead::new(
            path,
            CompleteOverworldFile::MAX_FILE_LEN as u64,
            "complete overworld",
        ),
        BoundedRead::new(mode_path, 256, "ExAnimation size-mode table"),
        BoundedRead::new(
            map16_path,
            Map16SetFile::MAX_FILE_LEN as u64,
            "overworld Map16 set",
        ),
        BoundedRead::new(
            graphics_path,
            GraphicsInterchangeFile::MAX_FILE_LEN as u64,
            "overworld graphics",
        ),
    ])
}

pub(super) fn pending_from_loaded(loaded: LoadedDocument) -> Result<PendingOpen, String> {
    let [
        (path, bytes),
        (_, mode_bytes),
        (_, map16_bytes),
        (_, graphics_bytes),
    ] = loaded.into_exact::<4>("complete-overworld")?;
    Ok(PendingOpen {
        path,
        bytes,
        modes: animation_modes::decode(&mode_bytes)?,
        assets: OverworldAssets {
            map16: Map16SetFile::decode(&map16_bytes).map_err(|error| error.to_string())?,
            graphics: GraphicsInterchangeFile::decode(&graphics_bytes)
                .map_err(|error| error.to_string())?,
            native_sprite_graphics_cache: Vec::new(),
            external_sprite_assets: lm_graphics::ExternalSpriteAssets::default(),
            gfx32: Vec::new(),
            gfx33: Vec::new(),
            built_in_animation_addresses: Vec::new(),
            built_in_level_dot_palette: None,
            built_in_lightning: None,
            animation_options: crate::overworld_editor_render::vanilla_overworld_animation_options(
            ),
        },
        maximum_records: "32".into(),
    })
}

pub(super) fn decode_document(
    pending: PendingOpen,
    maximum_records: usize,
) -> Result<OverworldDocument, Box<(String, PendingOpen)>> {
    if maximum_records == 0 || maximum_records > 255 {
        return Err(Box::new((
            "maximum animation record count must be between 1 and 255".into(),
            pending,
        )));
    }
    match OverworldDocumentController::decode(
        pending.path.clone(),
        &pending.bytes,
        maximum_records,
        &pending.modes,
    ) {
        Ok(controller) => {
            let ownership =
                PaletteOwnership::editable(controller.value().data.palette.colors.len());
            Ok(OverworldDocument {
                controller,
                modes: pending.modes,
                ownership,
                assets: pending.assets,
            })
        }
        Err(error) => Err(Box::new((error.to_string(), pending))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_overworld_load_requires_all_four_ordered_inputs() {
        assert!(pending_from_loaded(LoadedDocument { files: Vec::new() }).is_err());
        assert!(
            pending_from_loaded(LoadedDocument {
                files: vec![
                    (std::path::PathBuf::from("world.lmow"), Vec::new()),
                    (std::path::PathBuf::from("modes.bin"), vec![0; 255]),
                    (std::path::PathBuf::from("map16.lm16set"), Vec::new()),
                    (std::path::PathBuf::from("graphics.lmgfx"), Vec::new()),
                ],
            })
            .is_err()
        );
    }
}
