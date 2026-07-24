use lm_app::{
    CompleteLevelDocumentController, CustomObjectLibraryController, CustomSpriteLibraryController,
    DscSidecarController, EntityAppearanceDocumentController, ExAnimationDocumentController,
    ExpandedSettingsDocumentController, GraphicsDocumentController, Layer3DocumentController,
    Map16DocumentController, Map16PageDocumentController, MwlDocumentController,
    NativeLevelAssetsDocumentController, NativeLevelDocumentController,
    NativeMap16SidecarController, OverworldAppearanceDocumentController,
    OverworldDocumentController, OverworldMetadataController, OverworldPathController,
    PaletteDocumentController,
};

/// All independent portable documents owned by the runnable frontend.
#[derive(Default)]
pub(crate) struct PortableDocumentSessions {
    pub custom_objects: Option<CustomObjectLibraryController>,
    pub custom_sprites: Option<CustomSpriteLibraryController>,
    pub dsc_sidecar: Option<DscSidecarController>,
    pub metadata: Option<OverworldMetadataController>,
    pub paths: Option<OverworldPathController>,
    pub layer3: Option<Layer3DocumentController>,
    pub expanded_settings: Option<ExpandedSettingsDocumentController>,
    pub complete_level: Option<CompleteLevelDocumentController>,
    pub map16: Option<Map16DocumentController>,
    pub map16_page: Option<Map16PageDocumentController>,
    pub overworld: Option<OverworldDocumentController>,
    pub overworld_appearances: Option<OverworldAppearanceDocumentController>,
    pub graphics: Option<GraphicsDocumentController>,
    pub palette: Option<PaletteDocumentController>,
    pub exanimation: Option<ExAnimationDocumentController>,
    pub entity_appearances: Option<EntityAppearanceDocumentController>,
    pub mwl: Option<MwlDocumentController>,
    pub native_level: Option<NativeLevelDocumentController>,
    pub native_assets: Option<NativeLevelAssetsDocumentController>,
    pub native_map16_sidecar: Option<NativeMap16SidecarController>,
}

impl PortableDocumentSessions {
    pub fn dirty_documents(&self) -> Vec<&'static str> {
        let mut names = self.dirty_level_documents();
        names.extend(self.dirty_asset_documents());
        names
    }

    fn dirty_level_documents(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self
            .custom_objects
            .as_ref()
            .is_some_and(CustomObjectLibraryController::is_modified)
        {
            names.push("custom objects");
        }
        if self
            .custom_sprites
            .as_ref()
            .is_some_and(CustomSpriteLibraryController::is_modified)
        {
            names.push("custom sprites");
        }
        if self
            .dsc_sidecar
            .as_ref()
            .is_some_and(DscSidecarController::is_modified)
        {
            names.push("DSC sidecar");
        }
        if self
            .metadata
            .as_ref()
            .is_some_and(OverworldMetadataController::is_modified)
        {
            names.push("overworld metadata");
        }
        if self
            .paths
            .as_ref()
            .is_some_and(OverworldPathController::is_modified)
        {
            names.push("overworld paths");
        }
        if self
            .layer3
            .as_ref()
            .is_some_and(Layer3DocumentController::is_modified)
        {
            names.push("Layer 3");
        }
        if self
            .expanded_settings
            .as_ref()
            .is_some_and(ExpandedSettingsDocumentController::is_modified)
        {
            names.push("expanded settings");
        }
        if self
            .complete_level
            .as_ref()
            .is_some_and(CompleteLevelDocumentController::is_modified)
        {
            names.push("complete level");
        }
        if self
            .map16
            .as_ref()
            .is_some_and(Map16DocumentController::is_modified)
        {
            names.push("complete Map16");
        }
        if self
            .map16_page
            .as_ref()
            .is_some_and(Map16PageDocumentController::is_modified)
        {
            names.push("Map16 page");
        }
        if self
            .overworld
            .as_ref()
            .is_some_and(OverworldDocumentController::is_modified)
        {
            names.push("complete overworld");
        }
        if self
            .mwl
            .as_ref()
            .is_some_and(MwlDocumentController::is_modified)
        {
            names.push("MWL level");
        }
        if self
            .native_level
            .as_ref()
            .is_some_and(NativeLevelDocumentController::is_modified)
        {
            names.push("native-level transfer");
        }
        if self
            .native_map16_sidecar
            .as_ref()
            .is_some_and(NativeMap16SidecarController::is_modified)
        {
            names.push("native Map16 sidecar");
        }
        names
    }

    fn dirty_asset_documents(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self
            .native_assets
            .as_ref()
            .is_some_and(NativeLevelAssetsDocumentController::is_modified)
        {
            names.push("native level assets");
        }
        if self
            .overworld_appearances
            .as_ref()
            .is_some_and(OverworldAppearanceDocumentController::is_modified)
        {
            names.push("overworld appearances");
        }
        if self
            .graphics
            .as_ref()
            .is_some_and(GraphicsDocumentController::is_modified)
        {
            names.push("graphics");
        }
        if self
            .palette
            .as_ref()
            .is_some_and(PaletteDocumentController::is_modified)
        {
            names.push("palette");
        }
        if self
            .exanimation
            .as_ref()
            .is_some_and(ExAnimationDocumentController::is_modified)
        {
            names.push("ExAnimation");
        }
        if self
            .entity_appearances
            .as_ref()
            .is_some_and(EntityAppearanceDocumentController::is_modified)
        {
            names.push("entity appearances");
        }
        names
    }

    pub fn has_dirty_documents(&self) -> bool {
        !self.dirty_documents().is_empty()
    }

    pub fn discard_all(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_app::Map16DocumentEdit;
    use lm_level::{
        CompleteLevelFile, Entrance, EntranceKind, Level, LevelAuxiliaryEdit, LevelObjectData,
        Map16Address, Map16Page, Map16Quadrant, Map16Set, Map16SetFile, Map16Tile, MwlFile,
        MwlLevelHeaderSection, MwlSection, MwlSectionKind, NativeLevelFile, NativeSpriteStream,
        SequenceEdit, SpriteLengthTable, Subtile,
    };

    #[test]
    fn complete_level_dirty_state_is_reported_and_discarded() {
        let file = CompleteLevelFile(Level::default());
        let mut controller = CompleteLevelDocumentController::decode(
            "level.lmlevel".into(),
            &file.encode().unwrap(),
        )
        .unwrap();
        controller
            .apply_auxiliary_edits(
                0,
                &[LevelAuxiliaryEdit::Entrance(SequenceEdit::Insert {
                    index: 0,
                    value: Entrance {
                        kind: EntranceKind::Main,
                        x: 0,
                        y: 0,
                        screen: 0,
                        action: 0,
                        raw_flags: 0,
                    },
                })],
            )
            .unwrap();
        let mut sessions = PortableDocumentSessions {
            complete_level: Some(controller),
            ..PortableDocumentSessions::default()
        };
        assert_eq!(sessions.dirty_documents(), ["complete level"]);
        sessions.discard_all();
        assert!(!sessions.has_dirty_documents());
    }

    #[test]
    fn complete_map16_dirty_state_participates_in_shutdown() {
        let file = Map16SetFile {
            set: Map16Set {
                pages: vec![
                    Map16Page::new(vec![Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap(),
                ],
            },
        };
        let mut controller =
            Map16DocumentController::decode("all.lm16set".into(), &file.encode().unwrap()).unwrap();
        controller
            .apply_edits(
                0,
                &[Map16DocumentEdit::SetSubtile {
                    address: Map16Address { page: 0, tile: 0 },
                    quadrant: Map16Quadrant::TopLeft,
                    subtile: Subtile(1),
                    resolution_limit: Map16Page::TILE_COUNT,
                }],
            )
            .unwrap();
        let sessions = PortableDocumentSessions {
            map16: Some(controller),
            ..PortableDocumentSessions::default()
        };
        assert_eq!(sessions.dirty_documents(), ["complete Map16"]);
    }

    #[test]
    fn mwl_dirty_state_participates_in_shutdown() {
        let mut sections: [MwlSection; MwlFile::SECTION_COUNT] =
            std::array::from_fn(|_| MwlSection::default());
        sections[MwlSectionKind::LevelHeader as usize].bytes =
            vec![0; MwlLevelHeaderSection::ENCODED_LEN];
        let file = MwlFile {
            version: MwlFile::CURRENT_VERSION,
            flags: 0,
            attribution: [0; MwlFile::ATTRIBUTION_LEN],
            sections,
        };
        let mut controller =
            MwlDocumentController::decode("level.mwl".into(), &file.encode().unwrap()).unwrap();
        controller
            .apply_edits(0, &[lm_app::MwlDocumentEdit::SetFlags(1)])
            .unwrap();
        let sessions = PortableDocumentSessions {
            mwl: Some(controller),
            ..PortableDocumentSessions::default()
        };
        assert_eq!(sessions.dirty_documents(), ["MWL level"]);
    }

    #[test]
    fn native_level_transfer_dirty_state_participates_in_shutdown() {
        let lengths = SpriteLengthTable::standard();
        let file = NativeLevelFile {
            source_level: 0x105,
            layer1: LevelObjectData::parse(&[0, 0, 0, 0, 0, 1, 2, 3, 0xff]).unwrap(),
            sprites: NativeSpriteStream::parse(&[0, 0, 0, 1, 0xff], false, &lengths).unwrap(),
        };
        let mut controller = NativeLevelDocumentController::decode(
            "level.lmlvl".into(),
            &file.encode().unwrap(),
            lengths,
        )
        .unwrap();
        controller
            .apply_edits(0, &[lm_app::NativeLevelEdit::SetSpriteHeader(1)])
            .unwrap();
        let sessions = PortableDocumentSessions {
            native_level: Some(controller),
            ..PortableDocumentSessions::default()
        };
        assert_eq!(sessions.dirty_documents(), ["native-level transfer"]);
    }

    #[test]
    fn standalone_map16_page_dirty_state_participates_in_shutdown() {
        let file = lm_level::Map16PageFile {
            source_page: 1,
            page: Map16Page::new(vec![Map16Tile::default(); Map16Page::TILE_COUNT]).unwrap(),
        };
        let mut controller =
            Map16PageDocumentController::decode("page.map16".into(), &file.encode().unwrap())
                .unwrap();
        controller
            .apply_edits(
                0,
                &[lm_app::Map16PageDocumentEdit::SetActsLike { tile: 0, value: 1 }],
            )
            .unwrap();
        let sessions = PortableDocumentSessions {
            map16_page: Some(controller),
            ..PortableDocumentSessions::default()
        };
        assert_eq!(sessions.dirty_documents(), ["Map16 page"]);
    }

    #[test]
    fn entity_appearance_dirty_state_participates_in_shutdown() {
        let file = lm_level::EntityAppearanceFile {
            appearances: vec![],
        };
        let mut controller = EntityAppearanceDocumentController::decode(
            "entities.lmentapp".into(),
            &file.encode().unwrap(),
        )
        .unwrap();
        controller
            .apply_edits(
                0,
                &[lm_app::EntityAppearanceDocumentEdit::Insert {
                    index: 0,
                    value: lm_level::EntityAppearanceRecord {
                        source: lm_level::AppearanceSource::Sprite(1),
                        tile_index: 0,
                        palette_index: 0,
                        x: 0,
                        y: 0,
                        x_flip: false,
                        y_flip: false,
                    },
                }],
            )
            .unwrap();
        let sessions = PortableDocumentSessions {
            entity_appearances: Some(controller),
            ..PortableDocumentSessions::default()
        };
        assert_eq!(sessions.dirty_documents(), ["entity appearances"]);
    }

    #[test]
    fn overworld_appearance_dirty_state_participates_in_shutdown() {
        let file = lm_overworld::SpriteAppearanceFile {
            definitions: vec![],
        };
        let mut controller = OverworldAppearanceDocumentController::decode(
            "sprites.lmowapp".into(),
            &file.encode().unwrap(),
        )
        .unwrap();
        controller
            .apply_edits(
                0,
                &[lm_app::OverworldAppearanceDocumentEdit::InsertDefinition {
                    index: 0,
                    sprite_id: 1,
                }],
            )
            .unwrap();
        let sessions = PortableDocumentSessions {
            overworld_appearances: Some(controller),
            ..PortableDocumentSessions::default()
        };
        assert_eq!(sessions.dirty_documents(), ["overworld appearances"]);
    }
}
