use super::workspace::{LunarMagicMetadataWorkspace, MetadataRegion};
use crate::level_editor_forms::parse_hex_u8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct MetadataByteForm {
    pub region: MetadataRegion,
    pub index: String,
    pub value: String,
    loaded: Option<(MetadataRegion, usize)>,
}

impl Default for MetadataByteForm {
    fn default() -> Self {
        Self {
            region: MetadataRegion::Attribution,
            index: "00".into(),
            value: "00".into(),
            loaded: None,
        }
    }
}

impl MetadataByteForm {
    fn selection(&self) -> Result<(MetadataRegion, usize), String> {
        let index = usize::from(parse_hex_u8(&self.index, "metadata byte index")?);
        Ok((self.region, index))
    }

    pub(super) fn load(&mut self, workspace: &LunarMagicMetadataWorkspace) -> Result<(), String> {
        let selection = self.selection()?;
        self.value = format!("{:02X}", workspace.byte(selection.0, selection.1)?);
        self.loaded = Some(selection);
        Ok(())
    }

    pub(super) fn apply(&self, workspace: &mut LunarMagicMetadataWorkspace) -> Result<(), String> {
        let selection = self.selection()?;
        if self.loaded != Some(selection) {
            return Err("load the selected metadata byte before applying it".into());
        }
        workspace.set_byte(
            selection.0,
            selection.1,
            parse_hex_u8(&self.value, "metadata byte value")?,
        )
    }

    pub(super) fn selection_changed(&mut self) {
        self.loaded = None;
    }

    pub(super) fn clear_selection(&mut self) {
        self.loaded = None;
    }
}
