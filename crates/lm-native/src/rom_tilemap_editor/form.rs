use super::workspace::{TilemapKind, TilemapWorkspace};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TileForm {
    pub row: String,
    pub column: String,
    pub plane: usize,
    pub value: String,
    loaded: Option<(usize, usize, usize)>,
}

impl Default for TileForm {
    fn default() -> Self {
        Self {
            row: "00".into(),
            column: "00".into(),
            plane: 0,
            value: "0000".into(),
            loaded: None,
        }
    }
}

impl TileForm {
    pub(super) fn selection(&self, kind: TilemapKind) -> Result<(usize, usize, usize), String> {
        let row = parse_hex(&self.row, "row")?;
        let column = parse_hex(&self.column, "column")?;
        if row >= kind.rows() {
            return Err(format!(
                "row {row:#x} exceeds the final row {:#x}",
                kind.rows() - 1
            ));
        }
        if column >= kind.columns() {
            return Err(format!(
                "column {column:#x} exceeds the final column {:#x}",
                kind.columns() - 1
            ));
        }
        if self.plane >= kind.planes() {
            return Err(format!(
                "plane {} exceeds the final plane {}",
                self.plane,
                kind.planes() - 1
            ));
        }
        Ok((self.plane, row, column))
    }

    pub(super) fn load(&mut self, workspace: &TilemapWorkspace) -> Result<(), String> {
        let selection = self.selection(workspace.kind())?;
        self.value = format!("{:04X}", workspace.word(selection)?);
        self.loaded = Some(selection);
        Ok(())
    }

    pub(super) fn apply(&mut self, workspace: &mut TilemapWorkspace) -> Result<(), String> {
        let selection = self.selection(workspace.kind())?;
        if self.loaded != Some(selection) {
            return Err("load the selected tile before applying it".into());
        }
        let value = parse_hex_u16(&self.value, "tile word")?;
        workspace.set_word(selection, value)
    }

    pub(super) fn selection_changed(&mut self) {
        self.loaded = None;
    }
}

fn parse_hex(value: &str, name: &str) -> Result<usize, String> {
    if value.is_empty() || value.len() > 4 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "{name} must contain one to four hexadecimal digits"
        ));
    }
    usize::from_str_radix(value, 16).map_err(|_| format!("{name} is not hexadecimal"))
}

fn parse_hex_u16(value: &str, name: &str) -> Result<u16, String> {
    let parsed = parse_hex(value, name)?;
    u16::try_from(parsed).map_err(|_| format!("{name} exceeds FFFF"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_bounds_and_loaded_identity_are_explicit() {
        let mut workspace = TilemapWorkspace::blank_for_test(TilemapKind::Title);
        let mut form = TileForm::default();
        form.load(&workspace).unwrap();
        form.value = "1234".into();
        form.apply(&mut workspace).unwrap();
        assert_eq!(workspace.word((0, 0, 0)).unwrap(), 0x1234);
        form.column = "20".into();
        form.selection_changed();
        assert!(form.load(&workspace).is_err());
        form.column = "01".into();
        assert!(form.apply(&mut workspace).is_err());
        form.plane = 2;
        assert!(form.selection(TilemapKind::Title).is_err());
    }
}
