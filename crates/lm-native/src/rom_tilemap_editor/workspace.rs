use lm_app::{AppState, Command};
use lm_overworld::{CreditsTilemap, ExpandedLayerTilemap};
use lm_profile::{
    SMW_US_V1_CHECKSUM_FIELD, smw_us_v1_credits_allocation_policy,
    smw_us_v1_title_tilemap_allocation_policy,
};
use lm_profile::{smw_us_v1_credits_tilemap_locator, smw_us_v1_title_tilemap_locator};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TilemapKind {
    Title,
    Credits,
}

impl TilemapKind {
    pub(super) const fn title(self) -> &'static str {
        match self {
            Self::Title => "Title-Screen Tilemap",
            Self::Credits => "Credits Tilemap",
        }
    }

    pub(super) const fn rows(self) -> usize {
        match self {
            Self::Title => ExpandedLayerTilemap::ROWS,
            Self::Credits => CreditsTilemap::ROWS,
        }
    }

    pub(super) const fn columns(self) -> usize {
        match self {
            Self::Title => ExpandedLayerTilemap::COLUMNS,
            Self::Credits => CreditsTilemap::COLUMNS,
        }
    }

    pub(super) const fn planes(self) -> usize {
        match self {
            Self::Title => 2,
            Self::Credits => 1,
        }
    }
}

pub(super) enum TilemapData {
    Title(Box<ExpandedLayerTilemap>),
    Credits(CreditsTilemap),
}

pub(super) struct TilemapWorkspace {
    pub revision: u64,
    original: TilemapData,
    current: TilemapData,
}

impl TilemapWorkspace {
    pub(super) fn open(kind: TilemapKind, app: &AppState) -> Result<Self, String> {
        let project = app
            .project()
            .ok_or_else(|| "open a supported ROM first".to_owned())?;
        let data = match kind {
            TilemapKind::Title => TilemapData::Title(Box::new(
                project
                    .load_title_tilemap_detected(smw_us_v1_title_tilemap_locator())
                    .map_err(|error| error.to_string())?
                    .tilemap,
            )),
            TilemapKind::Credits => TilemapData::Credits(
                project
                    .load_credits_tilemap_detected(&smw_us_v1_credits_tilemap_locator())
                    .map_err(|error| error.to_string())?
                    .tilemap,
            ),
        };
        Ok(Self {
            revision: app.project_revision(),
            original: data.clone(),
            current: data,
        })
    }

    pub(super) const fn kind(&self) -> TilemapKind {
        match self.current {
            TilemapData::Title(_) => TilemapKind::Title,
            TilemapData::Credits(_) => TilemapKind::Credits,
        }
    }

    pub(super) fn is_dirty(&self) -> bool {
        self.current != self.original
    }

    pub(super) fn staged_recovery_generation(&self, app: &AppState) -> Option<u64> {
        self.is_dirty().then(|| {
            let content_revision = match &self.current {
                TilemapData::Title(tilemap) => tilemap
                    .primary_bytes()
                    .iter()
                    .chain(tilemap.secondary_bytes())
                    .fold(0x5449_544c_4554_494c_u64, |revision, byte| {
                        revision.rotate_left(5) ^ u64::from(*byte)
                    }),
                TilemapData::Credits(tilemap) => tilemap
                    .words()
                    .iter()
                    .fold(0x4352_4544_5449_4c45_u64, |revision, word| {
                        revision.rotate_left(5) ^ u64::from(*word)
                    }),
            };
            app.project_revision().wrapping_mul(0xd6e8_feb8_6659_fd93)
                ^ self.revision.rotate_left(23)
                ^ content_revision
        })
    }

    pub(super) fn staged_recovery_snapshot(
        &self,
        app: &AppState,
    ) -> Result<Option<lm_app::RecoverySnapshot>, String> {
        if self.revision != app.project_revision() {
            return Err("stale tilemap workspace cannot be recovered".into());
        }
        if !self.is_dirty() {
            return Ok(app.recovery_snapshot());
        }
        let mut staged = app
            .project()
            .ok_or_else(|| "open a supported ROM first".to_owned())?
            .clone();
        match &self.current {
            TilemapData::Title(tilemap) => staged
                .save_title_tilemap_detected(
                    tilemap,
                    smw_us_v1_title_tilemap_locator(),
                    &smw_us_v1_title_tilemap_allocation_policy(staged.rom.logical_len()),
                    SMW_US_V1_CHECKSUM_FIELD,
                    0xff,
                )
                .map_err(|error| error.to_string())?,
            TilemapData::Credits(tilemap) => staged
                .save_credits_tilemap_detected(
                    tilemap,
                    &smw_us_v1_credits_tilemap_locator(),
                    &smw_us_v1_credits_allocation_policy(staged.rom.logical_len()),
                    SMW_US_V1_CHECKSUM_FIELD,
                    0xff,
                )
                .map_err(|error| error.to_string())?,
        };
        app.recovery_snapshot_with_current_rom(staged.save_snapshot(), app.current_level())
            .map_err(|error| error.to_string())
    }

    pub(super) fn word(&self, selection: (usize, usize, usize)) -> Result<u16, String> {
        let (plane, row, column) = selection;
        let index = checked_index(self.kind(), plane, row, column)?;
        match &self.current {
            TilemapData::Title(tilemap) => {
                let bytes = if plane == 0 {
                    tilemap.primary_bytes()
                } else {
                    tilemap.secondary_bytes()
                };
                Ok(u16::from_le_bytes([bytes[index * 2], bytes[index * 2 + 1]]))
            }
            TilemapData::Credits(tilemap) => Ok(tilemap.words()[index]),
        }
    }

    pub(super) fn set_word(
        &mut self,
        selection: (usize, usize, usize),
        value: u16,
    ) -> Result<(), String> {
        let (plane, row, column) = selection;
        let index = checked_index(self.kind(), plane, row, column)?;
        match &mut self.current {
            TilemapData::Title(tilemap) => {
                let bytes = if plane == 0 {
                    tilemap.primary_bytes_mut()
                } else {
                    tilemap.secondary_bytes_mut()
                };
                bytes[index * 2..index * 2 + 2].copy_from_slice(&value.to_le_bytes());
            }
            TilemapData::Credits(tilemap) => tilemap.words_mut()[index] = value,
        }
        Ok(())
    }

    pub(super) fn command(&self, project_revision: u64) -> Result<Option<Command>, String> {
        if self.revision != project_revision {
            return Err("stale tilemap workspace cannot be committed".into());
        }
        if !self.is_dirty() {
            return Ok(None);
        }
        Ok(Some(match &self.current {
            TilemapData::Title(tilemap) => Command::ReplaceNativeTitleTilemap {
                rev: self.revision,
                tilemap: Box::new(tilemap.as_ref().clone()),
            },
            TilemapData::Credits(tilemap) => Command::ReplaceNativeCreditsTilemap {
                rev: self.revision,
                tilemap: Box::new(tilemap.clone()),
            },
        }))
    }

    #[cfg(test)]
    pub(super) fn blank_for_test(kind: TilemapKind) -> Self {
        let data = match kind {
            TilemapKind::Title => TilemapData::Title(Box::default()),
            TilemapKind::Credits => TilemapData::Credits(CreditsTilemap::blank(0x38fc)),
        };
        Self {
            revision: 0,
            original: data.clone(),
            current: data,
        }
    }
}

impl Clone for TilemapData {
    fn clone(&self) -> Self {
        match self {
            Self::Title(tilemap) => Self::Title(tilemap.clone()),
            Self::Credits(tilemap) => Self::Credits(tilemap.clone()),
        }
    }
}

impl PartialEq for TilemapData {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Title(left), Self::Title(right)) => left == right,
            (Self::Credits(left), Self::Credits(right)) => left == right,
            _ => false,
        }
    }
}

fn checked_index(
    kind: TilemapKind,
    plane: usize,
    row: usize,
    column: usize,
) -> Result<usize, String> {
    if plane >= kind.planes() || row >= kind.rows() || column >= kind.columns() {
        return Err("tilemap coordinate is outside the selected native shape".into());
    }
    row.checked_mul(kind.columns())
        .and_then(|base| base.checked_add(column))
        .ok_or_else(|| "tilemap coordinate overflow".into())
}
