use crate::portable_value_history::PortableValueHistory;
use lm_level::{
    ExpandedLevelSettingsError, Layer2ScrollSettings, Layer3TilemapGraphicsDescriptor,
    LevelEditError, LevelObjectData, MwlError, MwlFile, MwlLevelHeaderSection,
    MwlMainEntranceSettings, MwlMidwayEntranceSettings, MwlSection, MwlSectionKind,
    NativeSpriteEncodingError, NativeSpriteStream, ObjectStreamError, SpriteLengthTable,
    SpriteStreamError,
};
use lm_project::{
    MwlOptionalAssetsEdit, MwlOptionalAssetsEditError, MwlOptionalLevelAssets,
    MwlOptionalLevelAssetsError, apply_mwl_optional_assets_edit,
};
use std::fmt;
use std::path::PathBuf;

/// One semantic mutation of a portable MWL document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MwlDocumentEdit {
    SetFlags(u32),
    SetAttribution([u8; MwlFile::ATTRIBUTION_LEN]),
    SetLevelNumber(u16),
    SetMainEntrance(MwlMainEntranceSettings),
    SetMidwayEntrance(MwlMidwayEntranceSettings),
    SetLayer2Scroll(Layer2ScrollSettings),
    ReplaceSection {
        section: MwlSectionKind,
        bytes: Vec<u8>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MwlDocumentSaveSnapshot {
    pub request_id: u64,
    pub revision: u64,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
struct PendingSave {
    request_id: u64,
    value: MwlFile,
}

/// Revisioned, toolkit-neutral owner for one lossless MWL document.
#[derive(Clone, Debug)]
pub struct MwlDocumentController {
    path: PathBuf,
    value: MwlFile,
    saved: MwlFile,
    revision: u64,
    next_save_request: u64,
    pending_save: Option<PendingSave>,
    history: PortableValueHistory<MwlFile>,
}

impl MwlDocumentController {
    pub const HISTORY_LIMIT: usize = 100;

    /// Decodes one bounded MWL container while preserving every section byte.
    ///
    /// # Errors
    ///
    /// Returns [`MwlDocumentControllerError::File`] for malformed input.
    pub fn decode(path: PathBuf, bytes: &[u8]) -> Result<Self, MwlDocumentControllerError> {
        let value = MwlFile::decode(bytes).map_err(MwlDocumentControllerError::File)?;
        Ok(Self {
            path,
            saved: value.clone(),
            value,
            revision: 0,
            next_save_request: 0,
            pending_save: None,
            history: PortableValueHistory::with_limit(Self::HISTORY_LIMIT),
        })
    }

    #[must_use]
    pub const fn value(&self) -> &MwlFile {
        &self.value
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn is_modified(&self) -> bool {
        self.value != self.saved
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    #[must_use]
    pub const fn save_pending(&self) -> bool {
        self.pending_save.is_some()
    }

    /// Applies an ordered edit batch to a staged clone and commits it only after canonical
    /// encoding and decoding prove the complete resulting container.
    ///
    /// # Errors
    ///
    /// Returns a stale revision, malformed level-header section, file bound, or revision error.
    pub fn apply_edits(
        &mut self,
        expected_revision: u64,
        edits: &[MwlDocumentEdit],
    ) -> Result<(), MwlDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(MwlDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.value.clone();
        for (command, edit) in edits.iter().enumerate() {
            apply_edit(&mut staged, edit)
                .map_err(|error| MwlDocumentControllerError::Edit { command, error })?;
        }
        self.commit_staged(&staged)
    }

    /// Imports typed palette and `ExAnimation` sections from another MWL container as one
    /// revision while preserving every unrelated section in this document.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions, malformed optional sections, incompatible animation modes, and
    /// noncanonical results without changing the document or its history.
    pub fn import_optional_assets(
        &mut self,
        expected_revision: u64,
        source: &MwlFile,
        maximum_animation_records: usize,
        double_size_modes: &[bool],
    ) -> Result<(), MwlDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(MwlDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let assets =
            MwlOptionalLevelAssets::decode(source, maximum_animation_records, double_size_modes)
                .map_err(MwlDocumentControllerError::OptionalAssets)?;
        self.replace_optional_assets(
            expected_revision,
            &assets,
            maximum_animation_records,
            double_size_modes,
        )
    }

    /// Replaces both typed optional-asset sections as one canonical document revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions, invalid palette or animation shapes, and noncanonical results
    /// without changing the document or its history.
    pub fn replace_optional_assets(
        &mut self,
        expected_revision: u64,
        assets: &MwlOptionalLevelAssets,
        maximum_animation_records: usize,
        double_size_modes: &[bool],
    ) -> Result<(), MwlDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(MwlDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.value.clone();
        assets
            .install_into(&mut staged, double_size_modes)
            .map_err(MwlDocumentControllerError::OptionalAssets)?;
        MwlOptionalLevelAssets::decode(&staged, maximum_animation_records, double_size_modes)
            .map_err(MwlDocumentControllerError::OptionalAssets)?;
        self.commit_staged(&staged)
    }

    /// Applies an ordered semantic optional-assets edit batch to a staged aggregate and commits
    /// both MWL sections only after limit validation and canonical reopen.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions, invalid indexes or animation state, interpretation mismatches,
    /// record-limit violations, and noncanonical results without changing history.
    pub fn apply_optional_assets_edits(
        &mut self,
        expected_revision: u64,
        maximum_animation_records: usize,
        double_size_modes: &[bool],
        edits: &[MwlOptionalAssetsEdit],
    ) -> Result<(), MwlDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(MwlDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut assets = MwlOptionalLevelAssets::decode(
            &self.value,
            maximum_animation_records,
            double_size_modes,
        )
        .map_err(MwlDocumentControllerError::OptionalAssets)?;
        for (command, edit) in edits.iter().enumerate() {
            apply_mwl_optional_assets_edit(&mut assets, double_size_modes, edit)
                .map_err(|error| MwlDocumentControllerError::OptionalEdit { command, error })?;
        }
        self.replace_optional_assets(
            expected_revision,
            &assets,
            maximum_animation_records,
            double_size_modes,
        )
    }

    /// Changes the verified custom Layer 3 fields in the exact MWL expanded-settings record.
    /// Every unrelated flag bit, opaque word, and MWL section is retained.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions, missing or malformed expanded-settings sections, and
    /// noncanonical results without changing the document or its history.
    pub fn apply_layer3_settings(
        &mut self,
        expected_revision: u64,
        enabled: bool,
        descriptor: Layer3TilemapGraphicsDescriptor,
    ) -> Result<(), MwlDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(MwlDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let mut staged = self.value.clone();
        let mut settings = staged
            .expanded_settings_section()
            .map_err(MwlDocumentControllerError::ExpandedSettings)?;
        settings
            .set_layer3_tilemap_enabled(enabled)
            .map_err(MwlDocumentControllerError::ExpandedSettings)?;
        settings
            .set_layer3_tilemap_graphics_descriptor(descriptor)
            .map_err(MwlDocumentControllerError::ExpandedSettings)?;
        staged.set_expanded_settings_section(&settings);
        self.commit_staged(&staged)
    }

    /// Decodes the typed sprite stream carried by this MWL document.
    ///
    /// The expanded/legacy interpretation and record-length table are revision inputs rather than
    /// properties invented from opaque MWL metadata.
    ///
    /// # Errors
    ///
    /// Rejects a short common-prefix section or malformed sprite framing.
    pub fn sprites(
        &self,
        expanded: bool,
        lengths: &SpriteLengthTable,
    ) -> Result<NativeSpriteStream, MwlDocumentControllerError> {
        let section = self
            .value
            .payload_section(MwlSectionKind::Sprites)
            .map_err(MwlDocumentControllerError::SpriteSection)?;
        NativeSpriteStream::parse(&section.payload, expanded, lengths)
            .map_err(MwlDocumentControllerError::SpriteParse)
    }

    /// Replaces the typed MWL sprite stream while preserving its two opaque metadata words and
    /// every unrelated section.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions, malformed existing section framing, invalid record lengths or
    /// tokens, and any noncanonical encode/decode result without changing history.
    pub fn replace_sprites(
        &mut self,
        expected_revision: u64,
        sprites: &NativeSpriteStream,
        lengths: &SpriteLengthTable,
    ) -> Result<(), MwlDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(MwlDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let vertical = self.layer1()?.header.is_vertical();
        let mut canonical = sprites.clone();
        canonical
            .canonicalize_for_orientation(vertical)
            .map_err(MwlDocumentControllerError::SpriteCanonicalization)?;
        let encoded = canonical
            .encode_for_table(lengths)
            .map_err(MwlDocumentControllerError::SpriteEncoding)?;
        let reparsed = NativeSpriteStream::parse(&encoded, canonical.expanded, lengths)
            .map_err(MwlDocumentControllerError::SpriteParse)?;
        if reparsed != canonical {
            return Err(MwlDocumentControllerError::NonCanonicalSprites);
        }
        let mut staged = self.value.clone();
        let mut section = staged
            .payload_section(MwlSectionKind::Sprites)
            .map_err(MwlDocumentControllerError::SpriteSection)?;
        section.payload = encoded;
        staged
            .set_payload_section(MwlSectionKind::Sprites, &section)
            .map_err(MwlDocumentControllerError::SpriteSection)?;
        self.commit_staged(&staged)
    }

    /// Decodes the typed legacy header and Layer 1 object stream carried by this MWL document.
    ///
    /// # Errors
    ///
    /// Rejects a short common-prefix section or malformed object framing.
    pub fn layer1(&self) -> Result<LevelObjectData, MwlDocumentControllerError> {
        let section = self
            .value
            .payload_section(MwlSectionKind::Layer1)
            .map_err(MwlDocumentControllerError::Layer1Section)?;
        LevelObjectData::parse(&section.payload).map_err(MwlDocumentControllerError::Layer1Parse)
    }

    /// Replaces the typed MWL Layer 1 payload while preserving its opaque metadata and every
    /// unrelated section.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions, malformed existing section framing, invalid/noncanonical object
    /// streams, and single-bank overflow without changing document history.
    pub fn replace_layer1(
        &mut self,
        expected_revision: u64,
        layer1: &LevelObjectData,
    ) -> Result<(), MwlDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(MwlDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        let encoded = layer1
            .encode_banked()
            .map_err(MwlDocumentControllerError::Layer1Encoding)?;
        let reparsed =
            LevelObjectData::parse(&encoded).map_err(MwlDocumentControllerError::Layer1Parse)?;
        if reparsed != *layer1 {
            return Err(MwlDocumentControllerError::NonCanonicalLayer1);
        }
        let mut staged = self.value.clone();
        let mut section = staged
            .payload_section(MwlSectionKind::Layer1)
            .map_err(MwlDocumentControllerError::Layer1Section)?;
        section.payload = encoded;
        staged
            .set_payload_section(MwlSectionKind::Layer1, &section)
            .map_err(MwlDocumentControllerError::Layer1Section)?;
        self.commit_staged(&staged)
    }

    fn commit_staged(&mut self, staged: &MwlFile) -> Result<(), MwlDocumentControllerError> {
        if *staged == self.value {
            return Ok(());
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(MwlDocumentControllerError::RevisionOverflow)?;
        let encoded = staged.encode().map_err(MwlDocumentControllerError::File)?;
        let canonical = MwlFile::decode(&encoded).map_err(MwlDocumentControllerError::File)?;
        if canonical != *staged {
            return Err(MwlDocumentControllerError::CanonicalMismatch);
        }
        self.history.record(self.value.clone());
        self.value = canonical;
        self.revision = revision;
        Ok(())
    }

    /// Restores the previous complete canonical MWL value as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn undo(&mut self, expected_revision: u64) -> Result<bool, MwlDocumentControllerError> {
        self.navigate_history(expected_revision, true)
    }

    /// Reapplies the next reverted complete canonical MWL value as a new revision.
    ///
    /// # Errors
    ///
    /// Rejects stale revisions and revision overflow without changing history.
    pub fn redo(&mut self, expected_revision: u64) -> Result<bool, MwlDocumentControllerError> {
        self.navigate_history(expected_revision, false)
    }

    fn navigate_history(
        &mut self,
        expected_revision: u64,
        undo: bool,
    ) -> Result<bool, MwlDocumentControllerError> {
        if expected_revision != self.revision {
            return Err(MwlDocumentControllerError::StaleRevision {
                expected: expected_revision,
                actual: self.revision,
            });
        }
        if if undo {
            !self.history.can_undo()
        } else {
            !self.history.can_redo()
        } {
            return Ok(false);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(MwlDocumentControllerError::RevisionOverflow)?;
        let changed = if undo {
            self.history.undo(&mut self.value)
        } else {
            self.history.redo(&mut self.value)
        };
        debug_assert!(changed);
        self.revision = revision;
        Ok(true)
    }

    /// Reserves one immutable canonical save snapshot.
    ///
    /// # Errors
    ///
    /// Rejects overlapping saves, invalid programmatic state, and request-counter overflow.
    pub fn begin_save(&mut self) -> Result<MwlDocumentSaveSnapshot, MwlDocumentControllerError> {
        if self.pending_save.is_some() {
            return Err(MwlDocumentControllerError::SavePending);
        }
        let bytes = self
            .value
            .encode()
            .map_err(MwlDocumentControllerError::File)?;
        let canonical = MwlFile::decode(&bytes).map_err(MwlDocumentControllerError::File)?;
        if canonical != self.value {
            return Err(MwlDocumentControllerError::CanonicalMismatch);
        }
        let request_id = self.next_save_request;
        self.next_save_request = self
            .next_save_request
            .checked_add(1)
            .ok_or(MwlDocumentControllerError::SaveRequestOverflow)?;
        self.pending_save = Some(PendingSave {
            request_id,
            value: self.value.clone(),
        });
        Ok(MwlDocumentSaveSnapshot {
            request_id,
            revision: self.revision,
            path: self.path.clone(),
            bytes,
        })
    }

    /// Acknowledges exactly the snapshot that reached durable storage.
    ///
    /// # Errors
    ///
    /// A missing or stale acknowledgement leaves any mismatched pending save retryable.
    pub fn acknowledge_save(&mut self, request_id: u64) -> Result<(), MwlDocumentControllerError> {
        let pending = self
            .pending_save
            .take()
            .ok_or(MwlDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            let expected = pending.request_id;
            self.pending_save = Some(pending);
            return Err(MwlDocumentControllerError::StaleSave {
                expected,
                actual: request_id,
            });
        }
        self.saved = pending.value;
        Ok(())
    }

    /// Releases one failed persistence attempt without moving the saved baseline.
    ///
    /// # Errors
    ///
    /// Rejects missing or mismatched requests.
    pub fn cancel_save(&mut self, request_id: u64) -> Result<(), MwlDocumentControllerError> {
        let pending = self
            .pending_save
            .as_ref()
            .ok_or(MwlDocumentControllerError::NoPendingSave)?;
        if pending.request_id != request_id {
            return Err(MwlDocumentControllerError::StaleSave {
                expected: pending.request_id,
                actual: request_id,
            });
        }
        self.pending_save = None;
        Ok(())
    }
}

fn apply_edit(file: &mut MwlFile, edit: &MwlDocumentEdit) -> Result<(), MwlError> {
    match edit {
        MwlDocumentEdit::SetFlags(flags) => file.flags = *flags,
        MwlDocumentEdit::SetAttribution(attribution) => file.attribution = *attribution,
        MwlDocumentEdit::SetLevelNumber(level) => {
            let section = &mut file.sections[MwlSectionKind::LevelHeader as usize];
            let mut header = MwlLevelHeaderSection::decode(&section.bytes)?;
            header.set_level_number(*level);
            section.bytes = header.0.to_vec();
        }
        MwlDocumentEdit::SetMainEntrance(entrance) => {
            let section = &mut file.sections[MwlSectionKind::LevelHeader as usize];
            let mut header = MwlLevelHeaderSection::decode(&section.bytes)?;
            header.set_main_entrance(*entrance);
            section.bytes = header.0.to_vec();
        }
        MwlDocumentEdit::SetMidwayEntrance(entrance) => {
            let section = &mut file.sections[MwlSectionKind::LevelHeader as usize];
            let mut header = MwlLevelHeaderSection::decode(&section.bytes)?;
            header.set_midway_entrance(*entrance);
            section.bytes = header.0.to_vec();
        }
        MwlDocumentEdit::SetLayer2Scroll(settings) => {
            let section = &mut file.sections[MwlSectionKind::LevelHeader as usize];
            let mut header = MwlLevelHeaderSection::decode(&section.bytes)?;
            header.set_layer2_scroll_settings(*settings)?;
            section.bytes = header.0.to_vec();
        }
        MwlDocumentEdit::ReplaceSection { section, bytes } => {
            file.sections[*section as usize] = MwlSection {
                bytes: bytes.clone(),
            };
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MwlDocumentControllerError {
    File(MwlError),
    Edit {
        command: usize,
        error: MwlError,
    },
    OptionalAssets(MwlOptionalLevelAssetsError),
    OptionalEdit {
        command: usize,
        error: MwlOptionalAssetsEditError,
    },
    ExpandedSettings(ExpandedLevelSettingsError),
    SpriteSection(MwlError),
    SpriteParse(SpriteStreamError),
    SpriteEncoding(NativeSpriteEncodingError),
    SpriteCanonicalization(LevelEditError),
    NonCanonicalSprites,
    Layer1Section(MwlError),
    Layer1Parse(ObjectStreamError),
    Layer1Encoding(ObjectStreamError),
    NonCanonicalLayer1,
    CanonicalMismatch,
    StaleRevision {
        expected: u64,
        actual: u64,
    },
    RevisionOverflow,
    SavePending,
    SaveRequestOverflow,
    NoPendingSave,
    StaleSave {
        expected: u64,
        actual: u64,
    },
}

impl fmt::Display for MwlDocumentControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "MWL document controller failed: {self:?}")
    }
}

impl std::error::Error for MwlDocumentControllerError {}

#[cfg(test)]
#[path = "mwl_document_controller_tests.rs"]
mod tests;
