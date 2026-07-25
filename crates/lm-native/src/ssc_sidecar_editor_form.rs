use crate::level_editor_forms;
use lm_level::{SscDirective, SscEntry, SscSidecar};

#[derive(Clone, Debug, Default)]
pub(crate) struct SscSourceForm {
    pub(crate) bytes: String,
}

impl SscSourceForm {
    pub(crate) fn load(value: &SscSidecar) -> Self {
        Self {
            bytes: level_editor_forms::format_bytes(value.source()),
        }
    }

    pub(crate) fn parse(&self) -> Result<Vec<u8>, String> {
        level_editor_forms::parse_bytes(&self.bytes, "SSC source byte")
    }
}

pub(crate) fn diagnostic(entry: &SscEntry) -> String {
    let selector = entry.selector.map_or_else(
        || "global".into(),
        |value| {
            format!(
                "sprite {:02X}; index {:03X}; extra {}; dimensions {}×{}; length {:?}; alternate {}; global-slot {}",
                value.sprite_number,
                value.index,
                value.extra_bits,
                value.width,
                value.height,
                value.record_length,
                value.alternate,
                value.global_slot
            )
        },
    );
    let directive = match &entry.directive {
        SscDirective::Description(value) => format!("description {value:?}"),
        SscDirective::Display(value) => format!("{} display tiles", value.len()),
        SscDirective::Palette(value) => format!("{} palette records", value.len()),
        SscDirective::TileRemap { mode, ranges } => {
            format!("tile remap mode {mode}; {} ranges", ranges.len())
        }
        SscDirective::PaletteRemap(ranges) => format!("{} palette remap ranges", ranges.len()),
    };
    format!("{selector}; raw flags {:08X}; {directive}", entry.flags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_and_diagnostic_preserve_source_and_unknown_flags() {
        let source = b"bad\xff\n10\t800002\t0,0,10\n";
        let value = SscSidecar::decode(source).unwrap();
        assert_eq!(SscSourceForm::load(&value).parse().unwrap(), source);
        assert!(diagnostic(&value.entries()[0]).contains("00800002"));
    }
}
