use crate::level_editor_forms;
use lm_app::NativeMap16SidecarEdit;

#[derive(Clone, Debug, Default)]
pub(crate) struct NativeMap16SidecarForm {
    pub(crate) entry: usize,
    pub(crate) value: String,
}

impl NativeMap16SidecarForm {
    pub(crate) fn load(entry: usize, value: u32) -> Self {
        Self {
            entry,
            value: format!("{value:08X}"),
        }
    }

    pub(crate) fn edit(&self) -> Result<NativeMap16SidecarEdit, String> {
        Ok(NativeMap16SidecarEdit {
            entry: self.entry,
            value: level_editor_forms::parse_hex_u32(&self.value, "raw Map16 sidecar entry")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_retains_full_raw_dword() {
        let form = NativeMap16SidecarForm::load(0x123, 0xfedc_ba98);
        assert_eq!(
            form.edit().unwrap(),
            NativeMap16SidecarEdit {
                entry: 0x123,
                value: 0xfedc_ba98
            }
        );
    }
}
