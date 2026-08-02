use lm_app::{AppState, Command};
use lm_rom::{CopierHeader, RomImage};

pub(super) struct CopierHeaderWorkspace {
    revision: u64,
    current: CopierHeader,
    target: CopierHeader,
    logical_len: usize,
    canonical_lunar_magic: bool,
}

impl CopierHeaderWorkspace {
    pub(super) fn load(app: &AppState) -> Result<Self, String> {
        let snapshot = app
            .controller_snapshot()
            .map_err(|error| error.to_string())?;
        let image = RomImage::from_bytes(snapshot.rom_bytes).map_err(|error| error.to_string())?;
        let current = image.copier_header();
        Ok(Self {
            revision: snapshot.revision,
            current,
            target: match current {
                CopierHeader::Absent => CopierHeader::Present,
                CopierHeader::Present => CopierHeader::Absent,
            },
            logical_len: image.logical_len(),
            canonical_lunar_magic: image.copier_header_bytes()
                == Some(lm_profile::smw_us_v1_lunar_magic_copier_header().as_slice()),
        })
    }

    pub(super) const fn current(&self) -> CopierHeader {
        self.current
    }

    pub(super) const fn target(&self) -> CopierHeader {
        self.target
    }

    pub(super) const fn target_mut(&mut self) -> &mut CopierHeader {
        &mut self.target
    }

    pub(super) const fn logical_len(&self) -> usize {
        self.logical_len
    }

    pub(super) const fn canonical_lunar_magic(&self) -> bool {
        self.canonical_lunar_magic
    }

    pub(super) fn prepare_lunar_magic_canonical(
        &self,
        current_revision: u64,
    ) -> Result<Command, String> {
        if current_revision != self.revision {
            return Err(format!(
                "ROM changed while this dialog was open (expected revision {}, current {})",
                self.revision, current_revision
            ));
        }
        if self.canonical_lunar_magic {
            return Err("the open ROM already has Lunar Magic's canonical SMW header".into());
        }
        Ok(Command::SetLunarMagicSmwUsCopierHeader { rev: self.revision })
    }

    pub(super) fn prepare(&self, current_revision: u64, fill: u8) -> Result<Command, String> {
        if current_revision != self.revision {
            return Err(format!(
                "ROM changed while this dialog was open (expected revision {}, current {})",
                self.revision, current_revision
            ));
        }
        if self.target == self.current {
            return Err("target copier-header state already matches the open ROM".into());
        }
        Ok(Command::SetCopierHeader {
            rev: self.revision,
            target: self.target,
            fill,
        })
    }
}
