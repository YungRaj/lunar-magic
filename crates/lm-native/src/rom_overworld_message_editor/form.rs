use super::workspace::OverworldMessageWorkspace;
use crate::level_editor_forms::{parse_hex_u8, parse_hex_u16};
use lm_overworld::OverworldMessage;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct MessageTileForm {
    pub message: String,
    pub row: String,
    pub column: String,
    pub value: String,
    loaded: Option<(usize, usize, usize)>,
}

impl Default for MessageTileForm {
    fn default() -> Self {
        Self {
            message: "000".into(),
            row: "00".into(),
            column: "00".into(),
            value: "1F".into(),
            loaded: None,
        }
    }
}

impl MessageTileForm {
    fn selection(
        &self,
        workspace: &OverworldMessageWorkspace,
    ) -> Result<(usize, usize, usize), String> {
        let message = usize::from(parse_hex_u16(&self.message, "message")?);
        let row = usize::from(parse_hex_u8(&self.row, "row")?);
        let column = usize::from(parse_hex_u8(&self.column, "column")?);
        if message >= workspace.len() {
            return Err(format!(
                "message must be below the current count {:03X}",
                workspace.len()
            ));
        }
        if row >= OverworldMessage::ROWS {
            return Err("row must be 00–07".into());
        }
        if column >= OverworldMessage::COLUMNS {
            return Err("column must be 00–11".into());
        }
        Ok((message, row, column))
    }

    pub(super) fn load(&mut self, workspace: &OverworldMessageWorkspace) -> Result<(), String> {
        let selection = self.selection(workspace)?;
        self.value = format!("{:02X}", workspace.tile(selection));
        self.loaded = Some(selection);
        Ok(())
    }

    pub(super) fn apply(&self, workspace: &mut OverworldMessageWorkspace) -> Result<(), String> {
        let selection = self.selection(workspace)?;
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
    use lm_profile::SmwUsV1OverworldMessageStorage;

    #[test]
    fn selection_bounds_follow_current_variable_count() {
        let workspace = OverworldMessageWorkspace::for_test(194);
        let mut form = MessageTileForm {
            message: "0C2".into(),
            ..Default::default()
        };
        assert!(form.selection(&workspace).is_err());
        form.message = "0C1".into();
        form.row = "08".into();
        assert!(form.selection(&workspace).is_err());
        form.row = "00".into();
        form.column = "12".into();
        assert!(form.selection(&workspace).is_err());
        assert!(matches!(
            workspace.storage,
            SmwUsV1OverworldMessageStorage::Pristine
        ));
    }
}
