use lm_app::{AppState, Command};
use lm_profile::smw_us_v1_lunar_magic_metadata_layout;
use lm_rom::LunarMagicRomMetadata;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum MetadataRegion {
    Attribution,
    VramVersion,
    FeatureRecord,
}

pub(super) struct LunarMagicMetadataWorkspace {
    revision: u64,
    original: LunarMagicRomMetadata,
    current: LunarMagicRomMetadata,
}

impl LunarMagicMetadataWorkspace {
    pub(super) fn load(app: &AppState) -> Result<Self, String> {
        let metadata = app
            .project()
            .ok_or_else(|| "open a supported ROM first".to_owned())?
            .load_lunar_magic_rom_metadata(smw_us_v1_lunar_magic_metadata_layout())
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                "this ROM has no installed Lunar Magic attribution/feature metadata".to_owned()
            })?;
        Ok(Self {
            revision: app.project_revision(),
            original: metadata.clone(),
            current: metadata,
        })
    }

    pub(super) fn byte(&self, region: MetadataRegion, index: usize) -> Result<u8, String> {
        match region {
            MetadataRegion::Attribution => self.current.attribution().get(index).copied(),
            MetadataRegion::VramVersion => (index == 0).then(|| self.current.vram_version()),
            MetadataRegion::FeatureRecord => self.current.feature_record().get(index).copied(),
        }
        .ok_or_else(|| "metadata byte index is outside the selected region".to_owned())
    }

    pub(super) fn set_byte(
        &mut self,
        region: MetadataRegion,
        index: usize,
        value: u8,
    ) -> Result<(), String> {
        let mut attribution = *self.current.attribution();
        let mut vram_version = self.current.vram_version();
        let mut feature_record = *self.current.feature_record();
        match region {
            MetadataRegion::Attribution => {
                *attribution
                    .get_mut(index)
                    .ok_or_else(|| "attribution byte index must be 00–9F".to_owned())? = value;
            }
            MetadataRegion::VramVersion if index == 0 => vram_version = value,
            MetadataRegion::VramVersion => {
                return Err("VRAM-version byte index must be 00".to_owned());
            }
            MetadataRegion::FeatureRecord => {
                *feature_record
                    .get_mut(index)
                    .ok_or_else(|| "feature-record byte index must be 00–18".to_owned())? = value;
            }
        }
        self.current =
            LunarMagicRomMetadata::from_parts(&attribution, vram_version, &feature_record)
                .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(super) fn is_dirty(&self) -> bool {
        self.current != self.original
    }

    pub(super) const fn is_stale(&self, project_revision: u64) -> bool {
        self.revision != project_revision
    }

    pub(super) fn prepare_commit(&self, project_revision: u64) -> Result<Option<Command>, String> {
        if self.is_stale(project_revision) {
            return Err("stale Lunar Magic metadata workspace cannot be committed".into());
        }
        if !self.is_dirty() {
            return Ok(None);
        }
        Ok(Some(Command::ReplaceLunarMagicRomMetadata {
            rev: self.revision,
            metadata: Box::new(self.current.clone()),
        }))
    }

    pub(super) const fn metadata(&self) -> &LunarMagicRomMetadata {
        &self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lm_project::Project;
    use lm_rom::RomImage;
    use std::{fs, path::PathBuf};

    fn workspace() -> LunarMagicMetadataWorkspace {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture =
            fs::read(root.join("oracle-work/lm363/pristine-us/level-save-000/after.smc")).unwrap();
        let project = Project::open_supported(RomImage::from_bytes(fixture).unwrap()).unwrap();
        let metadata = project
            .load_lunar_magic_rom_metadata(smw_us_v1_lunar_magic_metadata_layout())
            .unwrap()
            .unwrap();
        LunarMagicMetadataWorkspace {
            revision: 7,
            original: metadata.clone(),
            current: metadata,
        }
    }

    #[test]
    fn opaque_byte_edit_is_lossless_and_revision_checked() {
        let mut workspace = workspace();
        let old = workspace.byte(MetadataRegion::Attribution, 0x9f).unwrap();
        workspace
            .set_byte(MetadataRegion::Attribution, 0x9f, old ^ 1)
            .unwrap();
        assert_eq!(
            workspace.byte(MetadataRegion::Attribution, 0x9f).unwrap(),
            old ^ 1
        );
        assert!(workspace.prepare_commit(8).is_err());
        assert!(workspace.prepare_commit(7).unwrap().is_some());
    }

    #[test]
    fn stable_signature_and_checksum_reserved_bits_cannot_be_corrupted() {
        let mut workspace = workspace();
        assert!(
            workspace
                .set_byte(MetadataRegion::Attribution, 0, b'X')
                .is_err()
        );
        assert!(
            workspace
                .set_byte(MetadataRegion::FeatureRecord, 0x18, 0xf0)
                .is_err()
        );
    }
}
