use crate::level_editor_forms;
use lm_level::{OscDirective, OscEntry, OscSidecar};

#[derive(Clone, Debug, Default)]
pub(crate) struct OscSourceForm {
    pub(crate) bytes: String,
}

impl OscSourceForm {
    pub(crate) fn load(value: &OscSidecar) -> Self {
        Self {
            bytes: level_editor_forms::format_bytes(value.source()),
        }
    }

    pub(crate) fn parse(&self) -> Result<Vec<u8>, String> {
        level_editor_forms::parse_bytes(&self.bytes, "OSC source byte")
    }
}

pub(crate) fn diagnostic(entry: &OscEntry) -> String {
    let selectors = entry
        .selectors
        .iter()
        .map(|value| {
            format!(
                "{:03X}/v{} p{:02X} {}×{} len={:?} alt={}",
                value.index,
                value.variant,
                value.parameter,
                value.width,
                value.height,
                value.record_length,
                value.alternate_linear
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let directive = match &entry.directive {
        OscDirective::Description(value) => format!("description {value:?}"),
        OscDirective::Display(value) => format!("{} display tiles", value.len()),
        OscDirective::Values(value) => format!("{} eight-word records", value.len()),
        OscDirective::Attributes(value) => format!("{} attribute bytes", value.len()),
    };
    format!(
        "raw flags {:08X}; selectors [{selectors}]; {directive}",
        entry.flags
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_preserves_non_utf8_and_diagnostic_exposes_selector_shape() {
        let source = b"bad\xff\n10\t2\t10002\t0,0,10\n";
        let value = OscSidecar::decode(source).unwrap();
        assert_eq!(OscSourceForm::load(&value).parse().unwrap(), source);
        let text = diagnostic(&value.entries()[0]);
        assert!(text.contains("00010002"));
        assert!(text.contains("len=Some(2)"));
    }
}
