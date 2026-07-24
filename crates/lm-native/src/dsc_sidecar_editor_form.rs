use crate::level_editor_forms;
use lm_level::{DscDirective, DscEntry, DscSidecar};

#[derive(Clone, Debug, Default)]
pub(crate) struct DscSourceForm {
    pub(crate) bytes: String,
}

impl DscSourceForm {
    pub(crate) fn load(value: &DscSidecar) -> Self {
        Self {
            bytes: level_editor_forms::format_bytes(value.source()),
        }
    }

    pub(crate) fn parse(&self) -> Result<Vec<u8>, String> {
        level_editor_forms::parse_bytes(&self.bytes, "DSC source byte")
    }
}

pub(crate) fn diagnostic(entry: &DscEntry) -> String {
    let directive = match &entry.directive {
        DscDirective::Description(value) => format!(
            "description {:?}; style b={:?} d={:?} f={:?} m={:?}",
            value.text, value.background, value.detail, value.foreground, value.mode
        ),
        DscDirective::DisplayMapping(value) => format!("display mapping {value:04X}"),
        DscDirective::AlternateMapping(value) => format!("alternate mapping {value:04X}"),
    };
    format!(
        "key {:04X}; raw flags {:08X}; {directive}",
        entry.key, entry.flags
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_form_preserves_non_utf8_and_malformed_lines() {
        let source = b"bad\xffline\n0001\t0000\tvalid\n";
        let value = DscSidecar::decode(source).unwrap();
        assert_eq!(DscSourceForm::load(&value).parse().unwrap(), source);
        assert_eq!(value.entries().len(), 1);
    }

    #[test]
    fn diagnostic_retains_unknown_flags() {
        let value = DscSidecar::decode(b"0001\t80000000\ttext\n").unwrap();
        assert!(diagnostic(&value.entries()[0]).contains("80000000"));
    }
}
