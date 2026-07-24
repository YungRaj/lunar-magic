use crate::{DscDescription, DscDirective, DscSidecar};

pub const DSC_ENTRY_COUNT: usize = 0x8000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DscDescriptionStyle {
    pub background: u32,
    pub detail: u32,
    pub foreground: u32,
    pub mode: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DscResolvedEntry {
    pub description: Option<DscDescription>,
    pub display_mapping: Option<u16>,
    pub alternate_mapping: Option<u16>,
    /// Native flag byte: description bits `1`/`8`, mapping-mode bits `2`/`4`.
    pub native_flags: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DscResolvedTable {
    entries: Vec<DscResolvedEntry>,
}

impl DscResolvedTable {
    #[must_use]
    pub fn from_sidecar(sidecar: &DscSidecar, defaults: DscDescriptionStyle) -> Self {
        let mut entries = vec![DscResolvedEntry::default(); DSC_ENTRY_COUNT];
        for source in sidecar.entries() {
            match &source.directive {
                DscDirective::Description(description) => {
                    let start = if source.flags & 1 != 0 {
                        usize::from(source.key & 0xff00)
                    } else {
                        usize::from(source.key)
                    };
                    let end = if source.flags & 1 != 0 {
                        start + 0x100
                    } else {
                        start + 1
                    };
                    let mut resolved = description.clone();
                    resolved.background.get_or_insert(defaults.background);
                    resolved.detail.get_or_insert(defaults.detail);
                    resolved.foreground.get_or_insert(defaults.foreground);
                    resolved.mode.get_or_insert(defaults.mode);
                    for entry in &mut entries[start..end] {
                        entry.description = Some(resolved.clone());
                        if source.flags & 8 != 0 {
                            entry.native_flags |= 1;
                        }
                        if source.flags & 0x20 != 0 {
                            entry.native_flags |= 8;
                        }
                    }
                }
                DscDirective::DisplayMapping(mapping) => {
                    let entry = &mut entries[usize::from(source.key)];
                    entry.display_mapping = Some(*mapping);
                    if source.flags & 2 != 0 {
                        entry.native_flags |= 4;
                    }
                    if source.flags & 4 != 0 {
                        entry.native_flags |= 2;
                    }
                }
                DscDirective::AlternateMapping(mapping) => {
                    entries[usize::from(source.key)].alternate_mapping = Some(*mapping);
                }
            }
        }
        Self { entries }
    }

    #[must_use]
    pub fn get(&self, key: u16) -> Option<&DscResolvedEntry> {
        self.entries.get(usize::from(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULTS: DscDescriptionStyle = DscDescriptionStyle {
        background: 1,
        detail: 2,
        foreground: 3,
        mode: 4,
    };

    #[test]
    fn page_descriptions_expand_and_later_records_replace_them() {
        let source = DscSidecar::decode(b"1234\t29\tpage\n127f\t28\tone\\b000009\n").unwrap();
        let table = DscResolvedTable::from_sidecar(&source, DEFAULTS);
        assert_eq!(
            table
                .get(0x1200)
                .unwrap()
                .description
                .as_ref()
                .unwrap()
                .text,
            "page"
        );
        let one = table.get(0x127f).unwrap();
        assert_eq!(one.description.as_ref().unwrap().text, "one");
        assert_eq!(one.description.as_ref().unwrap().background, Some(9));
        assert_eq!(one.native_flags, 9);
    }

    #[test]
    fn mapping_flags_match_native_flag_byte() {
        let source = DscSidecar::decode(b"10\t6\t1234\n10\t10\t2345\n").unwrap();
        let table = DscResolvedTable::from_sidecar(&source, DEFAULTS);
        let entry = table.get(0x10).unwrap();
        assert_eq!(entry.display_mapping, Some(0x1234));
        assert_eq!(entry.alternate_mapping, Some(0x2345));
        assert_eq!(entry.native_flags, 6);
    }
}
