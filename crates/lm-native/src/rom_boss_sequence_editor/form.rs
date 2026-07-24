use super::workspace::BossSequenceWorkspace;
use crate::level_editor_forms::parse_hex_u8;
use lm_overworld::{BossSequenceMessage, BossSequenceMessageTable};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct BossTileForm {
    pub message: String,
    pub row: String,
    pub column: String,
    pub value: String,
    loaded: Option<(usize, usize, usize)>,
}

impl Default for BossTileForm {
    fn default() -> Self {
        Self {
            message: "00".into(),
            row: "00".into(),
            column: "00".into(),
            value: "1F".into(),
            loaded: None,
        }
    }
}

impl BossTileForm {
    fn selection(&self) -> Result<(usize, usize, usize), String> {
        let message = usize::from(parse_hex_u8(&self.message, "message")?);
        let row = usize::from(parse_hex_u8(&self.row, "row")?);
        let column = usize::from(parse_hex_u8(&self.column, "column")?);
        if message >= BossSequenceMessageTable::MESSAGE_COUNT {
            return Err("message must be 00–06".into());
        }
        if row >= BossSequenceMessage::ROWS {
            return Err("row must be 00–07".into());
        }
        if column >= BossSequenceMessage::COLUMNS {
            return Err("column must be 00–17".into());
        }
        Ok((message, row, column))
    }

    pub(super) fn load(&mut self, workspace: &BossSequenceWorkspace) -> Result<(), String> {
        let selection = self.selection()?;
        self.value = format!("{:02X}", workspace.tile(selection));
        self.loaded = Some(selection);
        Ok(())
    }

    pub(super) fn apply(&self, workspace: &mut BossSequenceWorkspace) -> Result<(), String> {
        let selection = self.selection()?;
        if self.loaded != Some(selection) {
            return Err("load the selected tile before applying it".into());
        }
        workspace.set_tile(selection, parse_hex_u8(&self.value, "tile value")?)
    }

    pub(super) fn selection_changed(&mut self) {
        self.loaded = None;
    }

    pub(super) fn clear_selection(&mut self) {
        self.loaded = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_and_loaded_identity_are_enforced() {
        let mut form = BossTileForm {
            message: "07".into(),
            ..Default::default()
        };
        assert!(form.selection().is_err());
        form.message = "00".into();
        form.row = "08".into();
        assert!(form.selection().is_err());
        form.row = "00".into();
        form.column = "18".into();
        assert!(form.selection().is_err());
    }
}
